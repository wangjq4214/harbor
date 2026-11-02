//! Explicit Host policy. Terminal OSC parsing never inspects the filesystem.
use std::{collections::BTreeMap, path::PathBuf};

use harbor_app::profiles::ProfileLaunchIntent;
use harbor_config::{ProfileId, SessionProfile};
use harbor_pty::{ExecutablePolicy, ShellCommand};
use harbor_terminal::WorkingDirectoryMetadata;

/// Shared Host transaction: validate source/current identity before reserving a tab,
/// then let TabManager publish only successfully created resources.
pub(crate) fn create_tab(
    tabs: &mut harbor_app::tab_manager::TabManager,
    profiles: &BTreeMap<ProfileId, SessionProfile>,
    intent: &ProfileLaunchIntent,
    factory: impl FnOnce(
        harbor_app::tab_manager::TabId,
        harbor_widget::scene::primitive::ExternalDrawId,
        &ShellCommand,
    ) -> anyhow::Result<harbor_app::tab_manager::TerminalTabResources>,
) -> anyhow::Result<harbor_app::tab_manager::TabActionOutcome> {
    let metadata = if let Some(source) = intent.source {
        anyhow::ensure!(
            tabs.active_live(source),
            "profile source is no longer active and live"
        );
        tabs.active_working_directory()
    } else {
        None
    };
    let command = resolve(profiles, intent, metadata)?;
    tabs.create_tab(|id, draw| factory(id, draw, &command))
}
pub(crate) struct ResolvedLaunch {
    command: ShellCommand,
    // Keep local directory identities pinned until process creation completes.
    _local_directory: Option<LocalDirectory>,
}

impl std::ops::Deref for ResolvedLaunch {
    type Target = ShellCommand;
    fn deref(&self) -> &Self::Target {
        &self.command
    }
}

pub(crate) struct LocalDirectory {
    path: PathBuf,
    #[cfg(windows)]
    _handles: Vec<std::os::windows::io::OwnedHandle>,
}

pub(crate) fn resolve(
    profiles: &BTreeMap<ProfileId, SessionProfile>,
    intent: &ProfileLaunchIntent,
    metadata: Option<&WorkingDirectoryMetadata>,
) -> anyhow::Result<ResolvedLaunch> {
    let profile = profiles
        .get(&intent.profile)
        .ok_or_else(|| anyhow::anyhow!("selected profile is no longer available"))?;
    let mut command = crate::tab_coordinator::configured_shell(&profile.launch);
    command.executable_policy = ExecutablePolicy::Strict;
    let mut local = None;
    command.directory = Some(
        if let Some(directory) = intent.directory.as_ref().or(command.directory.as_ref()) {
            // Explicit/configured failures are not bypassed by metadata or process cwd.
            anyhow::ensure!(directory.is_absolute(), "launch directory must be absolute");
            anyhow::ensure!(
                !directory.as_os_str().to_string_lossy().contains('\0'),
                "launch directory contains NUL"
            );
            anyhow::ensure!(
                directory.is_dir(),
                "selected launch directory is unavailable or not a directory"
            );
            directory.clone()
        } else if let Some(directory) = metadata.and_then(local_directory) {
            let path = directory.path.clone();
            local = Some(directory);
            path
        } else {
            std::env::current_dir()?
        },
    );
    Ok(ResolvedLaunch {
        command,
        _local_directory: local,
    })
}

fn is_local_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    #[cfg(windows)]
    {
        use windows::{
            Win32::System::SystemInformation::{
                ComputerNameDnsFullyQualified, ComputerNameDnsHostname, ComputerNameNetBIOS,
                GetComputerNameExW,
            },
            core::PWSTR,
        };
        // OS identity only: no DNS resolution or remote authority lookup.
        for kind in [
            ComputerNameNetBIOS,
            ComputerNameDnsHostname,
            ComputerNameDnsFullyQualified,
        ] {
            let mut buffer = [0u16; 256];
            let mut size = buffer.len() as u32;
            if unsafe { GetComputerNameExW(kind, Some(PWSTR(buffer.as_mut_ptr())), &mut size) }
                .is_ok()
                && String::from_utf16(&buffer[..size as usize])
                    .is_ok_and(|name| name.eq_ignore_ascii_case(host))
            {
                return true;
            }
        }
    }
    false
}

/// Metadata paths have already been percent-decoded by Terminal, exactly once.
/// Reject authorities and native remote/device forms before any filesystem probe.
fn native_local_path(metadata: &WorkingDirectoryMetadata) -> Option<PathBuf> {
    if metadata.host().is_some_and(|host| !is_local_host(host)) {
        return None;
    }
    let path = metadata.path();
    if path.contains('\\') || path.chars().any(char::is_control) {
        return None;
    }
    #[cfg(windows)]
    {
        let bytes = path.as_bytes();
        if bytes.len() < 4
            || bytes[0] != b'/'
            || !bytes[1].is_ascii_alphabetic()
            || bytes[2] != b':'
            || bytes[3] != b'/'
        {
            return None;
        }
        let suffix = &path[4..];
        if suffix.contains([':', '?', '*', '<', '>', '|', '"'])
            || suffix
                .split('/')
                .any(|part| part == "." || part == ".." || part.ends_with(['.', ' ']))
        {
            return None;
        }
        Some(PathBuf::from(&path[1..]))
    }
    #[cfg(not(windows))]
    {
        (path.starts_with('/') && !path.starts_with("//")).then(|| PathBuf::from(path))
    }
}

pub(crate) fn local_directory(metadata: &WorkingDirectoryMetadata) -> Option<LocalDirectory> {
    let path = native_local_path(metadata)?;
    inspect_local_directory(path)
}

#[cfg(windows)]
fn inspect_local_directory(path: PathBuf) -> Option<LocalDirectory> {
    use std::os::windows::{
        ffi::OsStrExt,
        io::{FromRawHandle, OwnedHandle},
    };
    use windows::{
        Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_ATTRIBUTE_DIRECTORY,
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
            FILE_READ_ATTRIBUTES, FILE_SHARE_READ, GetDriveTypeW, GetFileInformationByHandle,
            OPEN_EXISTING,
        },
        core::PCWSTR,
    };
    let root: Vec<_> = path
        .components()
        .take(2)
        .collect::<PathBuf>()
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // Mapped drives are not local merely because they have a drive letter.
    if !matches!(
        unsafe { GetDriveTypeW(PCWSTR(root.as_ptr())) },
        2 | 3 | 5 | 6
    ) {
        return None;
    }
    let mut handles = Vec::new();
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        if !current.is_absolute() {
            continue;
        }
        let name: Vec<_> = current.as_os_str().encode_wide().chain(Some(0)).collect();
        // Do not traverse the final reparse point. Retain each ancestor without
        // write/delete sharing so it cannot be replaced while descending/spawning.
        let handle = unsafe {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                FILE_READ_ATTRIBUTES.0,
                FILE_SHARE_READ,
                None,
                OPEN_EXISTING,
                FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
                None,
            )
        }
        .ok()?;
        let owned = unsafe { OwnedHandle::from_raw_handle(handle.0) };
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        unsafe { GetFileInformationByHandle(handle, &mut info) }.ok()?;
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0
            || info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 == 0
        {
            return None;
        }
        handles.push(owned);
    }
    Some(LocalDirectory {
        path,
        _handles: handles,
    })
}

#[cfg(not(windows))]
fn inspect_local_directory(path: PathBuf) -> Option<LocalDirectory> {
    // Unix PTY creation remains unimplemented; do not follow symlink metadata.
    path.ancestors()
        .all(|p| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir() && !m.is_symlink()))
        .then_some(LocalDirectory { path })
}

#[cfg(test)]
mod tests;
