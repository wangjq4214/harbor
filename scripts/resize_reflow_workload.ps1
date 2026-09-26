[CmdletBinding()]
param(
    [ValidateRange(1, 100000)]
    [int]$Lines = 1100,

    [ValidateRange(16, 4096)]
    [int]$PayloadWidth = 96,

    [ValidateRange(1, 2147483647)]
    [int]$Seed = 152170
)

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)

$esc = [char]27
$palette = @(31, 32, 33, 34, 35, 36, 91, 92, 93, 94, 95, 96)
$alphabet = '0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ'
$cjk = [char]0x754c

[Console]::Out.WriteLine("HARBOR_RESIZE_REFLOW_BEGIN lines=$Lines payload_width=$PayloadWidth seed=$Seed")
[Console]::Out.WriteLine('Legend: numbered colored/CJK lines; blank rows occur in consecutive pairs; each data line ends with three ordinary spaces.')
[Console]::Out.WriteLine()

for ($index = 0; $index -lt $Lines; $index++) {
    $color = $palette[($index + $Seed) % $palette.Count]
    $rotation = ($index * 17 + $Seed) % $alphabet.Length
    $rotated = $alphabet.Substring($rotation) + $alphabet.Substring(0, $rotation)
    $suffix = " X${cjk}Y${cjk}"
    $body = "[$index] A${cjk}B $rotated $rotated"
    $targetStringLength = $PayloadWidth - 3 # Three fixed CJK cells each occupy one extra terminal column.
    $prefixLength = $targetStringLength - $suffix.Length
    if ($body.Length -lt $prefixLength) {
        $body = $body.PadRight($prefixLength, [char](97 + (($index + $Seed) % 26)))
    }
    else {
        $body = $body.Substring(0, $prefixLength)
    }
    $body += $suffix

    [Console]::Out.Write("$esc[${color}m$body$esc[0m   `r`n")

    if (($index + 1) % 40 -eq 0) {
        [Console]::Out.Write("$esc[44m    $esc[0m styled-blank-record-$index`r`n")
    }
    if (($index + 1) % 50 -eq 0) {
        # The data row already ended with CRLF; two more create two consecutive blank rows.
        [Console]::Out.Write("`r`n`r`n")
    }
    if (($index + 1) % 100 -eq 0) {
        $uri = "https://example.invalid/harbor-resize/$index"
        [Console]::Out.Write("$esc]8;;$uri$esc\link-$index$esc]8;;$esc\`r`n")
    }
}

[Console]::Out.WriteLine()
[Console]::Out.WriteLine("HARBOR_RESIZE_REFLOW_END lines=$Lines payload_width=$PayloadWidth seed=$Seed")
