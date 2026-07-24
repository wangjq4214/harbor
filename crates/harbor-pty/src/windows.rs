use std::{
    ffi::{OsStr, OsString},
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
            Console::{COORD, CreatePseudoConsole, HPCON, ResizePseudoConsole},
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
                STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject,
            },
        },
    },
    core::{HRESULT, PCWSTR, PWSTR},
};
use anyhow::{Context as _, ensure};

use crate::{PtySize, ReaderShutdown};

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

impl Pty {
    pub fn spawn_shell(size: PtySize) -> anyhow::Result<(Self, PtyReader)> {
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
        let process_info = create_shell_process(&attribute_list)?;
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
        ))
    }

    pub fn resize(&mut self, size: PtySize) -> anyhow::Result<()> {
        ensure!(size.rows > 0 && size.cols > 0, "pty size must be positive");
        tracing::info!(rows = size.rows, cols = size.cols, "resizing windows pty");
        self._pseudo_console.as_mut().unwrap().resize(size)
    }

    /// Writes keyboard input bytes into the ConPTY input pipe.
    pub(crate) fn write(&mut self, data: &[u8]) -> anyhow::Result<usize> {
        self._input_write.as_mut().unwrap().write(data)
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
            Self::shutdown_reader(reader.as_raw_handle());
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
                    Self::shutdown_reader(reader.as_raw_handle());
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

    /// Requests cancellation of the reader's current synchronous I/O operation. ERROR_NOT_FOUND
    /// only means the reader is between reads; the reaper retries until it receives its ack.
    fn shutdown_reader(reader_handle: std::os::windows::io::RawHandle) {
        unsafe {
            if let Err(err) = CancelSynchronousIo(HANDLE(reader_handle as *mut _)) {
                if err.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0) {
                    tracing::debug!("reader had no synchronous I/O to cancel");
                } else {
                    tracing::error!(error = ?err, "CancelSynchronousIo on reader thread failed");
                }
            }
        }
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

fn create_shell_process(attribute_list: &AttributeList) -> anyhow::Result<PROCESS_INFORMATION> {
    let mut startup_info = STARTUPINFOEXW::default();
    startup_info.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup_info.lpAttributeList = attribute_list.as_ptr();

    let mut process_info = PROCESS_INFORMATION::default();
    let mut command_line = shell_command_line();
    let environment_block = build_environment_block();
    tracing::info!("creating shell process");

    unsafe {
        CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR::from_raw(command_line.as_mut_ptr())),
            None,
            None,
            false,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT,
            Some(environment_block.as_ptr().cast()),
            PCWSTR::null(),
            &startup_info as *const STARTUPINFOEXW as *const _,
            &mut process_info,
        )
    }
    .context("failed to create shell process")?;

    Ok(process_info)
}
fn terminate_process(process_info: &PROCESS_INFORMATION) {
    unsafe {
        let _ = TerminateProcess(process_info.hProcess, 0xcfffffff);
        let _ = CloseHandle(process_info.hProcess);
        let _ = CloseHandle(process_info.hThread);
    }
}

fn shell_command_line() -> Vec<u16> {
    let command = std::env::var_os("COMSPEC")
        .unwrap_or_else(|| OsString::from(r"C:\Windows\System32\cmd.exe"));
    tracing::info!(command = ?command, "selected shell command");
    command.encode_wide().chain(std::iter::once(0)).collect()
}

/// Builds a case-insensitively sorted, double-null-terminated UTF-16LE environment block
/// with `TERM=xterm-256color` added/overridden, without modifying the parent process.
fn build_environment_block() -> Vec<u16> {
    // `CreateProcessW` requires entries sorted case-insensitively by key (before `=`).
    // We also need to avoid duplicate `TERM` regardless of casing.
    const TERM_LOWER: [u16; 4] = [0x74, 0x65, 0x72, 0x6d]; // "term" in lowercase ASCII

    fn key_lower(s: &OsStr) -> Vec<u16> {
        s.encode_wide()
            .take_while(|&c| c != 0x3D) // '='
            .map(|c| {
                if (0x41..=0x5A).contains(&c) {
                    c + 0x20
                } else {
                    c
                }
            }) // A-Z → a-z
            .collect()
    }

    let mut entries: Vec<OsString> = std::env::vars_os()
        .filter(|(k, _)| key_lower(k.as_os_str()) != TERM_LOWER)
        .map(|(k, v)| {
            let mut entry = k;
            entry.push("=");
            entry.push(&v);
            entry
        })
        .collect();

    entries.push(OsString::from("TERM=xterm-256color"));

    // Sort case-insensitively by key portion as required by CreateProcessW.
    entries.sort_by_key(|a| key_lower(a));

    let mut block = Vec::new();
    for entry in &entries {
        block.extend(entry.encode_wide());
        block.push(0u16);
    }
    block.push(0u16); // double-null terminator
    block
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

    pub(crate) fn is_shutdown_error(error: &anyhow::Error) -> bool {
        error
            .downcast_ref::<::windows::core::Error>()
            .is_some_and(|error| error.code() == HRESULT::from_win32(ERROR_OPERATION_ABORTED.0))
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
struct PseudoConsole(HPCON);

impl PseudoConsole {
    fn create(size: PtySize, input: HANDLE, output: HANDLE) -> anyhow::Result<Self> {
        let pseudo_console = unsafe {
            CreatePseudoConsole(
                COORD {
                    X: size.cols,
                    Y: size.rows,
                },
                input,
                output,
                0,
            )
        }
        .context("failed to create pseudo console")?;
        tracing::info!(rows = size.rows, cols = size.cols, "pseudo console ready");

        Ok(Self(pseudo_console))
    }

    fn handle(&self) -> HPCON {
        self.0
    }

    fn resize(&mut self, size: PtySize) -> anyhow::Result<()> {
        tracing::trace!(
            rows = size.rows,
            cols = size.cols,
            "resizing pseudo console"
        );
        unsafe {
            ResizePseudoConsole(
                self.0,
                COORD {
                    X: size.cols,
                    Y: size.rows,
                },
            )
        }
        .context("failed to resize pseudo console")
    }
}

impl Drop for PseudoConsole {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            let (tx, rx) = mpsc::channel();
            let hpcon_val = self.0.0;

            let _ = std::thread::spawn(move || {
                unsafe {
                    use ::windows::Win32::System::Console::ClosePseudoConsole;
                    use ::windows::Win32::System::Console::HPCON;
                    ClosePseudoConsole(HPCON(hpcon_val));
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
mod tests {
    use std::{
        thread,
        time::{Duration, Instant},
    };

    use ::windows::Win32::System::Pipes::PeekNamedPipe;

    use super::*;

    #[test]
    fn rejects_empty_size() {
        let error = match Pty::spawn_shell(PtySize { rows: 0, cols: 80 }) {
            Ok(_) => panic!("zero-row pty size unexpectedly spawned a shell"),
            Err(error) => error,
        };
        assert!(
            format!("{error:#}").contains("pty size must be positive"),
            "{error:#}"
        );
    }

    #[test]
    #[ignore = "requires an interactive Windows ConPTY shell"]
    fn shell_prompt_output_is_readable() {
        let (_pty, mut reader) = Pty::spawn_shell(PtySize { rows: 24, cols: 80 }).unwrap();
        let mut buffer = [0_u8; 4096];
        let mut output = Vec::new();

        let deadline = Instant::now() + Duration::from_secs(2);
        while !contains_shell_prompt(&output) && Instant::now() < deadline {
            if output_bytes_available(&reader) == 0 {
                thread::sleep(Duration::from_millis(10));
                continue;
            }

            let bytes = reader.read(&mut buffer).unwrap();
            if bytes == 0 {
                break;
            }
            output.extend_from_slice(&buffer[..bytes]);
        }

        let text = String::from_utf8_lossy(&output);
        assert!(contains_shell_prompt(&output), "{text:?}");
    }

    fn contains_shell_prompt(output: &[u8]) -> bool {
        let text = String::from_utf8_lossy(output);
        text.contains("Microsoft Windows") || text.contains('>')
    }

    fn output_bytes_available(reader: &PtyReader) -> u32 {
        let mut bytes_available = 0_u32;
        unsafe {
            PeekNamedPipe(
                reader.output_read.handle(),
                None,
                0,
                None,
                Some(&mut bytes_available as *mut u32),
                None,
            )
        }
        .unwrap();
        bytes_available
    }
}
