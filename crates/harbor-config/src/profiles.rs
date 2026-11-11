use std::{borrow::Borrow, collections::BTreeMap};

use toml::Value;

use crate::{Diagnostic, DiagnosticLevel, ShellSettings, error, parse_shell};

/// Persistent identity: the TOML table key, independent of its editable display name.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Hash)]
pub struct ProfileId(String);

impl ProfileId {
    pub fn new(id: impl Into<String>) -> Option<Self> {
        let id = id.into();
        (!id.is_empty()
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_')))
        .then_some(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for ProfileId {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

/// Independent recipe; omitted launch fields use empty/inherited child defaults,
/// never another profile or `[shell]`. Accepted profiles always have a program.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionProfile {
    pub name: String,
    pub launch: ShellSettings,
    pub project: Option<crate::ProjectRecipe>,
}

pub(super) fn parse(
    value: Option<&Value>,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeMap<ProfileId, SessionProfile> {
    let mut profiles = BTreeMap::new();
    let Some(value) = value else { return profiles };
    let Some(table) = value.as_table() else {
        diagnostics.push(error("profiles must be a table; profiles unavailable"));
        return profiles;
    };
    for (key, value) in table {
        let Some(id) = ProfileId::new(key.clone()) else {
            diagnostics.push(error("profile identity must contain only ASCII letters, digits, '-' or '_' and must not be empty"));
            continue;
        };
        let Some(table) = value.as_table() else {
            diagnostics.push(error("profile must be a table; profile unavailable"));
            continue;
        };
        let start = diagnostics.len();
        let name = table.get("name").and_then(Value::as_str).map(str::trim);
        if name.is_none_or(|name| name.is_empty() || name.chars().any(char::is_control)) {
            diagnostics.push(error(
                "profiles.name must be a non-empty display name without control characters",
            ));
        }
        if name.is_some_and(|name| profiles.values().any(|p: &SessionProfile| p.name == name)) {
            diagnostics.push(error("profiles.name must be unique (case-sensitive)"));
        }
        let mut launch_table = table.clone();
        launch_table.remove("name");
        let project = launch_table.remove("project").and_then(|value| {
            let recipe = crate::ProjectRecipe::parse(&value);
            if recipe.is_none() {
                diagnostics.push(error("profiles.project is invalid (values omitted)"));
            }
            recipe
        });
        if launch_table.keys().any(|key| {
            !matches!(
                key.as_str(),
                "program" | "args" | "directory" | "environment"
            )
        }) {
            diagnostics.push(error(
                "profile contains unsupported fields; inheritance is not supported",
            ));
        }
        let mut launch = ShellSettings::default();
        parse_shell(Some(&Value::Table(launch_table)), &mut launch, diagnostics);
        if launch.program.is_none() {
            diagnostics.push(error("profiles.program is required; profile unavailable"));
        }
        if let Some(project) = &project
            && (launch.program.as_deref() != Some(project.environment.program())
                || !launch.args.is_empty()
                || launch.directory.is_some())
        {
            diagnostics.push(error("project profiles require the environment launcher, empty args and no native directory"));
        }
        for diagnostic in &mut diagnostics[start..] {
            diagnostic.message = diagnostic.message.replace("shell.", "profiles.");
            if diagnostic.level == DiagnosticLevel::Error {
                diagnostic.message = diagnostic
                    .message
                    .split("; using")
                    .next()
                    .unwrap()
                    .to_owned();
            }
        }
        if diagnostics[start..]
            .iter()
            .all(|d| d.level != DiagnosticLevel::Error)
        {
            profiles.insert(
                id,
                SessionProfile {
                    name: name.unwrap().to_owned(),
                    launch,
                    project,
                },
            );
        }
    }
    profiles
}
