# Sends a bounded ASCII line to the current foreground window as real virtual-key
# events. Harbor maps terminal input from WM_KEYDOWN logical keys, so WM_CHAR-only
# automation does not reach the child; this helper is for synthetic measurement
# drivers only. Never targets a window implicitly: the caller must foreground the
# owned test window first and verify the echoed command.
param(
    [Parameter(Mandatory = $true)][string]$Text,
    [int]$DelayMs = 20
)

$ErrorActionPreference = 'Stop'
Add-Type -Namespace T0004 -Name Native -MemberDefinition @'
[DllImport("user32.dll")] public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, System.UIntPtr dwExtraInfo);
'@

function Send-Vk([byte]$vk) {
    [T0004.Native]::keybd_event($vk, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds $DelayMs
    [T0004.Native]::keybd_event($vk, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds $DelayMs
}

$shift = @{
    ':' = 0xBA
    '_' = 0xBD
    '"' = 0xDE
    '|' = 0xDC
    '<' = 0xBC
    '>' = 0xBE
    '?' = 0xBF
    '+' = 0xBB
}

foreach ($ch in $Text.ToCharArray()) {
    if ($ch -eq "`n" -or $ch -eq "`r") {
        Send-Vk 0x0D
    }
    elseif ($ch -eq ' ') {
        Send-Vk 0x20
    }
    elseif ($ch -ge 'a' -and $ch -le 'z') {
        Send-Vk ([byte](0x41 + ([int][char]$ch - [int][char]'a')))
    }
    elseif ($ch -ge 'A' -and $ch -le 'Z') {
        Send-Vk 0x10
        Send-Vk ([byte](0x41 + ([int][char]$ch - [int][char]'A')))
    }
    elseif ($ch -ge '0' -and $ch -le '9') {
        Send-Vk ([byte](0x30 + ([int][char]$ch - [int][char]'0')))
    }
    elseif ($shift.ContainsKey([string]$ch)) {
        Send-Vk 0x10
        Send-Vk ([byte]$shift[[string]$ch])
    }
    elseif ($ch -eq '-') {
        Send-Vk 0xBD
    }
    elseif ($ch -eq '.') {
        Send-Vk 0xBE
    }
    elseif ($ch -eq ',') {
        Send-Vk 0xBC
    }
    elseif ($ch -eq '/') {
        Send-Vk 0xBF
    }
    elseif ($ch -eq ';') {
        Send-Vk 0xBA
    }
    elseif ($ch -eq '=') {
        Send-Vk 0xBB
    }
    else {
        throw "unsupported character for synthetic driver: U+$([int][char]$ch | ForEach-Object { $_.ToString('X4') })"
    }
}
