#Requires -Version 5.1
# Restore only the pinned Windows App SDK metadata and interop header needed to regenerate bindings.
[CmdletBinding()]
param([string]$PackageDirectory)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$destination = Join-Path $repoRoot 'third_party/wasdk'
$cache = Join-Path $repoRoot 'target/wasdk-download'
$packages = @(
    @{
        Name = 'interactive'; Id = 'microsoft.windowsappsdk.interactiveexperiences'
        Version = '1.8.260708001'
        Sha256 = '496EEA92D353B5D3601B67353F06DCADD6D2D9B635575ACEBE6E42587DBFAD76'
    },
    @{
        Name = 'foundation'; Id = 'microsoft.windowsappsdk.foundation'
        Version = '1.8.260803002'
        Sha256 = 'B9232041AFD605B606C6F78F442D92EAD0076453F1F2A3260D2B7F8089BCAB0E'
    }
)
New-Item -ItemType Directory -Force $cache | Out-Null
$staged = @()
try {
    foreach ($package in $packages) {
        $fileName = "$($package.Name).nupkg"
        if ($PackageDirectory) {
            $archive = Join-Path $PackageDirectory $fileName
        } else {
            $archive = Join-Path $cache $fileName
            if ((Test-Path -LiteralPath $archive) -and
                (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $package.Sha256) {
                Remove-Item -LiteralPath $archive -Force
            }
            if (-not (Test-Path -LiteralPath $archive)) {
                $url = "https://api.nuget.org/v3-flatcontainer/$($package.Id)/$($package.Version)/$($package.Id).$($package.Version).nupkg"
                $temporary = Join-Path $cache ([IO.Path]::GetRandomFileName())
                try {
                    Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $temporary
                    if ((Get-FileHash -LiteralPath $temporary -Algorithm SHA256).Hash -ne $package.Sha256) {
                        throw "WASDK $($package.Name) package SHA256 mismatch; no SDK files were changed."
                    }
                    Move-Item -LiteralPath $temporary -Destination $archive -Force
                } finally {
                    Remove-Item -LiteralPath $temporary -Force -ErrorAction SilentlyContinue
                }
            }
        }
        if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $package.Sha256) {
            throw "WASDK $($package.Name) package SHA256 mismatch; no SDK files were changed."
        }
        $zip = Join-Path $cache "$($package.Name).zip"
        $extracted = Join-Path $cache $package.Name
        Copy-Item -LiteralPath $archive -Destination $zip -Force
        if (Test-Path -LiteralPath $extracted) {
            Remove-Item -LiteralPath $extracted -Recurse -Force
        }
        Expand-Archive -LiteralPath $zip -DestinationPath $extracted
        $metadata = if ($package.Name -eq 'interactive') {
            Join-Path $extracted 'metadata/10.0.18362.0'
        } else {
            Join-Path $extracted 'metadata'
        }
        if (-not (Test-Path -LiteralPath $metadata)) {
            throw "WASDK $($package.Name) metadata missing; no SDK files were changed."
        }
        if ($package.Name -eq 'interactive') {
            $header = Join-Path $extracted 'include/Microsoft.UI.Interop.h'
            if (-not (Test-Path -LiteralPath $header)) {
                throw 'WASDK interop header missing; no SDK files were changed.'
            }
        }
        $staged += @{ Name = $package.Name; Root = $extracted }
    }
    foreach ($item in $staged) {
        $target = Join-Path $destination $item.Name
        New-Item -ItemType Directory -Force $target | Out-Null
        # Only metadata and the ABI reference header are needed; binaries stay out of the repository.
        Copy-Item -LiteralPath (Join-Path $item.Root 'metadata') -Destination $target -Recurse -Force
        if ($item.Name -eq 'interactive') {
            $include = Join-Path $target 'include'
            New-Item -ItemType Directory -Force $include | Out-Null
            Copy-Item -LiteralPath (Join-Path $item.Root 'include/Microsoft.UI.Interop.h') -Destination $include -Force
        }
    }
    Write-Output 'Restored pinned Windows App SDK binding metadata and interop header.'
} finally {
    foreach ($package in $packages) {
        Remove-Item -LiteralPath (Join-Path $cache "$($package.Name).zip") -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath (Join-Path $cache $package.Name) -Recurse -Force -ErrorAction SilentlyContinue
    }
}
