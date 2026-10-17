#Requires -Version 5.1
# Shared by the local packager and the independently callable MSI builder.
function Assert-WindowsPackageVersion([string]$Version) {
    if ($Version -cnotmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') {
        throw "Only stable MAJOR.MINOR.PATCH package versions are supported: $Version"
    }
    $parts = $Version.Split('.')
    $bounds = @(255, 255, 65535)
    for ($i = 0; $i -lt 3; $i++) {
        [UInt64]$number = 0
        if (-not [UInt64]::TryParse($parts[$i], [ref]$number) -or $number -gt $bounds[$i]) {
            throw "Version $Version cannot be represented by Windows Installer (255.255.65535 maximum)."
        }
    }
}
