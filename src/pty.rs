#![allow(unsafe_code)]

use anyhow::Result;
use std::process::Child;

#[cfg(unix)]
use std::os::unix::io::RawFd;



#[cfg(unix)]
pub struct Pty {
    master_fd: RawFd,
    writer: std::fs::File,
}

#[cfg(windows)]
pub struct Pty {
    hpcon: win_util::HANDLE,
    writer: std::fs::File,
    reader: std::fs::File,
    resize_fn: unsafe extern "system" fn(win_util::HANDLE, win_util::COORD) -> win_util::HRESULT,
    close_fn: unsafe extern "system" fn(win_util::HANDLE),
}

#[cfg(unix)]
impl Pty {
    /// Spawns a shell process connected to a native Unix PTY.
    pub fn spawn(shell: &str, rows: u16, cols: u16) -> Result<(Self, Child)> {
        use nix::pty::{openpty, Winsize};
        use anyhow::Context as _;
        use std::os::unix::io::FromRawFd as _;
        use std::process::Command;
        use std::fs::File;

        let winsize = Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        let pty_res = openpty(Some(&winsize), None).context("openpty failed")?;
        let master_fd = pty_res.master;
        let slave_fd = pty_res.slave;

        // Helper function to spawn the shell in a child process
        fn spawn_process(shell: &str, slave_fd: RawFd) -> std::io::Result<Child> {
            use std::os::unix::process::CommandExt;
            let dup_slave = unsafe { libc::dup(slave_fd) };
            if dup_slave < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let mut cmd = Command::new(shell);
            unsafe {
                cmd.pre_exec(move || {
                    if libc::login_tty(dup_slave) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            let child = cmd.spawn();
            let _ = unsafe { libc::close(dup_slave) };
            child
        }

        let child = match spawn_process(shell, slave_fd) {
            Ok(c) => c,
            Err(_) => match spawn_process("/bin/sh", slave_fd) {
                Ok(c) => c,
                Err(err) => {
                    let _ = unsafe { libc::close(master_fd) };
                    let _ = unsafe { libc::close(slave_fd) };
                    return Err(err).context("spawn fallback shell failed");
                }
            }
        };

        // Close slave_fd in the parent process, so that the parent does not hold an open descriptor.
        // This ensures the master side receives EOF when the child process exits.
        let _ = unsafe { libc::close(slave_fd) };

        let master_file = unsafe { File::from_raw_fd(master_fd) };
        let pty_writer = master_file;

        Ok((
            Self {
                master_fd,
                writer: pty_writer,
            },
            child,
        ))
    }

    /// Clones the reader file handle for the background reading thread.
    pub fn try_clone_reader(&self) -> Result<std::fs::File> {
        use anyhow::Context as _;
        self.writer.try_clone().context("clone master file failed")
    }

    /// Writes data into the PTY input stream.
    pub fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        use std::io::Write as _;
        use anyhow::Context as _;
        self.writer.write_all(buf).context("write to pty failed")
    }

    /// Flushes any buffered bytes.
    pub fn flush(&mut self) -> Result<()> {
        use std::io::Write as _;
        use anyhow::Context as _;
        self.writer.flush().context("flush pty failed")
    }

    /// Dynamically resizes the PTY.
    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        let ws = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let res = unsafe { libc::ioctl(self.master_fd, libc::TIOCSWINSZ, &ws) };
        if res < 0 {
            return Err(anyhow::anyhow!(
                "ioctl TIOCSWINSZ failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }
}

#[cfg(windows)]
mod win_util {
    use std::os::windows::io::RawHandle;

    pub type HANDLE = RawHandle;
    pub type HRESULT = i32;
    pub type BOOL = i32;
    pub type DWORD = u32;

    pub const HANDLE_FLAG_INHERIT: DWORD = 1;
    pub const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x00020016;

    #[repr(C)]
    pub struct COORD {
        pub X: i16,
        pub Y: i16,
    }

    #[repr(C)]
    pub struct SECURITY_ATTRIBUTES {
        pub nLength: u32,
        pub lpSecurityDescriptor: *mut std::ffi::c_void,
        pub bInheritHandle: BOOL,
    }

    extern "system" {
        pub fn CreatePipe(
            hReadPipe: *mut HANDLE,
            hWritePipe: *mut HANDLE,
            lpPipeAttributes: *const SECURITY_ATTRIBUTES,
            nSize: DWORD,
        ) -> BOOL;

        pub fn SetHandleInformation(
            hObject: HANDLE,
            dwMask: DWORD,
            dwFlags: DWORD,
        ) -> BOOL;

        pub fn CloseHandle(hObject: HANDLE) -> BOOL;

        pub fn LoadLibraryW(lpLibFileName: *const u16) -> *mut std::ffi::c_void;
        pub fn GetProcAddress(
            hModule: *mut std::ffi::c_void,
            lpProcName: *const u8,
        ) -> *mut std::ffi::c_void;
    }
}

#[cfg(windows)]
struct ConPtyApi {
    create_fn: unsafe extern "system" fn(
        win_util::COORD,
        win_util::HANDLE,
        win_util::HANDLE,
        u32,
        *mut win_util::HANDLE,
    ) -> win_util::HRESULT,
    resize_fn: unsafe extern "system" fn(win_util::HANDLE, win_util::COORD) -> win_util::HRESULT,
    close_fn: unsafe extern "system" fn(win_util::HANDLE),
}

#[cfg(windows)]
impl ConPtyApi {
    fn load() -> Result<Self> {
        let lib_name: Vec<u16> = "kernel32.dll\0".encode_utf16().collect();
        let h_module = unsafe { win_util::LoadLibraryW(lib_name.as_ptr()) };
        if h_module.is_null() {
            return Err(anyhow::anyhow!("Failed to load kernel32.dll"));
        }

        unsafe {
            let create_addr = win_util::GetProcAddress(h_module, b"CreatePseudoConsole\0".as_ptr());
            let resize_addr = win_util::GetProcAddress(h_module, b"ResizePseudoConsole\0".as_ptr());
            let close_addr = win_util::GetProcAddress(h_module, b"ClosePseudoConsole\0".as_ptr());

            if create_addr.is_null() || resize_addr.is_null() || close_addr.is_null() {
                return Err(anyhow::anyhow!(
                    "ConPTY API is not supported on this Windows version"
                ));
            }

            Ok(Self {
                create_fn: std::mem::transmute(create_addr),
                resize_fn: std::mem::transmute(resize_addr),
                close_fn: std::mem::transmute(close_addr),
            })
        }
    }
}

#[cfg(windows)]
impl Pty {
    /// Spawns a shell process connected to Windows ConPTY.
    pub fn spawn(shell: &str, rows: u16, cols: u16) -> Result<(Self, Child)> {
        use std::os::windows::io::FromRawHandle as _;
        use std::os::windows::process::CommandExt as _;
        use std::process::Command;

        let api = ConPtyApi::load()?;

        // 1. Create Pipes for input and output
        let mut input_read: win_util::HANDLE = std::ptr::null_mut();
        let mut input_write: win_util::HANDLE = std::ptr::null_mut();
        let mut output_read: win_util::HANDLE = std::ptr::null_mut();
        let mut output_write: win_util::HANDLE = std::ptr::null_mut();

        let sa = win_util::SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<win_util::SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: 1,
        };

        unsafe {
            if win_util::CreatePipe(&mut input_read, &mut input_write, &sa, 0) == 0 {
                return Err(anyhow::anyhow!(
                    "CreatePipe failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
            if win_util::CreatePipe(&mut output_read, &mut output_write, &sa, 0) == 0 {
                win_util::CloseHandle(input_read);
                win_util::CloseHandle(input_write);
                return Err(anyhow::anyhow!(
                    "CreatePipe failed: {}",
                    std::io::Error::last_os_error()
                ));
            }

            // Disable inheritance of parent sides to prevent blocking read EOFs
            win_util::SetHandleInformation(input_write, win_util::HANDLE_FLAG_INHERIT, 0);
            win_util::SetHandleInformation(output_read, win_util::HANDLE_FLAG_INHERIT, 0);
        }

        // 2. Create PseudoConsole
        let mut hpcon: win_util::HANDLE = std::ptr::null_mut();
        let size = win_util::COORD {
            X: cols as i16,
            Y: rows as i16,
        };

        let hr = unsafe { (api.create_fn)(size, input_read, output_write, 0, &mut hpcon) };
        if hr < 0 {
            unsafe {
                win_util::CloseHandle(input_read);
                win_util::CloseHandle(input_write);
                win_util::CloseHandle(output_read);
                win_util::CloseHandle(output_write);
            }
            return Err(anyhow::anyhow!(
                "CreatePseudoConsole failed with HRESULT: {:#X}",
                hr
            ));
        }

        // 3. Spawn child shell process
        let mut cmd = Command::new(shell);
        cmd.raw_attribute(
            win_util::PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
            hpcon as usize,
        );

        let child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                // Try fallback to cmd.exe if the preferred shell fails
                let mut fallback_cmd = Command::new("cmd.exe");
                fallback_cmd.raw_attribute(
                    win_util::PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
                    hpcon as usize,
                );
                fallback_cmd.spawn().map_err(|err| {
                    unsafe {
                        (api.close_fn)(hpcon);
                        win_util::CloseHandle(input_read);
                        win_util::CloseHandle(input_write);
                        win_util::CloseHandle(output_read);
                        win_util::CloseHandle(output_write);
                    }
                    anyhow::anyhow!(
                        "Spawn fallback shell failed: {}, original error: {}",
                        err,
                        e
                    )
                })?
            }
        };

        // 4. Close the handles passed to ConPTY inside the parent process
        unsafe {
            win_util::CloseHandle(input_read);
            win_util::CloseHandle(output_write);
        }

        let pty_writer = unsafe { std::fs::File::from_raw_handle(input_write) };
        let pty_reader = unsafe { std::fs::File::from_raw_handle(output_read) };

        Ok((
            Self {
                hpcon,
                writer: pty_writer,
                reader: pty_reader,
                resize_fn: api.resize_fn,
                close_fn: api.close_fn,
            },
            child,
        ))
    }

    /// Clones the read-pipe file handle for the reading thread.
    pub fn try_clone_reader(&self) -> Result<std::fs::File> {
        use anyhow::Context as _;
        self.reader.try_clone().context("clone pipe failed")
    }

    /// Writes data into the Windows ConPTY input stream.
    pub fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        use std::io::Write as _;
        use anyhow::Context as _;
        self.writer.write_all(buf).context("write to pipe failed")
    }

    /// Flushes any buffered bytes.
    pub fn flush(&mut self) -> Result<()> {
        use std::io::Write as _;
        use anyhow::Context as _;
        self.writer.flush().context("flush pipe failed")
    }

    /// Resizes the Windows ConPTY.
    pub fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        let size = win_util::COORD {
            X: cols as i16,
            Y: rows as i16,
        };
        let hr = unsafe { (self.resize_fn)(self.hpcon, size) };
        if hr < 0 {
            return Err(anyhow::anyhow!(
                "ResizePseudoConsole failed with HRESULT: {:#X}",
                hr
            ));
        }
        Ok(())
    }
}

#[cfg(windows)]
impl Drop for Pty {
    fn drop(&mut self) {
        unsafe {
            (self.close_fn)(self.hpcon);
        }
    }
}
