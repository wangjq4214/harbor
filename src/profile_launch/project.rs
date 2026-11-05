//! Fixed project launch templates. All interpolated values are literal arguments, never scripts.
use harbor_config::{ProjectEnvironment, ProjectRecipe};
use harbor_pty::{ExecutablePolicy, ShellCommand};

pub(super) fn resolve(
    profile: &harbor_config::SessionProfile,
    recipe: &ProjectRecipe,
) -> anyhow::Result<ShellCommand> {
    recipe.validate().map_err(anyhow::Error::msg)?;
    anyhow::ensure!(
        profile.launch.program.as_deref() == Some(recipe.environment.program())
            && profile.launch.args.is_empty()
            && profile.launch.directory.is_none(),
        "invalid project launcher"
    );
    let mut command = ShellCommand::new(Some(recipe.environment.program().into()), Vec::new());
    command.executable_policy = ExecutablePolicy::Strict;
    command.environment = profile.launch.environment.clone();
    match &recipe.environment {
        ProjectEnvironment::LocalPwsh => {
            let directory = std::path::PathBuf::from(&recipe.directory);
            anyhow::ensure!(directory.is_dir(), "project directory is unavailable");
            command.directory = Some(directory);
            // PS7 native argument passing preserves empty/quoted args, including .exe clients.
            let mut script = format!(
                "$ErrorActionPreference='Stop'; Set-Location -LiteralPath {};",
                pwsh_quote(&recipe.directory)
            );
            if let Some(agent) = &recipe.agent {
                script.push_str(" if ($PSVersionTable.PSVersion -lt [version]'7.3') { throw 'Project agents require PowerShell 7.3+' }; $PSNativeCommandArgumentPassing='Standard';");
                script.push_str(&format!(" $a=@({}); $p=Get-Command {} -CommandType Application,ExternalScript -ErrorAction Stop | Select-Object -First 1; if ($p.Extension -in @('.cmd','.bat')) {{ throw 'Use an executable or PowerShell agent entry point; batch shims cannot preserve argv' }};", agent.args.iter().map(|s| pwsh_quote(s)).collect::<Vec<_>>().join(","), pwsh_quote(&agent.program)));
                script.push_str(&format!(
                    " [Console]::Write({});",
                    pwsh_quote(&agent_metadata(recipe))
                ));
                script.push_str(" try { & $p @a } finally { [Console]::Write([char]27 + \"]777;harbor;1;busy\" + [char]7) };");
            }
            command.args = vec![
                "-NoLogo".into(),
                "-NoExit".into(),
                "-Command".into(),
                script,
            ];
        }
        ProjectEnvironment::Wsl { distribution } => {
            command.args = vec![
                "--distribution".into(),
                distribution.clone(),
                "--exec".into(),
                "bash".into(),
                "-lc".into(),
                bash_script(recipe),
            ];
        }
        ProjectEnvironment::Ssh { destination } => {
            // SSH joins command arguments into a remote shell string. Quote the entire bash
            // script again at that distinct boundary, rather than relying on Windows argv quotes.
            command.args = vec![
                "-t".into(),
                "--".into(),
                destination.clone(),
                format!("bash -lc {}", posix_quote(&bash_script(recipe))),
            ];
        }
    }
    Ok(command)
}
fn bash_script(recipe: &ProjectRecipe) -> String {
    let mut script = format!("cd -- {} || exit; ", posix_quote(&recipe.directory));
    if let Some(agent) = &recipe.agent {
        script.push_str(&format!(
            "printf %s {}; ",
            posix_quote(&agent_metadata(recipe))
        ));
        script.push_str("command ");
        script.push_str(&posix_quote(&agent.program));
        for arg in &agent.args {
            script.push(' ');
            script.push_str(&posix_quote(arg));
        }
        script.push_str("; printf '\\033]777;harbor;1;busy\\007'; ");
    }
    script.push_str("exec bash -i");
    script
}
fn agent_metadata(recipe: &ProjectRecipe) -> String {
    let agent = recipe
        .agent
        .as_ref()
        .expect("agent metadata requires an agent");
    let fields = [
        recipe.environment.kind(),
        recipe.environment.target(),
        recipe.directory.as_str(),
        agent.program.as_str(),
    ]
    .into_iter()
    .chain(agent.args.iter().map(String::as_str))
    .map(|value| {
        value
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    })
    .collect::<Vec<_>>()
    .join(";");
    let payload = format!("harbor;1;agent;{fields}");
    if payload.len() > 4000 {
        return "\x1b]777;harbor;1;busy\x07".into();
    }
    format!("\x1b]777;{payload}\x07")
}
fn posix_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
fn pwsh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
#[path = "project_tests.rs"]
mod tests;
