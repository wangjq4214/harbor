# Opt-in, local PowerShell 7.3+: . /path/to/harbor-project.ps1
# Current process only. No profile-file changes or remote installs.
if ($PSVersionTable.PSVersion -lt [version]'7.3' -or $env:SSH_CONNECTION -or $env:WSL_DISTRO_NAME) {
    throw 'Harbor local integration requires native local PowerShell 7.3+ (no nested environment).'
}
if (Get-Variable HarborProjectEnabled -Scope Global -ErrorAction Ignore) { return }
Import-Module PSReadLine -ErrorAction Stop
foreach ($name in @('pi', 'codex', 'claude', 'opencode')) {
    if (Get-Command $name -CommandType Function,Alias -ErrorAction Ignore) {
        throw 'Harbor refuses to replace existing agent aliases/functions.'
    }
}
$global:HarborProjectOldPrompt = (Get-Command prompt).ScriptBlock
$global:HarborProjectOldHistoryHandler = (Get-PSReadLineOption).AddToHistoryHandler
function global:Send-HarborProject {
    param([string]$Action, [string[]]$Arguments = @())
    $location = Get-Location
    if ($location.Provider.Name -ne 'FileSystem') {
        [Console]::Write("`e]777;harbor;1;busy`a")
        return
    }
    $parts = @('harbor', '1', $Action)
    foreach ($part in (@('pwsh', '', $location.Path) + $Arguments)) {
        $parts += [Convert]::ToHexString([Text.Encoding]::UTF8.GetBytes([string]$part))
    }
    $payload = $parts -join ';'
    if ($payload.Length -le 4000) { [Console]::Write("`e]777;$payload`a") }
    else { [Console]::Write("`e]777;harbor;1;busy`a") }
}
function global:Invoke-HarborAgent {
    param([string]$Name, [AllowEmptyCollection()][AllowEmptyString()][string[]]$Arguments)
    $command = Get-Command $Name -CommandType Application,ExternalScript -ErrorAction Stop | Select-Object -First 1
    $action = if ($global:HarborProjectSingleAgent -eq $Name) { 'agent' } else { 'record' }
    Send-HarborProject -Action $action -Arguments (@($Name) + $Arguments)
    $PSNativeCommandArgumentPassing = 'Standard'
    & $command @Arguments
}
function global:pi { Invoke-HarborAgent -Name pi -Arguments $args }
function global:codex { Invoke-HarborAgent -Name codex -Arguments $args }
function global:claude { Invoke-HarborAgent -Name claude -Arguments $args }
function global:opencode { Invoke-HarborAgent -Name opencode -Arguments $args }
function global:prompt {
    $result = & $global:HarborProjectOldPrompt
    Send-HarborProject -Action context
    $result
}
Set-PSReadLineOption -AddToHistoryHandler {
    param($line)
    # Do not parse, retain or replay ordinary commands; invalidate before execution.
    [Console]::Write("`e]777;harbor;1;busy`a")
    # Only a single direct command has a known shell cwd while it runs. Compound
    # lines/scripts may change cwd afterwards; retain their invocation but not cwd.
    $global:HarborProjectSingleAgent = $null
    $tokens = $null; $errors = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseInput($line, [ref]$tokens, [ref]$errors)
    $statements = $ast.EndBlock.Statements
    if (!$errors -and $statements.Count -eq 1 -and $statements[0] -is [System.Management.Automation.Language.PipelineAst] -and $statements[0].PipelineElements.Count -eq 1) {
        $command = $statements[0].PipelineElements[0]
        if ($command -is [System.Management.Automation.Language.CommandAst]) {
            $name = $command.GetCommandName()
            if ($name -in @('pi', 'codex', 'claude', 'opencode')) { $global:HarborProjectSingleAgent = $name }
        }
    }
    if ($null -ne $global:HarborProjectOldHistoryHandler) {
        return $global:HarborProjectOldHistoryHandler.Invoke($line)
    }
    return $true
}
$global:HarborProjectEnabled = $true
Send-HarborProject -Action context
