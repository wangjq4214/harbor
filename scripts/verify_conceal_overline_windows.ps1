<#
.SYNOPSIS
    T0002 synthetic conceal/overline fixture for a newly built Harbor/ConPTY.
.DESCRIPTION
    Conceal is presentation, NOT redaction. Copy the HIDDEN row, including its
    invisible original text. Narrow/widen and inspect the overlined blank tail.
    This is not Neovim, Su discovery, or full modern-style/color acceptance.
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$e = [char]27
$cjk = [char]0x754C
$mark = [char]0x0301
[Console]::Out.Write("$e[0m$e[2J$e[H")
[Console]::Out.WriteLine('T0002 conceal / overline - synthetic Harbor Windows/ConPTY')
[Console]::Out.WriteLine("OS=$([Environment]::OSVersion.VersionString) PowerShell=$($PSVersionTable.PSVersion)")
[Console]::Out.WriteLine("Revision=$(git rev-parse HEAD) - see evidence for dirty scope")
[Console]::Out.WriteLine("OVERLINE: $e[53;32mA    $cjk    B$e[0m OFF")
[Console]::Out.WriteLine("COMBINED: $e[53;4;9;31mA    $cjk    B$e[0m OFF")
[Console]::Out.WriteLine("INVERSE:  $e[53;31;44;7mA    $cjk    B$e[0m OFF")
[Console]::Out.WriteLine("HIDDEN: <$e[8;53;4;9;44mSECRET-e$mark-$cjk$e[28;55;24;29;49m> REVEALED")
[Console]::Out.WriteLine("INVHIDE: <$e[8;53;4;9;7;31;44mSECRET-e$mark-$cjk$e[0m> NORMAL")
[Console]::Out.WriteLine("LINKHIDE: <$e]8;;https://example.test$e\$e[8mSECRET$e[28m>$e]8;;$e\ NORMAL")
[Console]::Out.WriteLine('TAIL below: AB followed by 65 visibly overlined blanks; preserve through resize.')
[Console]::Out.WriteLine("AB$e[53;32m" + (' ' * 65) + "$e[0m")
[Console]::Out.WriteLine('ERASE below: overline via styled erase to row end.')
[Console]::Out.WriteLine("$e[53;33m$e[K$e[0m")
[Console]::Out.WriteLine('END T0002 - ordinary shell prompt/input below; conceal does not redact copied text.')
