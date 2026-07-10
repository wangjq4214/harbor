use std::{ffi::OsString, mem::size_of, os::windows::ffi::OsStrExt};

use ::windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        Storage::FileSystem::{ReadFile, WriteFile},
        System::{
            Console::{COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON, ResizePseudoConsole},
            Pipes::CreatePipe,
            Threading::{
                CreateProcessW, DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT,
                InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION, STARTF_USESTDHANDLES,
                STARTUPINFOEXW, UpdateProcThreadAttribute, CREATE_UNICODE_ENVIRONMENT,
            },
        },
    },
    core::{PCWSTR, PWSTR},
};
use anyhow::{Context as _, ensure};

use super::{PtySize, ShellExtraEnvs};

/// Windows ConPTY session and the handles that must outlive the shell process.
pub(crate) struct Pty {
    /// Input write end retained so ConPTY keeps stdin open for the child.
    _input_write: OwnedHandle,
    /// ConPTY handle; must outlive the process attached through the attribute list.
    _pseudo_console: PseudoConsole,
    /// Shell process handle retained for lifetime ownership.
    _process: OwnedHandle,
    /// Primary thread handle returned with the process handle.
    _thread: OwnedHandle,
}

/// Read side of the ConPTY output pipe consumed by the background pump.
pub(crate) struct PtyReader {
    /// Output read end consumed by `PtyReader::read`.
    output_read: OwnedHandle,
}

impl Pty {
    pub(crate) fn spawn_shell(
        size: PtySize,
        extra_envs: ShellExtraEnvs,
    ) -> anyhow::Result<(Self, PtyReader)> {
        ensure!(size.rows > 0 && size.cols > 0, "pty size must be positive");
        tracing::info!(rows = size.rows, cols = size.cols, "creating windows pty");

        let (input_read, input_write) =
            OwnedHandle::pipe().context("failed to create pty input pipe")?;
        let (output_read, output_write) =
            OwnedHandle::pipe().context("failed to create pty output pipe")?;
        tracing::info!("created pty pipes");

        let pseudo_console =
            PseudoConsole::create(size, input_read.handle(), output_write.handle())?;
        tracing::info!("created pseudo console");
        let attribute_list = AttributeList::with_pseudo_console(pseudo_console.handle())?;
        let process_info = create_shell_process(&attribute_list, extra_envs)?;
        tracing::info!("created shell process");
        // The pseudo console owns the child-side pipe handles after process creation.
        // Dropping our duplicates makes EOF observable when the shell exits.
        drop(input_read);
        drop(output_write);

        Ok((
            Self {
                _input_write: input_write,
                _pseudo_console: pseudo_console,
                _process: OwnedHandle::new(process_info.hProcess),
                _thread: OwnedHandle::new(process_info.hThread),
            },
            PtyReader { output_read },
        ))
    }

    pub(crate) fn resize(&mut self, size: PtySize) -> anyhow::Result<()> {
        ensure!(size.rows > 0 && size.cols > 0, "pty size must be positive");
        tracing::info!(rows = size.rows, cols = size.cols, "resizing windows pty");
        self._pseudo_console.resize(size)
    }

    /// Writes keyboard input bytes into the ConPTY input pipe.
    pub(crate) fn write(&mut self, data: &[u8]) -> anyhow::Result<usize> {
        self._input_write.write(data)
    }
}

fn create_shell_process(
    attribute_list: &AttributeList,
    extra_envs: ShellExtraEnvs,
) -> anyhow::Result<PROCESS_INFORMATION> {
    let mut startup_info = STARTUPINFOEXW::default();
    startup_info.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup_info.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup_info.lpAttributeList = attribute_list.as_ptr();

    let mut process_info = PROCESS_INFORMATION::default();
    let mut command_line = shell_command_line();
    let env_block = create_environment_block(extra_envs);
    tracing::info!("creating shell process");

    unsafe {
        CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR::from_raw(command_line.as_mut_ptr())),
            None,
            None,
            false,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
            Some(env_block.as_ptr() as *const _),
            PCWSTR::null(),
            &startup_info as *const STARTUPINFOEXW as *const _,
            &mut process_info,
        )
    }
    .context("failed to create shell process")?;

    Ok(process_info)
}

fn shell_command_line() -> Vec<u16> {
    let command = std::env::var_os("COMSPEC")
        .unwrap_or_else(|| OsString::from(r"C:\Windows\System32\cmd.exe"));
    tracing::info!(command = ?command, "selected shell command");
    command.encode_wide().chain(std::iter::once(0)).collect()
}
fn create_environment_block(extra_envs: ShellExtraEnvs) -> Vec<u16> {
    let mut env_vars: std::collections::HashMap<OsString, OsString> = std::env::vars_os().collect();
    for (k, v) in extra_envs.envs {
        env_vars.insert(k, v);
    }

    let mut block = Vec::new();
    for (key, val) in env_vars {
        let mut entry = OsString::new();
        entry.push(&key);
        entry.push("=");
        entry.push(&val);
        block.extend(entry.encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}
impl PtyReader {
    pub(crate) fn read(&mut self, buffer: &mut [u8]) -> anyhow::Result<usize> {
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
    pub(crate) fn write(&self, data: &[u8]) -> anyhow::Result<usize> {
        let mut written: u32 = 0;
        unsafe { WriteFile(self.0, Some(data), Some(&mut written as *mut u32), None) }?;
        Ok(written as usize)
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = CloseHandle(self.0);
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
            unsafe {
                ClosePseudoConsole(self.0);
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
        let error = match Pty::spawn_shell(PtySize { rows: 0, cols: 80 }, ShellExtraEnvs::default()) {
            Ok(_) => panic!("zero-row pty size unexpectedly spawned a shell"),
            Err(error) => error,
        };
        assert!(
            format!("{error:#}").contains("pty size must be positive"),
            "{error:#}"
        );
    }

    #[test]
    fn shell_prompt_output_is_readable() {
        let (_pty, mut reader) = Pty::spawn_shell(PtySize { rows: 24, cols: 80 }, ShellExtraEnvs::default()).unwrap();
        let mut buffer = [0_u8; 4096];
        let mut output = Vec::new();

        let bytes = reader.read(&mut buffer).unwrap();
        assert!(bytes > 0);
        output.extend_from_slice(&buffer[..bytes]);

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
