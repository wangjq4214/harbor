#Requires -Version 5.1
# Consumer-visible rejection checks; runtime checks use a real built release.
[CmdletBinding()]
param([string]$ReleaseDirectory, [string]$MsiPath)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$packager = Join-Path $repoRoot 'scripts/packaging/package_windows.ps1'
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('harbor-package-tests-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporary | Out-Null

function Assert-Rejected([scriptblock]$Action, [string]$Message) {
    $rejected = $false
    try { & $Action | Out-Null } catch {
        if ($_.Exception.Message -notmatch $Message) { throw }
        $rejected = $true
    }
    if (-not $rejected) { throw "Expected rejection matching: $Message" }
}

try {
    . (Join-Path $repoRoot 'scripts/packaging/windows_package_version.ps1')
    foreach ($version in @('0.0.0', '255.255.65535')) { Assert-WindowsPackageVersion $version }
    $msiBuilder = Join-Path $repoRoot 'scripts/packaging/build_windows_msi.ps1'
    foreach ($version in @('256.0.0', '0.256.0', '0.0.65536', '18446744073709551616.0.0', '0.1.0-rc.1')) {
        $message = if ($version.Contains('-')) { 'Only stable' } else { 'cannot be represented' }
        Assert-Rejected {
            & $msiBuilder -PayloadDirectory $temporary -Version $version -OutputPath (Join-Path $temporary 'invalid.msi')
        } $message
    }
    Assert-Rejected { Assert-WindowsPackageVersion '01.0.0' } 'Only stable'
    if (Test-Path -LiteralPath (Join-Path $temporary 'invalid.msi')) { throw 'Invalid MSI version produced an artifact.' }
    Assert-Rejected { & $packager -ValidateVersionOnly -ReleaseTag 'v999.999.999' } 'must exactly match'
    Assert-Rejected { & $packager -ValidateVersionOnly -ReleaseTag 'v0.1.0-rc.1' } 'must exactly match'

    # Exercise the real entry point against minimal valid Cargo packages, not a mocked parser.
    $fixture = Join-Path $temporary 'version-fixture'
    New-Item -ItemType Directory -Path (Join-Path $fixture 'scripts/packaging') -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $fixture 'src') | Out-Null
    Copy-Item -LiteralPath $packager -Destination (Join-Path $fixture 'scripts/packaging/package_windows.ps1')
    Copy-Item -LiteralPath (Join-Path $repoRoot 'scripts/packaging/windows_package_version.ps1') -Destination (Join-Path $fixture 'scripts/packaging/windows_package_version.ps1')
    [IO.File]::WriteAllText((Join-Path $fixture 'src/main.rs'), 'fn main() {}')
    foreach ($version in @('256.0.0', '0.256.0', '0.0.65536', '0.1.0-rc.1')) {
        [IO.File]::WriteAllText((Join-Path $fixture 'Cargo.toml'), "[package]`nname = 'harbor'`nversion = '$version'`nedition = '2024'`n")
        Push-Location $fixture
        try {
            & cargo generate-lockfile
            if ($LASTEXITCODE -ne 0) { throw 'Could not prepare version fixture.' }
        } finally { Pop-Location }
        if ($version.Contains('-')) {
            Assert-Rejected { & (Join-Path $fixture 'scripts/packaging/package_windows.ps1') -ValidateVersionOnly } 'Only stable'
        } else {
            Assert-Rejected { & (Join-Path $fixture 'scripts/packaging/package_windows.ps1') -ValidateVersionOnly } 'cannot be represented'
        }
    }
    Write-Output 'PASS: mismatched/prerelease tags and MSI numeric bounds are rejected.'

    if ($ReleaseDirectory) {
        $release = (Resolve-Path -LiteralPath $ReleaseDirectory).Path
        $payload = Join-Path $temporary 'release'
        New-Item -ItemType Directory -Path $payload | Out-Null
        Copy-Item -LiteralPath (Join-Path $release 'harbor.exe') -Destination $payload
        Copy-Item -LiteralPath (Join-Path $release 'conpty') -Destination $payload -Recurse
        $output = Join-Path $temporary 'artifacts'
        $hostPath = Join-Path $payload 'conpty/x86/OpenConsole.exe'
        Remove-Item -LiteralPath $hostPath
        Assert-Rejected { & $packager -Format Zip -SkipBuild -ReleaseDirectory $payload -OutputDirectory $output } 'Missing required release payload'
        if (Test-Path -LiteralPath $output) { throw 'Missing runtime produced a final artifact directory.' }
        Copy-Item -LiteralPath (Join-Path $release 'conpty/x86/OpenConsole.exe') -Destination $hostPath
        $dll = Join-Path $payload 'conpty/conpty.dll'
        [IO.File]::WriteAllText($dll, 'not the pinned DLL')
        Assert-Rejected { & $packager -Format Zip -SkipBuild -ReleaseDirectory $payload -OutputDirectory $output } 'does not match the pinned'
        if (Test-Path -LiteralPath $output) { throw 'Mismatched runtime produced a final artifact directory.' }
        Write-Output 'PASS: missing and mismatched ConPTY payloads are rejected before artifact creation.'
        # A valid archive must expose normalized nested paths on PowerShell 5.1
        # as well as pwsh; .NET Framework's bulk ZIP creation once broke this.
        $zip = & $packager -Format Zip -SkipBuild -ReleaseDirectory $release -OutputDirectory $output
        Add-Type -AssemblyName @('System.IO.Compression', 'System.IO.Compression.FileSystem')
        $archive = [IO.Compression.ZipFile]::OpenRead($zip)
        try {
            if (@($archive.Entries | Where-Object { $_.FullName.Contains('\') }).Count -ne 0) {
                throw 'ZIP entry names contain Windows path separators.'
            }
            foreach ($relative in @('harbor.exe', 'harbor.portable', 'conpty/conpty.dll',
                'conpty/x64/OpenConsole.exe', 'conpty/arm64/OpenConsole.exe',
                'conpty/x86/OpenConsole.exe', 'conpty/LICENSE', 'conpty/VERSION')) {
                if (-not $archive.GetEntry($relative)) { throw "ZIP consumer cannot resolve required path: $relative" }
            }
        } finally { $archive.Dispose() }
        Write-Output 'PASS: real ZIP exposes complete nested runtime paths and portable marker with normalized separators.'
    }
    if ($MsiPath) {
        $installer = New-Object -ComObject WindowsInstaller.Installer
        $database = $installer.OpenDatabase((Resolve-Path -LiteralPath $MsiPath).Path, 0)
        $media = $database.OpenView('SELECT `Cabinet` FROM `Media`')
        $media.Execute() | Out-Null
        $embedded = 0
        try {
            $properties = $database.OpenView('SELECT `Value` FROM `Property` WHERE `Property`=''SecureCustomProperties''')
            $properties.Execute() | Out-Null
            try {
                $row = $properties.Fetch()
                if ($null -eq $row -or 'APPLICATIONFOLDER' -cnotin $row.StringData(1).Split(';')) {
                    throw 'The selected installation directory is not preserved across elevation.'
                }
            } finally {
                $properties.Close() | Out-Null
                [Runtime.InteropServices.Marshal]::FinalReleaseComObject($properties) | Out-Null
            }
            while ($row = $media.Fetch()) {
                $cabinet = $row.StringData(1)
                if (-not $cabinet -or -not $cabinet.StartsWith('#')) {
                    throw 'MSI requires an external cabinet; a standalone download would fail to install.'
                }
                $name = $cabinet.Substring(1).Replace("'", "''")
                $stream = $database.OpenView("SELECT ``Name`` FROM ``_Streams`` WHERE ``Name``='$name'")
                $stream.Execute() | Out-Null
                try {
                    if ($null -eq $stream.Fetch()) { throw "MSI cabinet stream is missing: $cabinet" }
                } finally {
                    $stream.Close() | Out-Null
                    [Runtime.InteropServices.Marshal]::FinalReleaseComObject($stream) | Out-Null
                }
                $embedded++
            }
            if ($embedded -eq 0) { throw 'MSI has no embedded application payload cabinet.' }
        } finally {
            $media.Close() | Out-Null
            foreach ($item in @($media, $database, $installer)) {
                [Runtime.InteropServices.Marshal]::FinalReleaseComObject($item) | Out-Null
            }
        }
        Write-Output 'PASS: MSI payload cabinets are embedded and present; no sibling cabinet download is needed.'

        # Reproduce an interactive switch after MSI cached per-user standard
        # folders. Costing only: no InstallInitialize/InstallFiles actions run.
        if (-not ('HarborPackagingMsiUi' -as [type])) {
            Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class HarborPackagingMsiUi {
    [DllImport("msi.dll")]
    public static extern uint MsiSetInternalUI(uint level, IntPtr owner);
}
'@
        }
        $previousUi = [HarborPackagingMsiUi]::MsiSetInternalUI(2, [IntPtr]::Zero)
        $costInstaller = $null
        $session = $null
        try {
            $costInstaller = New-Object -ComObject WindowsInstaller.Installer
            $session = $costInstaller.OpenPackage((Resolve-Path -LiteralPath $MsiPath).Path, 0)
            $session.GetType().InvokeMember('Property', [Reflection.BindingFlags]::SetProperty,
                $null, $session, @('ALLUSERS', '1')) | Out-Null
            $session.GetType().InvokeMember('Property', [Reflection.BindingFlags]::SetProperty,
                $null, $session, @('MSIINSTALLPERUSER', '')) | Out-Null
            foreach ($action in @('AppSearch', 'CostInitialize', 'FileCost', 'HarborMachineDirectory', 'CostFinalize')) {
                if ($session.DoAction($action) -ne 1) { throw "MSI costing action failed: $action" }
            }
            $expected = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Harbor'
            $actual = $session.TargetPath('APPLICATIONFOLDER').TrimEnd('\')
            if ($actual -ine $expected) {
                throw "Late machine-scope choice retained a redirected directory: $actual; expected $expected"
            }
        } finally {
            foreach ($item in @($session, $costInstaller)) {
                if ($null -ne $item) { [Runtime.InteropServices.Marshal]::FinalReleaseComObject($item) | Out-Null }
            }
            [HarborPackagingMsiUi]::MsiSetInternalUI($previousUi, [IntPtr]::Zero) | Out-Null
        }
        Write-Output 'PASS: late machine-scope selection costs the native Program Files path without installing.'
    }
} finally {
    Remove-Item -LiteralPath $temporary -Recurse -Force
}
