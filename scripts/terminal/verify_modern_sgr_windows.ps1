<#
.SYNOPSIS
    T0003 integrated synthetic VT fixture; run only in a dedicated Harbor/ConPTY.
.DESCRIPTION
    Conceal is presentation, not redaction. All hidden values are public fixtures.
    ConPTY live-primary styled overflow is clipped under ADR 0052, not lossless.
    Core tests cover replies/fragmentation/reset; this fixture is visual evidence.
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$e = [char]27
$cjk = [char]0x754C
$mark = [char]0x0301
[Console]::Out.Write("$e[0m$e[2J$e[H")
[Console]::Out.WriteLine('T0003 integrated synthetic modern SGR')
[Console]::Out.WriteLine("OS=$([Environment]::OSVersion.VersionString) PS=$($PSVersionTable.PSVersion) TERM=$env:TERM")
[Console]::Out.WriteLine("Revision=$(git rev-parse HEAD); binary dirty scope must be recorded separately")
$names = @('single', 'double', 'curly', 'dotted', 'dashed')
for ($style = 1; $style -le 5; $style++) {
    [Console]::Out.WriteLine("$($names[$style-1]): $e[4:$style;58:2::255:80:80;53mAA    $cjk    BB$e[0m")
}
[Console]::Out.WriteLine("INVERSE: $e[31;44;7;4:3;53mdefault $e[58;2;255;80;80mexplicit $e[59mdefault$e[0m")
[Console]::Out.WriteLine("COLOR/OFF: $e[58;5;10;4:2mgreen$e[24m OFF $e[4:2magain$e[59m foreground$e[0m")
[Console]::Out.WriteLine("HIDDEN: <$e[4:3;58;2;255;80;80;8;53;9;7;31;44mPUBLIC-e$mark-$cjk$e[0m> REVEALED")
[Console]::Out.WriteLine("LINKHIDE: <$e]8;;https://example.test$e\$e[8;4:5;58;5;10;53mPUBLIC$e[28;24;55m> fallback   $cjk$e]8;;$e\$e[0m")
[Console]::Out.WriteLine("TAIL: AB$e[4:3;58;5;10;53m" + (' ' * 65) + "$e[0m")
[Console]::Out.WriteLine("$e[4:4;58;5;10;53m$e[K$e[0m")
[Console]::Out.WriteLine('END T0003 - inspect copy, narrow/widen, then ordinary shell output.')
