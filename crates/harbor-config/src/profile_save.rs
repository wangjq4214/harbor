//! Add one independent recipe with lossless TOML editing; preserve comments and unrelated settings.
use crate::{ProfileId, SessionProfile, SettingsLoad, candidate_from_source};
use std::path::Path;
use toml::{Table, Value};

pub fn append_profile(
    path: &Path,
    source: &str,
    profile: &SessionProfile,
) -> Result<(String, SettingsLoad), &'static str> {
    let current = candidate_from_source(path, source)
        .map_err(|_| "Configuration is invalid; correct it before saving")?;
    let name = profile.name.trim();
    if name.is_empty() || name.chars().any(char::is_control) {
        return Err("Enter a non-empty name without control characters");
    }
    if current.settings.profiles.values().any(|p| p.name == name) {
        return Err("A preset with this name already exists");
    }
    let id = (1_u64..)
        .map(|n| format!("saved-{n}"))
        .find(|id| !current.settings.profiles.contains_key(id.as_str()))
        .unwrap();
    let mut recipe = Table::new();
    recipe.insert("name".into(), Value::String(name.into()));
    recipe.insert(
        "program".into(),
        Value::String(
            profile
                .launch
                .program
                .clone()
                .ok_or("Source launch program unavailable")?,
        ),
    );
    recipe.insert(
        "args".into(),
        Value::Array(
            profile
                .launch
                .args
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
    );
    if let Some(path) = &profile.launch.directory {
        recipe.insert(
            "directory".into(),
            Value::String(
                path.to_str()
                    .ok_or("Source directory is not Unicode")?
                    .into(),
            ),
        );
    }
    if !profile.launch.environment.is_empty() {
        recipe.insert(
            "environment".into(),
            Value::Table(
                profile
                    .launch
                    .environment
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                    .collect(),
            ),
        );
    }
    if let Some(project) = &profile.project {
        project.validate()?;
        recipe.insert("project".into(), project.value());
    }
    let mut profiles = Table::new();
    profiles.insert(id.clone(), Value::Table(recipe));
    let mut root = Table::new();
    root.insert("profiles".into(), Value::Table(profiles));
    let addition = toml::to_string(&root).map_err(|_| "Could not serialize preset")?;
    let mut document = source
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| "Could not preserve configuration document")?;
    let addition = addition
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| "Could not serialize preset")?;
    if !document.contains_key("profiles") {
        document.insert("profiles", toml_edit::Item::Table(toml_edit::Table::new()));
    }
    document["profiles"]
        .as_table_like_mut()
        .ok_or("Profiles must be a table")?
        .insert(&id, addition["profiles"][&id].clone());
    let result = document.to_string();
    let loaded = candidate_from_source(path, &result)
        .map_err(|_| "Could not add preset to this document")?;
    let saved = &loaded.settings.profiles[&ProfileId::new(id).unwrap()];
    if saved.name != name || saved.launch != profile.launch || saved.project != profile.project {
        return Err("Saved recipe did not validate unchanged");
    }
    Ok((result, loaded))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ShellSettings;
    #[test]
    fn append_retains_comments_settings_and_literal_recipe_and_rejects_duplicates() {
        let path = std::env::current_dir().unwrap().join("config.toml");
        let source = "# do not erase\n[shell]\nprogram = 'cmd.exe' # ordinary\n[profiles.old]\nname='Old'\nprogram='cmd.exe'\n";
        let profile = SessionProfile {
            project: None,
            name: "New\" 名".into(),
            launch: ShellSettings {
                program: Some("cmd.exe".into()),
                args: vec!["".into(), "a b".into(), "quote\"\\".into()],
                directory: Some(std::env::current_dir().unwrap()),
                environment: [("SYNTHETIC".into(), "literal $HOME".into())].into(),
            },
        };
        let (text, loaded) = append_profile(&path, source, &profile).unwrap();
        assert!(text.contains("# do not erase\n[shell]\nprogram = 'cmd.exe' # ordinary"));
        assert!(text.contains("[profiles.old]\nname='Old'\nprogram='cmd.exe'"));
        assert_eq!(loaded.settings.shell.program.as_deref(), Some("cmd.exe"));
        assert_eq!(loaded.settings.profiles.len(), 2);
        assert!(append_profile(&path, &text, &profile).is_err());
        assert!(append_profile(&path, "not [toml", &profile).is_err());
        let inline = "# keep\nprofiles = { old = { name = 'Old', program = 'cmd.exe' } } # inline comment\n[font]\nsize = 16.0 # unrelated\n";
        let (text, loaded) = append_profile(&path, inline, &profile).unwrap();
        assert!(text.contains("# keep"));
        assert!(text.contains("# inline comment"));
        assert!(text.contains("size = 16.0 # unrelated"));
        assert_eq!(loaded.settings.profiles.len(), 2);
        assert!(!format!("{loaded:?}").contains("literal $HOME"));
    }
}
