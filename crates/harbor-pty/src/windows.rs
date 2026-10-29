mod conpty;

use conpty::ConptyApi;
use std::{
    ffi::{OsStr, OsString},
    io,
    mem::size_of,
    os::windows::{ffi::OsStrExt, io::AsRawHandle},
    sync::{Arc, Mutex, mpsc},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use ::windows::{
    Win32::{
        Foundation::{
            CloseHandle, ERROR_NOT_FOUND, ERROR_OPERATION_ABORTED, HANDLE, WAIT_FAILED,
            WAIT_TIMEOUT,
        },
        Storage::FileSystem::{ReadFile, WriteFile},
        System::{
            Console::{COORD, HPCON},
            IO::CancelSynchronousIo,
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject, TerminateJobObject,
            },
            Pipes::CreatePipe,
            Threading::{
                CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
                DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT,
                InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION, ResumeThread,
                STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute,
                WaitForSingleObject,
            },
        },
    },
    core::{HRESULT, PCWSTR, PWSTR},
};
use anyhow::{Context as _, ensure};

use crate::{PtySize, ReaderShutdown, ShellCommand};

/// Windows ConPTY session and the handles that must outlive the shell process.
pub struct Pty {
    /// Input write end retained so ConPTY keeps stdin open for the child.
    _input_write: Option<OwnedHandle>,
    /// ConPTY handle; must outlive the process attached through the attribute list.
    _pseudo_console: Option<PseudoConsole>,
    /// Shell process handle retained for lifetime ownership.
    _process: Option<OwnedHandle>,
    /// Primary thread handle returned with the process handle.
    _thread: Option<OwnedHandle>,
    /// Windows Job Object used to ensure pwsh and cmd processes exit atomically together.
    _job: Option<OwnedHandle>,
    /// Whether an explicit job termination request has already been issued.
    terminated: bool,
}

/// Read side of the ConPTY output pipe consumed by the background pump.
pub struct PtyReader {
    /// Output read end consumed by `PtyReader::read`.
    output_read: OwnedHandle,
}

/// Write-only input endpoint transferred to the terminal UI thread.
pub struct PtyWriter {
    input_write: OwnedHandle,
}

impl Pty {
    pub fn spawn_shell(
        size: PtySize,
        command: &ShellCommand,
    ) -> anyhow::Result<(Self, PtyReader, String)> {
        ensure!(size.rows > 0 && size.cols > 0, "pty size must be positive");
        tracing::info!(rows = size.rows, cols = size.cols, "creating windows pty");

        // 1. Create and configure Job Object
        let job = unsafe { CreateJobObjectW(None, PCWSTR::null()) }
            .context("failed to create job object")?;
        let job = OwnedHandle::new(job);

        let mut limit_info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limit_info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let set_job = unsafe {
            SetInformationJobObject(
                job.handle(),
                JobObjectExtendedLimitInformation,
                &limit_info as *const _ as *const _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if let Err(err) = set_job {
            return Err(anyhow::anyhow!(
                "failed to set job object information: {:?}",
                err
            ));
        }

        let (input_read, input_write) =
            OwnedHandle::pipe().context("failed to create pty input pipe")?;
        let (output_read, output_write) =
            OwnedHandle::pipe().context("failed to create pty output pipe")?;
        tracing::info!("created pty pipes");

        let pseudo_console =
            PseudoConsole::create(size, input_read.handle(), output_write.handle())?;
        tracing::info!("created pseudo console");
        let attribute_list = AttributeList::with_pseudo_console(pseudo_console.handle())?;
        let (process_info, shell_name) = create_shell_process(&attribute_list, command)?;
        tracing::info!("created shell process");

        // The suspended process cannot create children before Job assignment succeeds.
        let assign = unsafe { AssignProcessToJobObject(job.handle(), process_info.hProcess) };
        if let Err(err) = assign {
            terminate_process(&process_info);
            return Err(anyhow::anyhow!(
                "failed to assign process to job object: {:?}",
                err
            ));
        }

        // Assignment is complete, so the shell and every subsequently-created child belong to
        // the Job before shell startup code executes.
        if unsafe { ResumeThread(process_info.hThread) } == u32::MAX {
            let err = ::windows::core::Error::from_thread();
            terminate_process(&process_info);
            return Err(anyhow::anyhow!("failed to resume shell process: {err:?}"));
        }

        // The pseudo console owns the child-side pipe handles after process creation.
        // Dropping our duplicates makes EOF observable when the shell exits.
        drop(input_read);
        drop(output_write);

        Ok((
            Self {
                _input_write: Some(input_write),
                _pseudo_console: Some(pseudo_console),
                _process: Some(OwnedHandle::new(process_info.hProcess)),
                _thread: Some(OwnedHandle::new(process_info.hThread)),
                _job: Some(job),
                terminated: false,
            },
            PtyReader { output_read },
            shell_name,
        ))
    }

    /// Separates the input pipe from the resources that must remain alive until
    /// the terminal reader has stopped. This keeps ConPTY ownership inside the
    /// PTY crate while allowing the terminal to own its reader thread.
    pub(crate) fn into_endpoints(mut self, reader: PtyReader) -> (PtyReader, PtyWriter, Self) {
        let input_write = self
            ._input_write
            .take()
            .expect("live pty must retain its input write handle");
        (reader, PtyWriter { input_write }, self)
    }

    pub fn resize(&mut self, size: PtySize) -> anyhow::Result<()> {
        ensure!(size.rows > 0 && size.cols > 0, "pty size must be positive");
        tracing::info!(rows = size.rows, cols = size.cols, "resizing windows pty");
        self._pseudo_console.as_mut().unwrap().resize(size)?;
        Ok(())
    }

    /// Starts termination of the shell process tree without blocking the caller.
    fn terminate(&mut self) {
        if self.terminated {
            return;
        }
        self.terminated = true;

        if let Some(job) = &self._job
            && let Err(err) = unsafe { TerminateJobObject(job.handle(), 0xcfffffff) }
        {
            tracing::error!(error = ?err, "failed to terminate job object");
        }
    }

    /// Waits for the terminated Job only from the shutdown worker.
    fn wait_for_exit(&self) {
        if !self.terminated {
            return;
        }

        if let Some(job) = &self._job {
            unsafe {
                match WaitForSingleObject(job.handle(), 5000) {
                    WAIT_TIMEOUT => {
                        tracing::warn!("WaitForSingleObject on Job timed out after 5000ms");
                    }
                    WAIT_FAILED => {
                        tracing::error!(
                            error = ?::windows::core::Error::from_thread(),
                            "WaitForSingleObject on Job failed"
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    /// Transfers the complete shutdown ownership graph to one worker. The worker owns both the
    /// reader and ConPTY until the reader acknowledges that it has released `output_read`.
    pub(crate) fn shutdown(pty: Self, reader: JoinHandle<()>, reader_shutdown: ReaderShutdown) {
        let shutdown_work = Arc::new(Mutex::new(Some((pty, reader, reader_shutdown))));
        let worker_work = Arc::clone(&shutdown_work);
        let worker = std::thread::Builder::new()
            .name("harbor-pty-shutdown".into())
            .spawn(move || {
                let (mut pty, reader, reader_shutdown) = worker_work
                    .lock()
                    .expect("pty shutdown work lock poisoned")
                    .take()
                    .expect("pty shutdown worker started without work");
                pty.terminate();
                reader_shutdown.request_stop();

                if Self::wait_for_reader(
                    &reader,
                    &reader_shutdown,
                    Instant::now() + Duration::from_secs(2),
                ) {
                    Self::finish_shutdown(pty, reader);
                } else {
                    tracing::error!(
                        "pty reader did not acknowledge shutdown within 2s; deferring to reaper"
                    );
                    Self::defer_to_reaper(pty, reader, reader_shutdown);
                }
            });

        if let Err(error) = worker {
            // Closing ConPTY while its reader may still use output_read is unsafe. Preserve the
            // ownership graph for process teardown rather than reintroducing a UI-thread wait.
            tracing::error!(
                ?error,
                "failed to spawn pty shutdown worker; leaking session"
            );
            let work = shutdown_work
                .lock()
                .expect("pty shutdown work lock poisoned")
                .take()
                .expect("pty shutdown work lost before worker startup");
            std::mem::forget(work);
        }
    }

    fn wait_for_reader(
        reader: &JoinHandle<()>,
        reader_shutdown: &ReaderShutdown,
        deadline: Instant,
    ) -> bool {
        loop {
            if let Err(error) = Self::cancel_reader(reader.as_raw_handle()) {
                tracing::error!(
                    ?error,
                    "failed to interrupt terminal reader during shutdown"
                );
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            if reader_shutdown.wait_for_completion(remaining.min(Duration::from_millis(10))) {
                return true;
            }
        }
    }

    /// Runs only after the reader's completion acknowledgement was observed.
    fn finish_shutdown(pty: Self, reader: JoinHandle<()>) {
        // The acknowledgement is emitted only after `PtyReader` has been dropped, so this join
        // cannot wait for another ReadFile and remains off the event-loop thread.
        let _ = reader.join();
        pty.wait_for_exit();
        drop(pty);
    }

    fn defer_to_reaper(pty: Self, reader: JoinHandle<()>, reader_shutdown: ReaderShutdown) {
        let reaper_work = Arc::new(Mutex::new(Some((pty, reader, reader_shutdown))));
        let worker_work = Arc::clone(&reaper_work);
        let reaper = std::thread::Builder::new()
            .name("harbor-pty-reaper".into())
            .spawn(move || {
                let (pty, reader, reader_shutdown) = worker_work
                    .lock()
                    .expect("pty reaper work lock poisoned")
                    .take()
                    .expect("pty reaper started without work");

                // The reaper retains the complete graph until the same acknowledgement is
                // observed, then performs the only JoinHandle::join call.
                while !reader_shutdown.wait_for_completion(Duration::from_millis(10)) {
                    if let Err(error) = Self::cancel_reader(reader.as_raw_handle()) {
                        tracing::error!(?error, "failed to interrupt terminal reader in reaper");
                    }
                }
                Self::finish_shutdown(pty, reader);
            });

        if let Err(error) = reaper {
            tracing::error!(?error, "failed to spawn pty reaper; leaking session");
            let work = reaper_work
                .lock()
                .expect("pty reaper work lock poisoned")
                .take()
                .expect("pty reaper work lost before startup");
            std::mem::forget(work);
        }
    }

    /// Requests cancellation of the reader's current synchronous I/O operation.
    /// `ERROR_NOT_FOUND` means the reader is between reads and is not an error.
    pub(crate) fn interrupt_reader(&self, reader: &JoinHandle<()>) -> anyhow::Result<()> {
        Self::cancel_reader(reader.as_raw_handle())
    }

    fn cancel_reader(reader_handle: std::os::windows::io::RawHandle) -> anyhow::Result<()> {
        unsafe {
            if let Err(error) = CancelSynchronousIo(HANDLE(reader_handle as *mut _)) {
                if error.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0) {
                    tracing::debug!("reader had no synchronous I/O to cancel");
                } else {
                    return Err(error).context("failed to cancel terminal reader I/O");
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn reader_io_error(error: anyhow::Error) -> io::Error {
    if error
        .downcast_ref::<::windows::core::Error>()
        .is_some_and(|error| error.code() == HRESULT::from_win32(ERROR_OPERATION_ABORTED.0))
    {
        io::Error::from(io::ErrorKind::Interrupted)
    } else {
        io::Error::other(error)
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        self.terminate();

        // Re-order field dropping explicitly to ensure all handle references (especially process/thread/job)
        // are closed before ClosePseudoConsole is invoked.
        if self._job.is_some() {
            drop(self._job.take());
        }
        if self._process.is_some() {
            drop(self._process.take());
        }
        if self._thread.is_some() {
            drop(self._thread.take());
        }
        if self._input_write.is_some() {
            drop(self._input_write.take());
        }
        if self._pseudo_console.is_some() {
            drop(self._pseudo_console.take());
        }
    }
}

fn create_shell_process(
    attribute_list: &AttributeList,
    command: &ShellCommand,
) -> anyhow::Result<(PROCESS_INFORMATION, String)> {
    let mut startup_info = STARTUPINFOEXW::default();
    startup_info.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup_info.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup_info.lpAttributeList = attribute_list.as_ptr();

    let environment_block = build_environment_block(&command.environment)?;
    let directory = command.directory.as_ref().map(|path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    });
    if let Some(path) = &command.directory {
        ensure!(
            path.is_absolute(),
            "shell.directory must be absolute at child creation"
        );
        ensure!(
            !path.as_os_str().encode_wide().any(|c| c == 0),
            "shell.directory contains NUL"
        );
        ensure!(
            path.is_dir(),
            "shell.directory is unavailable or not a directory at child creation"
        );
    }
    create_shell_process_with(command, std::env::var_os("COMSPEC"), |program, args| {
        let mut command_line = shell_command_line(program, args);
        try_create_shell_process(
            &startup_info,
            &mut command_line,
            &environment_block,
            directory.as_deref(),
        )
        .map_err(anyhow::Error::from)
    })
}

/// Try actual process creation rather than inferring availability from PATH.
/// The returned title comes from the executable that successfully started.
fn create_shell_process_with<T>(
    command: &ShellCommand,
    comspec: Option<OsString>,
    mut create: impl FnMut(&OsStr, &[String]) -> anyhow::Result<T>,
) -> anyhow::Result<(T, String)> {
    let mut failures = Vec::new();
    let configured_program = command
        .program
        .as_deref()
        .filter(|program| !program.trim().is_empty());
    if let Some(program) = configured_program {
        match create(OsStr::new(program), &command.args) {
            Ok(process) => return Ok((process, shell_display_name(OsStr::new(program)))),
            Err(error) => {
                tracing::warn!(program, %error, "configured shell failed; trying Windows defaults");
                failures.push(format!("{program}: {error:#}"));
            }
        }
    }

    // Configured arguments must not leak into a different fallback shell.
    // An args-only configuration still applies to the selected default shell.
    let args = if configured_program.is_some() {
        &[][..]
    } else {
        command.args.as_slice()
    };
    let comspec = comspec.filter(|program| !program.to_string_lossy().trim().is_empty());
    let programs = [
        Some(OsString::from("pwsh.exe")),
        Some(OsString::from("powershell.exe")),
        comspec,
        Some(OsString::from("cmd.exe")),
    ];
    for program in programs.into_iter().flatten() {
        match create(&program, args) {
            Ok(process) => return Ok((process, shell_display_name(&program))),
            Err(error) => {
                tracing::warn!(?program, %error, "default shell failed; trying next candidate");
                failures.push(format!("{}: {error:#}", program.to_string_lossy()));
            }
        }
    }
    anyhow::bail!("failed to create shell process: {}", failures.join("; "))
}

fn try_create_shell_process(
    startup_info: &STARTUPINFOEXW,
    command_line: &mut [u16],
    environment_block: &[u16],
    directory: Option<&[u16]>,
) -> windows::core::Result<PROCESS_INFORMATION> {
    let mut process_info = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR::from_raw(command_line.as_mut_ptr())),
            None,
            None,
            false,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT,
            Some(environment_block.as_ptr().cast()),
            directory.map_or(PCWSTR::null(), |path| PCWSTR(path.as_ptr())),
            startup_info as *const STARTUPINFOEXW as *const _,
            &mut process_info,
        )
    }?;
    Ok(process_info)
}
fn terminate_process(process_info: &PROCESS_INFORMATION) {
    unsafe {
        let _ = TerminateProcess(process_info.hProcess, 0xcfffffff);
        let _ = CloseHandle(process_info.hProcess);
        let _ = CloseHandle(process_info.hThread);
    }
}

fn shell_command_line(program: &OsStr, args: &[String]) -> Vec<u16> {
    tracing::info!(?program, ?args, "selected shell command");

    let mut encoded = Vec::new();
    append_windows_argument(&mut encoded, &program.encode_wide().collect::<Vec<_>>());
    for argument in args {
        encoded.push(b' ' as u16);
        append_windows_argument(&mut encoded, &argument.encode_utf16().collect::<Vec<_>>());
    }
    encoded.push(0);
    encoded
}

fn shell_display_name(program: &OsStr) -> String {
    let program = program.to_string_lossy();
    let basename = program
        .rsplit(['\\', '/'])
        .find(|component| !component.is_empty())
        .unwrap_or(&program);
    let suffix_start = basename.len().saturating_sub(4);
    basename
        .get(..suffix_start)
        .filter(|_| {
            basename
                .get(suffix_start..)
                .is_some_and(|suffix| suffix.eq_ignore_ascii_case(".exe"))
        })
        .unwrap_or(basename)
        .to_owned()
}

/// Quotes one argv element according to the Windows CommandLineToArgvW rules.
fn append_windows_argument(output: &mut Vec<u16>, argument: &[u16]) {
    // WSL parses its own command line and treats even quoted options as commands.
    // Leave simple argv elements bare; retain CRT escaping when quoting is needed.
    if !argument.is_empty()
        && !argument
            .iter()
            .any(|&unit| matches!(unit, 9 | 10 | 13 | 32 | 34))
    {
        output.extend_from_slice(argument);
        return;
    }
    output.push(b'"' as u16);
    let mut backslashes = 0;
    for &unit in argument {
        if unit == b'\\' as u16 {
            backslashes += 1;
        } else if unit == b'"' as u16 {
            output.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2 + 1));
            output.push(unit);
            backslashes = 0;
        } else {
            output.extend(std::iter::repeat_n(b'\\' as u16, backslashes));
            output.push(unit);
            backslashes = 0;
        }
    }
    output.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2));
    output.push(b'"' as u16);
}

/// Windows environment-name ordering, including non-ASCII case pairs and
/// inherited hidden drive entries. Compare ordinally, not with a user locale.
fn compare_environment_names(a: &OsStr, b: &OsStr) -> std::cmp::Ordering {
    use windows::Win32::Globalization::{CSTR_EQUAL, CSTR_LESS_THAN, CompareStringOrdinal};
    let a: Vec<_> = a.encode_wide().collect();
    let b: Vec<_> = b.encode_wide().collect();
    match unsafe { CompareStringOrdinal(&a, &b, true) } {
        CSTR_LESS_THAN => std::cmp::Ordering::Less,
        CSTR_EQUAL => std::cmp::Ordering::Equal,
        _ => std::cmp::Ordering::Greater,
    }
}

/// Validate even callers which bypass TOML, before attempting any shell fallback.
fn build_environment_block(
    overrides: &std::collections::BTreeMap<String, String>,
) -> anyhow::Result<Vec<u16>> {
    build_environment_block_from(std::env::vars_os(), overrides)
}

fn build_environment_block_from(
    inherited: impl IntoIterator<Item = (OsString, OsString)>,
    overrides: &std::collections::BTreeMap<String, String>,
) -> anyhow::Result<Vec<u16>> {
    use std::cmp::Ordering::Equal;
    let mut configured: Vec<&OsStr> = Vec::new();
    for (name, value) in overrides {
        ensure!(
            !name.is_empty() && !name.contains(['=', '\0']) && !value.contains('\0'),
            "shell.environment contains an invalid name or NUL value"
        );
        let key = OsStr::new(name);
        ensure!(
            !configured
                .iter()
                .any(|other| compare_environment_names(key, other) == Equal),
            "shell.environment contains conflicting case-insensitive names"
        );
        ensure!(
            compare_environment_names(key, OsStr::new("TERM")) != Equal
                || value == "xterm-256color",
            "shell.environment: TERM must remain xterm-256color"
        );
        configured.push(key);
    }
    let mut entries: Vec<(OsString, OsString)> = Vec::new();
    for (name, value) in inherited
        .into_iter()
        .chain(
            overrides
                .iter()
                .map(|(name, value)| (OsString::from(name), OsString::from(value))),
        )
        .chain(Some((
            OsString::from("TERM"),
            OsString::from("xterm-256color"),
        )))
    {
        // Do not split at '=': Windows also inherits special '=C:' drive entries.
        ensure!(
            !name
                .encode_wide()
                .chain(value.encode_wide())
                .any(|c| c == 0),
            "shell.environment contains an inherited NUL"
        );
        if let Some(entry) = entries
            .iter_mut()
            .find(|(key, _)| compare_environment_names(key, &name) == Equal)
        {
            *entry = (name, value);
        } else {
            entries.push((name, value));
        }
    }
    entries.sort_by(|(a, _), (b, _)| compare_environment_names(a, b));
    let mut block = Vec::new();
    for (name, value) in entries {
        block.extend(name.encode_wide());
        block.push('=' as u16);
        block.extend(value.encode_wide());
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

impl PtyWriter {
    pub(crate) fn write(&mut self, data: &[u8]) -> anyhow::Result<usize> {
        self.input_write.write(data)
    }
}

impl PtyReader {
    pub fn read(&mut self, buffer: &mut [u8]) -> anyhow::Result<usize> {
        ensure!(!buffer.is_empty(), "pty read buffer must be non-empty");

        let mut bytes_read = 0_u32;
        unsafe {
            ReadFile(
                self.output_read.handle(),
                Some(buffer),
                Some(&mut bytes_read as *mut u32),
                None,
            )
        }
        .context("failed to read pty output")?;

        Ok(bytes_read as usize)
    }
}

/// RAII wrapper for Win32 handles returned by pipe and process APIs.
struct OwnedHandle(HANDLE);

// HANDLE values are kernel references, not Rust-owned memory. Moving them to the
// reader thread is safe because ownership is unique and Drop closes each handle once.
unsafe impl Send for OwnedHandle {}

impl OwnedHandle {
    fn new(handle: HANDLE) -> Self {
        Self(handle)
    }

    fn pipe() -> anyhow::Result<(Self, Self)> {
        let mut read = HANDLE::default();
        let mut write = HANDLE::default();
        unsafe { CreatePipe(&mut read, &mut write, None, 0) }?;
        Ok((Self::new(read), Self::new(write)))
    }

    fn handle(&self) -> HANDLE {
        self.0
    }

    /// Writes bytes to the Win32 pipe handle; returns the number written on success.
    pub fn write(&self, data: &[u8]) -> anyhow::Result<usize> {
        let mut written: u32 = 0;
        unsafe { WriteFile(self.0, Some(data), Some(&mut written as *mut u32), None) }?;
        Ok(written as usize)
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let result = CloseHandle(self.0);
                if let Err(err) = result {
                    tracing::error!("CloseHandle failed for handle {:?}: {:?}", self.0, err);
                }
            }
        }
    }
}

/// RAII wrapper for the ConPTY handle, which has a different close API than HANDLE.
struct PseudoConsole {
    handle: HPCON,
    api: &'static ConptyApi,
}

impl PseudoConsole {
    fn create(size: PtySize, input: HANDLE, output: HANDLE) -> anyhow::Result<Self> {
        let coord = COORD {
            X: size.cols,
            Y: size.rows,
        };
        let api = ConptyApi::get()?;
        let mut handle = HPCON::default();
        // SAFETY: The pipe handles and output pointer are valid for this call.
        unsafe { (api.create)(coord, input, output, 0, &mut handle) }
            .ok()
            .context("failed to create bundled pseudo console")?;
        tracing::info!(
            rows = size.rows,
            cols = size.cols,
            "bundled pseudo console ready"
        );

        Ok(Self { handle, api })
    }

    fn handle(&self) -> HPCON {
        self.handle
    }

    fn resize(&mut self, size: PtySize) -> anyhow::Result<()> {
        tracing::trace!(
            rows = size.rows,
            cols = size.cols,
            "resizing pseudo console"
        );
        unsafe {
            (self.api.resize)(
                self.handle,
                COORD {
                    X: size.cols,
                    Y: size.rows,
                },
            )
        }
        .ok()
        .context("failed to resize pseudo console")
    }
}

impl Drop for PseudoConsole {
    fn drop(&mut self) {
        if !self.handle.is_invalid() {
            let (tx, rx) = mpsc::channel();
            let hpcon_val = self.handle.0;
            let api = self.api;

            let _ = std::thread::spawn(move || {
                unsafe {
                    // The library is process-owned and outlives this worker.
                    (api.close)(HPCON(hpcon_val));
                }
                let _ = tx.send(());
            });

            // Synchronously wait for up to 500 milliseconds for ClosePseudoConsole to return.
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(()) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    tracing::warn!(
                        "ClosePseudoConsole TIMEOUT after 500ms (abandoned, OS will reclaim resource on process exit)"
                    );
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    tracing::error!("ClosePseudoConsole channel disconnected unexpectedly");
                }
            }
        }
    }
}

/// Backing storage for the extended startup attribute list passed to CreateProcessW.
struct AttributeList {
    list: LPPROC_THREAD_ATTRIBUTE_LIST,
    _storage: Vec<usize>,
}

impl AttributeList {
    fn with_pseudo_console(pseudo_console: HPCON) -> anyhow::Result<Self> {
        let mut bytes_required = 0_usize;
        let _ = unsafe { InitializeProcThreadAttributeList(None, 1, None, &mut bytes_required) };
        ensure!(
            bytes_required > 0,
            "failed to size process thread attribute list"
        );

        // Windows reports the required byte count; store it in usize words so the
        // pointer is naturally aligned for PROC_THREAD_ATTRIBUTE_LIST.
        let words_required = bytes_required.div_ceil(size_of::<usize>());
        let mut storage = vec![0_usize; words_required];
        let list = LPPROC_THREAD_ATTRIBUTE_LIST(storage.as_mut_ptr().cast());

        unsafe { InitializeProcThreadAttributeList(Some(list), 1, None, &mut bytes_required) }
            .context("failed to initialize process thread attribute list")?;

        let attribute_list = Self {
            list,
            _storage: storage,
        };
        attribute_list.update_pseudo_console(pseudo_console)?;
        Ok(attribute_list)
    }

    fn update_pseudo_console(&self, pseudo_console: HPCON) -> anyhow::Result<()> {
        let value = pseudo_console.0 as usize as *const core::ffi::c_void;
        unsafe {
            UpdateProcThreadAttribute(
                self.list,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                Some(value),
                size_of::<HPCON>(),
                None,
                None,
            )
        }
        .context("failed to update pseudo console process attribute")
    }

    fn as_ptr(&self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.list
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        if !self.list.is_invalid() {
            unsafe {
                DeleteProcThreadAttributeList(self.list);
            }
        }
    }
}

#[cfg(test)]
mod tests;
