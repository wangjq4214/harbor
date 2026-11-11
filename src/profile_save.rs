//! Host capture and selected-file persistence. Never serializes accepted stale source text.
use harbor_app::{QuickSaveDraft, tab_manager::TabManager};
use harbor_config::{SessionProfile, SettingsLoad, ShellSettings};
#[cfg(windows)]
use std::io::Read;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub(crate) fn capture(tabs: &TabManager) -> Result<QuickSaveDraft, &'static str> {
    let source = tabs.active_id().ok_or("No current session")?;
    if !tabs.active_live(source) {
        return Err("The current session is not live");
    }
    let terminal = tabs.active_terminal().ok_or("No current session")?;
    let terminal = terminal.lock().map_err(|_| "Source launch unavailable")?;
    let provenance = terminal
        .launch_provenance()
        .ok_or("Source launch unavailable")?;
    capture_recipe(source, provenance, terminal.project_snapshot())
}

fn capture_recipe(
    source: harbor_app::tab_manager::TabId,
    provenance: &harbor_pty::LaunchProvenance,
    project: Option<harbor_config::ProjectRecipe>,
) -> Result<QuickSaveDraft, &'static str> {
    let project = project.filter(|recipe| {
        !matches!(
            recipe.environment,
            harbor_config::ProjectEnvironment::LocalPwsh
        ) || crate::profile_launch::inspect_local_directory(PathBuf::from(&recipe.directory))
            .is_some()
    });
    let program = provenance
        .program
        .to_str()
        .ok_or("Source program is not Unicode")?
        .to_owned();
    Ok(QuickSaveDraft {
        source,
        project,
        launch: ShellSettings {
            program: Some(program),
            args: provenance.args.clone(),
            directory: None, // Initial cwd and OSC 7 are not save-time evidence.
            environment: provenance.configured_environment.clone(),
        },
    })
}

/// Contains confidential file contents: intentionally no Debug implementation.
pub(crate) struct SaveTransaction {
    path: PathBuf,
    original: Option<Vec<u8>>,
    committed: Option<(SessionProfile, Vec<u8>)>,
}
fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, String> {
    reject_link(path)?;
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("Could not read selected configuration".into()),
    }
}
impl SaveTransaction {
    pub(crate) fn begin(path: PathBuf) -> Result<Self, String> {
        if !path.is_absolute() {
            return Err("Selected configuration path must be absolute".into());
        }
        let original = read_optional(&path)?;
        Ok(Self {
            path,
            original,
            committed: None,
        })
    }
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
    pub(crate) fn refresh(&mut self) -> Result<(), String> {
        if self.committed.is_some() {
            return Ok(());
        }
        self.original = read_optional(&self.path)?;
        Ok(())
    }
    pub(crate) fn committed_bytes(&self) -> Option<Vec<u8>> {
        self.committed.as_ref().map(|(_, bytes)| bytes.clone())
    }
    pub(crate) fn commit(&mut self, profile: &SessionProfile) -> Result<SettingsLoad, String> {
        self.commit_with(profile, |_| Ok(()))
    }
    fn commit_with(
        &mut self,
        profile: &SessionProfile,
        before_replace: impl FnOnce(&Path) -> Result<(), String>,
    ) -> Result<SettingsLoad, String> {
        if let Some((saved, bytes)) = &self.committed {
            if saved != profile {
                return Err("Preset is already on disk; retry its publication or cancel before saving another name".into());
            }
            if read_optional(&self.path)?.as_ref() != Some(bytes) {
                return Err("Preset was saved, but configuration changed before publication; reload and inspect the file".into());
            }
            let source =
                std::str::from_utf8(bytes).map_err(|_| "Invalid configuration encoding")?;
            return harbor_config::candidate_from_source(&self.path, source)
                .map_err(|_| "Saved configuration is no longer valid".into());
        }
        if read_optional(&self.path)? != self.original {
            return Err("Configuration changed externally. Return to edit, then review and retry; no file was overwritten".into());
        }
        let source = std::str::from_utf8(self.original.as_deref().unwrap_or_default())
            .map_err(|_| "Configuration is not UTF-8")?;
        let (text, loaded) =
            harbor_config::append_profile(&self.path, source, profile).map_err(str::to_owned)?;
        harbor_app::command::validate_keybindings(&loaded.settings.keybindings)
            .map_err(|_| "Configuration keybindings are invalid")?;
        if fs::metadata(&self.path).is_ok_and(|metadata| metadata.permissions().readonly()) {
            return Err("Selected configuration is read-only; no file was changed".into());
        }
        let parent = self
            .path
            .parent()
            .ok_or("Selected configuration has no parent directory")?;
        // No fallback directory and no automatic parent creation.
        let mut staged = tempfile::Builder::new()
            .prefix(".harbor-save-")
            .tempfile_in(parent)
            .map_err(|_| "Could not stage selected configuration")?;
        preserve_access(&self.path, staged.path())?;
        staged
            .write_all(text.as_bytes())
            .and_then(|_| staged.as_file().sync_all())
            .map_err(|_| "Could not write/sync staged configuration; original is unchanged")?;
        before_replace(&self.path)?;
        replace_selected(&self.path, self.original.as_deref(), staged)?;
        self.committed = Some((profile.clone(), text.into_bytes()));
        Ok(loaded)
    }
}

/// Pin the just-saved generation through complete in-memory acceptance.
#[cfg(windows)]
pub(crate) fn pin_generation(path: &Path, expected: &[u8]) -> Result<fs::File, String> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    let mut file = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x0020_0000)
        .open(path)
        .map_err(|_| "Saved configuration is busy or unavailable")?;
    if file
        .metadata()
        .map_err(|_| "Could not inspect saved configuration")?
        .file_attributes()
        & 0x400
        != 0
    {
        return Err("Saved configuration was replaced with a link".into());
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| "Could not verify saved configuration")?;
    if bytes != expected {
        return Err("Saved configuration changed before publication".into());
    }
    Ok(file)
}
#[cfg(not(windows))]
pub(crate) fn pin_generation(_: &Path, _: &[u8]) -> Result<fs::File, String> {
    Err("Conflict-safe quick save is currently supported only on Windows".into())
}
fn reject_link(path: &Path) -> Result<(), String> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        #[cfg(windows)]
        let linked = {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes() & 0x400 != 0
        };
        #[cfg(not(windows))]
        let linked = metadata.file_type().is_symlink();
        if linked {
            return Err("Linked configuration files cannot be replaced by quick save".into());
        }
    }
    Ok(())
}

#[cfg(windows)]
fn preserve_access(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::Security::{
            DACL_SECURITY_INFORMATION, GetFileSecurityW, PROTECTED_DACL_SECURITY_INFORMATION,
            PSECURITY_DESCRIPTOR, SetFileSecurityW,
        },
        core::PCWSTR,
    };
    if !source.exists() {
        return Ok(());
    }
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut length = 0;
    unsafe {
        GetFileSecurityW(
            PCWSTR(source.as_ptr()),
            DACL_SECURITY_INFORMATION.0,
            None,
            0,
            &mut length,
        )
        .ok()
        .ok();
    }
    if length == 0 {
        return Err("Could not inspect configuration access permissions".into());
    }
    let mut storage = vec![0_u64; (length as usize).div_ceil(8)];
    let descriptor = PSECURITY_DESCRIPTOR(storage.as_mut_ptr().cast());
    unsafe {
        GetFileSecurityW(
            PCWSTR(source.as_ptr()),
            DACL_SECURITY_INFORMATION.0,
            Some(descriptor),
            length,
            &mut length,
        )
        .ok()
        .map_err(|_| "Could not read configuration access permissions")?;
        SetFileSecurityW(
            PCWSTR(destination.as_ptr()),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        )
        .ok()
        .map_err(|_| "Could not preserve configuration access permissions")?;
    }
    Ok(())
}
#[cfg(not(windows))]
fn preserve_access(source: &Path, destination: &Path) -> Result<(), String> {
    if let Ok(metadata) = fs::metadata(source) {
        fs::set_permissions(destination, metadata.permissions())
            .map_err(|_| "Could not preserve configuration access permissions")?;
    }
    Ok(())
}

/// Keep the old file immutable from the final comparison until it has been moved
/// by its own handle. Installation is no-clobber: a late path creator wins, never
/// gets overwritten. A crash between the two renames leaves the intact original
/// at the reported recovery name, not a truncated document.
#[cfg(windows)]
fn replace_selected(
    path: &Path,
    original: Option<&[u8]>,
    staged: tempfile::NamedTempFile,
) -> Result<(), String> {
    replace_selected_with(path, original, staged, |staged, path| {
        staged.persist_noclobber(path).map_err(|_| ())
    })
}

#[cfg(windows)]
fn replace_selected_with(
    path: &Path,
    original: Option<&[u8]>,
    staged: tempfile::NamedTempFile,
    install: impl FnOnce(tempfile::NamedTempFile, &Path) -> Result<fs::File, ()>,
) -> Result<(), String> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::{FILE_GENERIC_READ, FILE_SHARE_READ};
    let Some(original) = original else {
        staged.persist_noclobber(path).map_err(
            |_| "Configuration appeared or installation failed; existing file was not overwritten",
        )?;
        return Ok(());
    };
    reject_link(path)?;
    let mut guard = fs::OpenOptions::new()
        .access_mode(FILE_GENERIC_READ.0 | 0x0001_0000 /* DELETE */)
        .custom_flags(0x0020_0000 /* FILE_FLAG_OPEN_REPARSE_POINT */)
        .share_mode(FILE_SHARE_READ.0)
        .open(path)
        .map_err(|_| "Configuration is busy or unavailable; nothing saved")?;
    use std::os::windows::fs::MetadataExt;
    if guard
        .metadata()
        .map_err(|_| "Could not inspect configuration")?
        .file_attributes()
        & 0x400
        != 0
    {
        return Err("Linked configuration files cannot be replaced by quick save".into());
    }
    let mut bytes = Vec::new();
    guard
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not recheck selected configuration")?;
    if bytes != original {
        return Err(
            "Configuration changed at commit; nothing saved. Return to edit and retry".into(),
        );
    }
    let backup = tempfile::Builder::new()
        .prefix(".harbor-save-recovery-")
        .tempfile_in(path.parent().unwrap())
        .map_err(|_| "Could not reserve recovery file")?;
    let backup_path = backup.path().to_owned();
    backup
        .close()
        .map_err(|_| "Could not prepare recovery path")?;
    rename_guarded(&guard, &backup_path)
        .map_err(|_| "Could not replace selected configuration; original is unchanged")?;
    match install(staged, path) {
        Ok(file) => {
            if file.sync_all().is_err() {
                // Preserve both complete versions and report the incomplete durability outcome.
                return Err(format!(
                    "New configuration installed but sync failed; original retained at {}. Inspect/reload before retrying",
                    backup_path.display()
                ));
            }
            drop(guard);
            let _ = fs::remove_file(&backup_path);
            Ok(())
        }
        Err(_) => {
            if rename_guarded(&guard, path).is_ok() {
                Err("Could not install configuration; original restored. Retry after correcting file access".into())
            } else {
                Err(format!(
                    "Configuration path changed during installation; it was not overwritten. Original retained at {}",
                    backup_path.display()
                ))
            }
        }
    }
}
#[cfg(windows)]
fn rename_guarded(file: &fs::File, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::{ffi::OsStrExt, io::AsRawHandle};
    use windows::{
        Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{FILE_RENAME_INFO, FileRenameInfo, SetFileInformationByHandle},
        },
        core::PCWSTR,
    };
    let destination = destination.as_os_str().encode_wide().collect::<Vec<_>>();
    let bytes = std::mem::offset_of!(FILE_RENAME_INFO, FileName) + destination.len() * 2;
    // Aligned, owned flexible-array storage; no replacement flag is set.
    let mut storage = vec![0_u64; bytes.div_ceil(8)];
    let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    unsafe {
        (*info).FileNameLength = (destination.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(
            PCWSTR(destination.as_ptr()).0,
            (*info).FileName.as_mut_ptr(),
            destination.len(),
        );
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileRenameInfo,
            info.cast(),
            bytes as u32,
        )
        .map_err(|_| std::io::Error::last_os_error())
    }
}
#[cfg(not(windows))]
fn replace_selected(_: &Path, _: Option<&[u8]>, _: tempfile::NamedTempFile) -> Result<(), String> {
    Err("Conflict-safe quick save is currently supported only on Windows".into())
}

#[cfg(all(test, windows))]
#[path = "profile_save/tests.rs"]
mod tests;
