function Get-ImageArchitecture([string]$Path) {
    $reader = [IO.BinaryReader]::new([IO.File]::OpenRead($Path))
    try {
        if ($reader.ReadUInt16() -ne 0x5A4D) { throw "Not a PE image: $Path" }
        $reader.BaseStream.Position = 0x3C
        $offset = $reader.ReadUInt32()
        if ($offset -gt $reader.BaseStream.Length - 6) { throw "Invalid PE header: $Path" }
        $reader.BaseStream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x00004550) { throw "Not a PE image: $Path" }
        switch ($reader.ReadUInt16()) {
            0x8664 { return 'x64' }
            0xAA64 { return 'arm64' }
            0x014C { return 'x86' }
            default { throw "Unsupported PE machine in $Path" }
        }
    } finally {
        $reader.Dispose()
    }
}


function Stage-Conpty([string]$executable, [string]$repoRoot) {
    $arch = Get-ImageArchitecture $executable
    $source = Join-Path $repoRoot 'third_party/conpty'
    $files = [ordered]@{ 'conpty.dll' = "$arch/conpty.dll" }
    foreach ($hostArch in @('x64', 'arm64', 'x86')) {
        $files["$hostArch/OpenConsole.exe"] = "$hostArch/OpenConsole.exe"
    }
    $files['LICENSE'] = 'LICENSE'
    $files['VERSION'] = 'VERSION'
    # Validate the entire source before touching the destination. Do not fetch
    # dependencies or change the user's toolchain/configuration automatically.
    foreach ($relative in $files.Values) {
        if (-not (Test-Path -LiteralPath (Join-Path $source $relative) -PathType Leaf)) {
            throw "Missing bundled ConPTY source: $relative. Run ./scripts/setup/fetch_conpty.ps1 first."
        }
    }
    $runtime = Join-Path (Split-Path -Parent $executable) 'conpty'
    foreach ($entry in $files.GetEnumerator()) {
        $from = Join-Path $source $entry.Value
        $to = Join-Path $runtime $entry.Key
        if ((Test-Path -LiteralPath $to -PathType Leaf) -and
            (Get-FileHash -LiteralPath $from).Hash -eq (Get-FileHash -LiteralPath $to).Hash) {
            continue
        }
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $to) | Out-Null
        Copy-Item -LiteralPath $from -Destination $to -Force
    }
    Write-Host "ConPTY ($arch) ready: $runtime"
}
