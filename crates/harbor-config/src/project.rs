//! Bounded project recipes and inert opt-in shell metadata. No filesystem/network effects.
use std::collections::BTreeMap;
use toml::{Table, Value};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ProjectEnvironment {
    LocalPwsh,
    Wsl { distribution: String },
    Ssh { destination: String },
}
impl ProjectEnvironment {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::LocalPwsh => "pwsh",
            Self::Wsl { .. } => "wsl",
            Self::Ssh { .. } => "ssh",
        }
    }
    pub fn target(&self) -> &str {
        match self {
            Self::LocalPwsh => "",
            Self::Wsl { distribution } => distribution,
            Self::Ssh { destination } => destination,
        }
    }
    pub fn new(kind: &str, target: &str) -> Option<Self> {
        // Targets are data, never options or remote command fragments. SSH config aliases supported.
        let target_valid = !target.is_empty()
            && target.len() <= 256
            && !target.starts_with('-')
            && target
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-@".contains(c));
        match kind {
            "pwsh" if target.is_empty() => Some(Self::LocalPwsh),
            "wsl" if target_valid && !target.contains('@') => Some(Self::Wsl {
                distribution: target.into(),
            }),
            "ssh" if target_valid => Some(Self::Ssh {
                destination: target.into(),
            }),
            _ => None,
        }
    }
    pub fn program(&self) -> &'static str {
        match self {
            Self::LocalPwsh => "pwsh",
            Self::Wsl { .. } => "wsl.exe",
            Self::Ssh { .. } => "ssh.exe",
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct AgentInvocation {
    pub program: String,
    pub args: Vec<String>,
}
impl std::fmt::Debug for AgentInvocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentInvocation")
            .field("program", &self.program)
            .field("argument_count", &self.args.len())
            .finish()
    }
}
impl AgentInvocation {
    pub fn supported(program: &str) -> bool {
        matches!(program, "pi" | "codex" | "claude" | "opencode")
    }
    pub fn valid(&self) -> bool {
        Self::supported(&self.program)
            && self.args.len() <= 128
            && self.args.iter().map(String::len).sum::<usize>() <= 16_384
            && self.args.iter().all(|arg| !arg.contains('\0'))
    }
}

/// Conservative warning, not a guarantee that an arbitrary argument is secret-free.
pub fn suspected_credentials(args: &[String]) -> bool {
    args.iter().any(|arg| {
        let lower = arg.to_ascii_lowercase();
        [
            "password",
            "passwd",
            "token",
            "api_key",
            "api-key",
            "apikey",
            "secret",
            "authorization",
            "bearer ",
            "://",
        ]
        .iter()
        .any(|part| lower.contains(part))
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRecipe {
    pub environment: ProjectEnvironment,
    pub directory: String,
    pub agent: Option<AgentInvocation>,
}
impl ProjectRecipe {
    pub fn validate(&self) -> Result<(), &'static str> {
        if ProjectEnvironment::new(self.environment.kind(), self.environment.target()).as_ref()
            != Some(&self.environment)
        {
            return Err("Select a supported environment and valid target");
        }
        let path = &self.directory;
        let absolute = match self.environment {
            ProjectEnvironment::LocalPwsh => {
                let bytes = path.as_bytes();
                bytes.len() >= 3
                    && bytes[0].is_ascii_alphabetic()
                    && bytes[1] == b':'
                    && matches!(bytes[2], b'/' | b'\\')
            }
            _ => path.starts_with('/') && !path.starts_with("//"),
        };
        if !absolute || path.len() > 4096 || path.chars().any(char::is_control) {
            return Err("Enter an absolute project directory in the selected environment");
        }
        if let Some(agent) = &self.agent {
            if !agent.valid() {
                return Err("Unsupported or invalid agent invocation");
            }
            if suspected_credentials(&agent.args) {
                return Err("Suspected credential arguments must be removed before saving");
            }
        }
        Ok(())
    }
    pub(crate) fn parse(value: &Value) -> Option<Self> {
        let t = value.as_table()?;
        if t.keys().any(|k| {
            !matches!(
                k.as_str(),
                "kind" | "target" | "directory" | "agent" | "agent_args"
            )
        }) {
            return None;
        }
        let environment =
            ProjectEnvironment::new(t.get("kind")?.as_str()?, t.get("target")?.as_str()?)?;
        let directory = t.get("directory")?.as_str()?.to_owned();
        let agent = match (t.get("agent"), t.get("agent_args")) {
            (None, None) => None,
            (Some(program), Some(args)) => Some(AgentInvocation {
                program: program.as_str()?.into(),
                args: args
                    .as_array()?
                    .iter()
                    .map(|arg| arg.as_str().map(str::to_owned))
                    .collect::<Option<_>>()?,
            }),
            _ => return None,
        };
        let recipe = Self {
            environment,
            directory,
            agent,
        };
        recipe.validate().ok()?;
        Some(recipe)
    }
    pub(crate) fn value(&self) -> Value {
        let mut t = Table::new();
        t.insert("kind".into(), Value::String(self.environment.kind().into()));
        t.insert(
            "target".into(),
            Value::String(self.environment.target().into()),
        );
        t.insert("directory".into(), Value::String(self.directory.clone()));
        if let Some(agent) = &self.agent {
            t.insert("agent".into(), Value::String(agent.program.clone()));
            t.insert(
                "agent_args".into(),
                Value::Array(agent.args.iter().cloned().map(Value::String).collect()),
            );
        }
        Value::Table(t)
    }
}

/// Only decoded metadata; Debug never includes argument values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectEvent {
    Unknown,
    Context {
        environment: ProjectEnvironment,
        directory: String,
    },
    Agent {
        environment: ProjectEnvironment,
        directory: String,
        invocation: AgentInvocation,
    },
    Record {
        environment: ProjectEnvironment,
        directory: String,
        invocation: AgentInvocation,
    },
}
impl ProjectEvent {
    /// OSC 777;harbor;1;{busy|context|agent};HEX(kind);HEX(target);HEX(cwd)[;HEX(agent);HEX(argv)...]
    pub fn parse(payload: &[u8]) -> Self {
        fn decode(value: &str) -> Option<String> {
            if !value.len().is_multiple_of(2) {
                return None;
            }
            let bytes = value
                .as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| {
                    let hi = (pair[0] as char).to_digit(16)?;
                    let lo = (pair[1] as char).to_digit(16)?;
                    Some((hi * 16 + lo) as u8)
                })
                .collect::<Option<Vec<_>>>()?;
            let text = String::from_utf8(bytes).ok()?;
            (!text.contains('\0')).then_some(text)
        }
        fn parse(payload: &[u8]) -> Option<ProjectEvent> {
            if payload.len() > 4000 {
                return None;
            }
            let text = std::str::from_utf8(payload).ok()?;
            let mut parts = text.strip_prefix("harbor;1;")?.split(';');
            let action = parts.next()?;
            if action == "busy" && parts.next().is_none() {
                return Some(ProjectEvent::Unknown);
            }
            let kind = decode(parts.next()?)?;
            let target = decode(parts.next()?)?;
            let directory = decode(parts.next()?)?;
            let environment = ProjectEnvironment::new(&kind, &target)?;
            ProjectRecipe {
                environment: environment.clone(),
                directory: directory.clone(),
                agent: None,
            }
            .validate()
            .ok()?;
            match action {
                "context" if parts.next().is_none() => Some(ProjectEvent::Context {
                    environment,
                    directory,
                }),
                "agent" | "record" => {
                    let program = decode(parts.next()?)?;
                    let args = parts.map(decode).collect::<Option<Vec<_>>>()?;
                    let invocation = AgentInvocation { program, args };
                    invocation.valid().then_some(if action == "agent" {
                        ProjectEvent::Agent {
                            environment,
                            directory,
                            invocation,
                        }
                    } else {
                        ProjectEvent::Record {
                            environment,
                            directory,
                            invocation,
                        }
                    })
                }
                _ => None,
            }
        }
        parse(payload).unwrap_or(Self::Unknown)
    }
}

/// One per terminal, never global. Bounded environment records; evicted records are unknown.
#[derive(Clone, Debug, Default)]
pub struct ProjectRecognition {
    current: Option<(ProjectEnvironment, String)>,
    agents: BTreeMap<ProjectEnvironment, AgentInvocation>,
}
impl ProjectRecognition {
    pub fn apply(&mut self, event: ProjectEvent) {
        let is_current = matches!(event, ProjectEvent::Agent { .. });
        match event {
            ProjectEvent::Unknown => self.current = None,
            ProjectEvent::Context {
                environment,
                directory,
            } => self.current = Some((environment, directory)),
            ProjectEvent::Agent {
                environment,
                directory,
                invocation,
            }
            | ProjectEvent::Record {
                environment,
                directory,
                invocation,
            } => {
                if self.agents.len() >= 16 && !self.agents.contains_key(&environment) {
                    self.agents.clear();
                }
                self.agents.insert(environment.clone(), invocation);
                self.current = is_current.then_some((environment, directory));
            }
        }
    }
    pub fn snapshot(&self) -> Option<ProjectRecipe> {
        let (environment, directory) = self.current.as_ref()?;
        Some(ProjectRecipe {
            environment: environment.clone(),
            directory: directory.clone(),
            agent: self.agents.get(environment).cloned(),
        })
    }
}

#[cfg(test)]
mod tests;
