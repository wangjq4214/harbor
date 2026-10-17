#Requires -Version 5.1
# Build once, validate the matching release payload, then produce MSI and/or ZIP.
[CmdletBinding()]
param(
    [ValidateSet('Both', 'Msi', 'Zip')][string]$Format = 'Both',
    [string]$ReleaseTag,
    [string]$OutputDirectory,
    [switch]$SkipBuild,
    [string]$ReleaseDirectory,
    [switch]$ValidateVersionOnly
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
. (Join-Path $PSScriptRoot 'windows_package_version.ps1')

function Convert-PeRvaToOffset([UInt32]$Rva, [object[]]$Sections) {
    foreach ($section in $Sections) {
        if ($Rva -ge $section.Address -and
            [UInt64]$Rva -lt ([UInt64]$section.Address + $section.Size)) {
            return [Int64]$section.Offset + $Rva - $section.Address
        }
    }
    throw "PE import RVA is not backed by a file section: $Rva"
}

function Assert-X64Image([string]$Path, [switch]$StaticCrt) {
    $stream = [IO.File]::OpenRead($Path)
    $reader = New-Object IO.BinaryReader($stream)
    try {
        if ($reader.ReadUInt16() -ne 0x5A4D) { throw "Not a PE image: $Path" }
        $stream.Position = 0x3C
        $offset = $reader.ReadUInt32()
        if ($offset -gt $stream.Length - 6) { throw "Invalid PE header: $Path" }
        $stream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x00004550 -or $reader.ReadUInt16() -ne 0x8664) {
            throw "Expected an x64 PE image: $Path"
        }
        if ($StaticCrt) {
            # PE import descriptors use RVAs; resolve them through section headers.
            # https://learn.microsoft.com/en-us/windows/win32/debug/pe-format#import-directory-table
            $stream.Position = $offset + 6
            $sectionCount = $reader.ReadUInt16()
            $stream.Position = $offset + 20
            $optionalSize = $reader.ReadUInt16()
            $optional = $offset + 24
            $stream.Position = $optional
            if ($reader.ReadUInt16() -ne 0x20B -or $optionalSize -lt 128) { throw "Invalid x64 optional header: $Path" }
            $stream.Position = $optional + 120
            $importRva = $reader.ReadUInt32()
            $importSize = $reader.ReadUInt32()
            $sections = @()
            for ($i = 0; $i -lt $sectionCount; $i++) {
                $stream.Position = $optional + $optionalSize + $i * 40 + 12
                $sections += [PSCustomObject]@{
                    Address = $reader.ReadUInt32()
                    Size = $reader.ReadUInt32()
                    Offset = $reader.ReadUInt32()
                }
            }
            if ($importRva -ne 0) {
                $start = Convert-PeRvaToOffset $importRva $sections
                $end = [Math]::Min($stream.Length, $start + $importSize)
                for ($descriptor = $start; $descriptor + 20 -le $end; $descriptor += 20) {
                    $stream.Position = $descriptor + 12
                    $nameRva = $reader.ReadUInt32()
                    if ($nameRva -eq 0) { break }
                    $stream.Position = Convert-PeRvaToOffset $nameRva $sections
                    $name = New-Object Text.StringBuilder
                    while (($character = $reader.ReadByte()) -ne 0) { [void]$name.Append([char]$character) }
                    if ($name.ToString() -match '^(?i:vcruntime|msvcp|msvcr|concrt)[0-9].*\.dll$') {
                        throw "Release binary imports $name. Rebuild with static CRT (-C target-feature=+crt-static), including before -SkipBuild packaging."
                    }
                }
            }
        }
    } finally {
        $reader.Dispose()
    }
}

Push-Location $repoRoot
try {
    $metadataText = & cargo metadata --locked --no-deps --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed.' }
    $metadata = $metadataText | ConvertFrom-Json
    $package = @($metadata.packages | Where-Object { $_.name -eq 'harbor' -and $_.manifest_path -eq (Join-Path $repoRoot 'Cargo.toml') })
    if ($package.Count -ne 1) { throw 'Could not uniquely identify the root Harbor package.' }
    $version = $package[0].version
    Assert-WindowsPackageVersion $version
    if ($ReleaseTag -and $ReleaseTag -cne "v$version") {
        throw "Release tag '$ReleaseTag' must exactly match stable Cargo version 'v$version'."
    }
    if ($ValidateVersionOnly) {
        Write-Output "Validated Windows release version $version"
        return
    }
    if ($SkipBuild -and -not $ReleaseDirectory) {
        throw '-SkipBuild requires an explicit -ReleaseDirectory containing the matching release build.'
    }
    if (-not $SkipBuild -and $ReleaseDirectory) {
        throw '-ReleaseDirectory is only supported with -SkipBuild.'
    }
    if (-not $SkipBuild) {
        & (Join-Path $repoRoot 'scripts/setup/fetch_conpty.ps1')
        # Apply the CRT policy only to this target release build, not dev builds.
        # Encoded flags take precedence and use Cargo's ASCII 0x1f separator.
        $previousRustFlags = $env:RUSTFLAGS
        $previousEncodedFlags = $env:CARGO_ENCODED_RUSTFLAGS
        try {
            if ($null -ne $previousEncodedFlags) {
                $env:CARGO_ENCODED_RUSTFLAGS = $previousEncodedFlags + [char]31 + '-Ctarget-feature=+crt-static'
            } else {
                $env:RUSTFLAGS = ($previousRustFlags + ' -Ctarget-feature=+crt-static').Trim()
            }
            & cargo build --locked --release --target x86_64-pc-windows-msvc --bin harbor
            if ($LASTEXITCODE -ne 0) { throw 'Harbor x64 static-CRT release build failed.' }
        } finally {
            $env:RUSTFLAGS = $previousRustFlags
            $env:CARGO_ENCODED_RUSTFLAGS = $previousEncodedFlags
        }
        $ReleaseDirectory = Join-Path $metadata.target_directory 'x86_64-pc-windows-msvc/release'
    }
    $ReleaseDirectory = (Resolve-Path -LiteralPath $ReleaseDirectory).Path
    $binary = Join-Path $ReleaseDirectory 'harbor.exe'
    if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) { throw "Missing release binary: $binary" }
    Assert-X64Image $binary -StaticCrt
    $binaryVersion = [Diagnostics.FileVersionInfo]::GetVersionInfo($binary).ProductVersion
    if ($binaryVersion -cne $version) {
        throw "Release binary ProductVersion '$binaryVersion' does not match Cargo version '$version'. Rebuild rather than relabeling a stale binary."
    }

    # One common file list, consumed unchanged by both package formats.
    $payloadFiles = @('harbor.exe', 'conpty/conpty.dll',
        'conpty/x64/OpenConsole.exe', 'conpty/arm64/OpenConsole.exe',
        'conpty/x86/OpenConsole.exe', 'conpty/LICENSE', 'conpty/VERSION')
    $sourceRuntime = Join-Path $repoRoot 'third_party/conpty'
    $hashManifest = @{}
    foreach ($line in Get-Content -LiteralPath (Join-Path $sourceRuntime 'SHA256SUMS')) {
        if ($line -match '^([0-9A-Fa-f]{64})  (.+)$') { $hashManifest[$Matches[2]] = $Matches[1] }
    }
    foreach ($relative in $payloadFiles) {
        $file = Join-Path $ReleaseDirectory $relative
        if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "Missing required release payload: $relative" }
        if ($relative -eq 'harbor.exe') { continue }
        $runtimeRelative = $relative.Substring('conpty/'.Length)
        $pinnedRelative = $runtimeRelative
        if ($runtimeRelative -eq 'conpty.dll') { $pinnedRelative = 'x64/conpty.dll' }
        $pinned = Join-Path $sourceRuntime $pinnedRelative
        if (-not (Test-Path -LiteralPath $pinned -PathType Leaf)) { throw "Missing pinned runtime source: $pinnedRelative" }
        $hash = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash
        if ($hash -ne (Get-FileHash -LiteralPath $pinned -Algorithm SHA256).Hash) {
            throw "Release payload does not match the pinned ConPTY distribution: $relative"
        }
        if ($pinnedRelative -match '\.(dll|exe)$' -and
            (-not $hashManifest.ContainsKey($pinnedRelative) -or $hash -ne $hashManifest[$pinnedRelative])) {
            throw "ConPTY payload does not match SHA256SUMS: $relative"
        }
    }
    Assert-X64Image (Join-Path $ReleaseDirectory 'conpty/conpty.dll')
    if (-not $OutputDirectory) { $OutputDirectory = Join-Path $metadata.target_directory 'dist' }
    $OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
    New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
    $temporary = Join-Path $OutputDirectory ('.package-' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $temporary | Out-Null
    try {
        $payload = Join-Path $temporary 'payload'
        New-Item -ItemType Directory -Path $payload | Out-Null
        foreach ($relative in $payloadFiles) {
            $destination = Join-Path $payload $relative
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
            Copy-Item -LiteralPath (Join-Path $ReleaseDirectory $relative) -Destination $destination
        }
        foreach ($relative in @('LICENSE', 'config.example.toml')) {
            Copy-Item -LiteralPath (Join-Path $repoRoot $relative) -Destination (Join-Path $payload $relative)
        }
        $artifacts = @()
        if ($Format -in @('Both', 'Msi')) {
            $msi = Join-Path $temporary "harbor-$version-windows-x64.msi"
            & (Join-Path $PSScriptRoot 'build_windows_msi.ps1') -PayloadDirectory $payload -Version $version -OutputPath $msi
            if (-not (Test-Path -LiteralPath $msi -PathType Leaf)) { throw 'MSI build did not produce the requested output.' }
            $artifacts += $msi
        }
        if ($Format -in @('Both', 'Zip')) {
            # Added only after MSI construction so no portable marker can leak into MSI.
            [IO.File]::WriteAllText((Join-Path $payload 'harbor.portable'), '')
            $zip = Join-Path $temporary "harbor-$version-windows-x64-portable.zip"
            Add-Type -AssemblyName @('System.IO.Compression', 'System.IO.Compression.FileSystem')
            $zipFiles = $payloadFiles + @('LICENSE', 'config.example.toml', 'harbor.portable')
            $archive = [IO.Compression.ZipFile]::Open($zip, [IO.Compression.ZipArchiveMode]::Create)
            try {
                # .NET Framework's CreateFromDirectory writes backslashes on Windows.
                # Explicit ZIP entry names preserve portable forward-slash paths.
                foreach ($relative in $zipFiles) {
                    [IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
                        $archive, (Join-Path $payload $relative), $relative,
                        [IO.Compression.CompressionLevel]::Optimal) | Out-Null
                }
            } finally { $archive.Dispose() }
            $archive = [IO.Compression.ZipFile]::OpenRead($zip)
            try {
                foreach ($relative in $zipFiles) {
                    if (-not $archive.GetEntry($relative)) { throw "ZIP is missing $relative" }
                }
            } finally { $archive.Dispose() }
            $artifacts += $zip
        }
        # No final assets are returned/published until every requested format succeeded.
        foreach ($artifact in $artifacts) {
            $destination = Join-Path $OutputDirectory (Split-Path -Leaf $artifact)
            Move-Item -LiteralPath $artifact -Destination $destination -Force
            Write-Output $destination
        }
    } finally {
        Remove-Item -LiteralPath $temporary -Recurse -Force
    }
} finally {
    Pop-Location
}
