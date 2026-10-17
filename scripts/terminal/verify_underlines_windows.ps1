<#
.SYNOPSIS
    T0001 synthetic underline fixtures in an actual Harbor Windows/ConPTY session.
.DESCRIPTION
    Run from the repository root inside the newly built Harbor. Observe the five
    styles, spaces and wide text, inverse/color independence, OSC 8 fallback,
    and decorated tails while narrowing/widening the window. This is not Neovim,
    Su discovery, conceal/overline, or broader transport acceptance.
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$e = [char]27
$cjk = [char]0x754C
[Console]::Out.Write("$e[0m$e[2J$e[H")
[Console]::Out.WriteLine("T0001 synthetic Windows underlines")
[Console]::Out.WriteLine("OS=$([Environment]::OSVersion.VersionString) PowerShell=$($PSVersionTable.PSVersion)")
[Console]::Out.WriteLine("TERM=$env:TERM TERM_PROGRAM=$env:TERM_PROGRAM")
[Console]::Out.WriteLine("Revision=$(git rev-parse HEAD) (see git status for dirty scope)")
[Console]::Out.WriteLine("Expect distinct red styles, including every blank and both halves of wide text:")
$names = @('single', 'double', 'curly', 'dotted', 'dashed')
for ($style = 1; $style -le 5; $style++) {
    [Console]::Out.WriteLine(("{0,-8} " -f $names[$style - 1]) + "$e[4:$($style);58:2::255:80:80mAA    $cjk    BB$e[0m")
}
[Console]::Out.WriteLine("Inverse: default follows blue background; explicit stays red:")
[Console]::Out.WriteLine("$e[31;44;7;4:3mdefault    $cjk$e[58;2;255;80;80m explicit    $cjk$e[59m default$e[0m")
[Console]::Out.WriteLine("24 keeps color; re-enable is green; 59 follows text again:")
[Console]::Out.WriteLine("$e[58;5;10;4:2mgreen$e[24m OFF $e[4:2magain$e[59m foreground$e[0m")
[Console]::Out.WriteLine("OSC8: linked text/wide single, spaces off; explicit curly covers spaces:")
[Console]::Out.WriteLine("$e]8;;https://example.test$e\link   $cjk $e[4:3mcurly   $cjk$e[24m fallback$e]8;;$e\$e[0m")
[Console]::Out.WriteLine("Resize tail fixture below: trailing red decoration must survive narrow/wide.")
[Console]::Out.WriteLine("AB$e[4:3;58;2;255;80;80m" + (' ' * 65) + "$e[0m")
[Console]::Out.WriteLine("Styled erase fixture: green dotted blanks to end of this row.")
[Console]::Out.WriteLine("$e[4:4;58;5;10m$e[K$e[0m")
[Console]::Out.WriteLine("END T0001 - shell input and prompt below must be ordinary.")
