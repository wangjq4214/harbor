@echo off
chcp 65001 >nul
setlocal EnableExtensions DisableDelayedExpansion
for /F "delims=" %%E in ('echo prompt $E^| cmd') do set "ESC=%%E"
set "CSI=%ESC%["
set "GIT_PAGER=cat"
set "ONLY=%~1"
if defined ONLY set "ONLY=%ONLY:a=A%"
if defined ONLY set "ONLY=%ONLY:b=B%"
if defined ONLY set "ONLY=%ONLY:c=C%"
if defined ONLY set "ONLY=%ONLY:d=D%"
if defined ONLY set "ONLY=%ONLY:e=E%"
if defined ONLY set "ONLY=%ONLY:f=F%"
if defined ONLY set "ONLY=%ONLY:g=G%"
if defined ONLY set "ONLY=%ONLY:h=H%"
if defined ONLY set "ONLY=%ONLY:i=I%"
if defined ONLY set "ONLY=%ONLY:j=J%"
if defined ONLY set "ONLY=%ONLY:k=K%"
if defined ONLY set "ONLY=%ONLY:l=L%"
if defined ONLY set "ONLY=%ONLY:m=M%"
if defined ONLY set "ONLY=%ONLY:n=N%"
if defined ONLY set "ONLY=%ONLY:o=O%"
if defined ONLY set "ONLY=%ONLY:p=P%"
if defined ONLY set "ONLY=%ONLY:q=Q%"
set "VALID_SECTION="
for %%S in (A B C D E F G H I J K L M N O P Q) do if /I "%ONLY%"=="%%S" set "VALID_SECTION=1"
if defined ONLY if not defined VALID_SECTION echo Usage: %~nx0 [A-Q]
if defined ONLY if not defined VALID_SECTION exit /b 2

call :run A && call :A
call :run B && call :B
call :run C && call :C
call :run D && call :D
call :run E && call :E
call :run F && call :F
call :run G && call :G
call :run H && call :H
call :run I && call :I
call :run J && call :J
call :run K && call :K
call :run L && call :L
call :run M && call :M
call :run N && call :N
call :run O && call :O
call :run P && call :P
call :run Q && call :Q

echo.
echo %CSI%1;32m
echo ^|  harbor End-to-End Test Complete  ^|
echo ^| All selected sections have run; verify the output. ^|
echo  %CSI%0m
echo  Time: %date% %time:~0,8%
echo  Size: %COLUMNS%x%LINES% ^(cols x rows^)
if defined TERM (echo  TERM: %TERM%) else echo  TERM: unknown
exit /b 0

:run
if not defined ONLY exit /b 0
if /I "%ONLY%"=="%~1" exit /b 0
exit /b 1

:head
echo.
echo %CSI%0m%CSI%1;36m
echo  %~1  %~2
echo  %CSI%0m
exit /b
:info
echo %CSI%90m  %~1%CSI%0m
exit /b
:sep
echo %CSI%90m  %CSI%0m
exit /b
:ok
echo %CSI%32m  %~1%CSI%0m
exit /b
:wait
powershell -NoProfile -Command "if ([Console]::IsInputRedirected) { exit 0 } else { exit 1 }" >nul 2>&1
if not errorlevel 1 exit /b 0
echo.
pause >nul
exit /b 0

:A
call :head A "Common Windows Commands (Basic Text Output)"
call :info "git config --global --list"
git config --global --list 2>nul || echo  ^(  git  ^)
call :sep
call :info "git log --oneline --color -8"
git log --oneline --color -8 2>nul || echo  ^(  git  ^)
call :sep
call :info "git status --short"
git status --short 2>nul || echo  ^(  git  ^)
call :sep
call :info "git diff --stat HEAD~1 HEAD"
git diff --stat HEAD~1 HEAD 2>nul || echo  ^(  commit^)
call :sep
call :info "dir /a"
dir /a
call :sep
call :info "set (environment variables)"
set
call :sep
call :info "tasklist"
tasklist
call :sep
call :info "wmic logicaldisk (drives)"
wmic logicaldisk get Caption,FreeSpace,Size 2>nul || echo  ^(wmic  ^)
call :sep
call :info "ver"
ver
call :sep
call :ok "Section A complete: verify alignment, colors, and special characters"
call :wait
exit /b

:B
call :head B "SGR Character Attributes"
call :info "Terminal attribute rendering"
echo  Normal  %CSI%1mBold%CSI%0m  %CSI%2mDim%CSI%0m  %CSI%3mItalic%CSI%0m  %CSI%4mUnderline%CSI%0m  %CSI%5mBlink%CSI%0m  %CSI%7mReverse%CSI%0m  %CSI%9mStrikethrough%CSI%0m
call :sep
call :info "Attribute combinations"
echo  %CSI%1;3mBold Italic%CSI%0m  %CSI%1;4mBold Underline%CSI%0m  %CSI%1;31mBold Red%CSI%0m  %CSI%3;4mItalic Underline%CSI%0m  %CSI%1;3;4;31mAll Combined%CSI%0m
call :info "Reset and unknown SGR codes"
echo  %CSI%0;1;31mBold Red (0;1;31)%CSI%0m  %CSI%1;999;31mBold Red with unknown 999%CSI%0m
call :sep
call :ok "Section B complete"
call :wait
exit /b

:C
call :head C "Color System"
call :info "Basic 16-color palette"
for %%C in (30 31 32 33 34 35 36 37 90 91 92 93 94 95 96 97) do <nul set /p "=%CSI%%%Cm %%C %CSI%0m"
echo.
call :info "Basic 16-color palette"
for %%C in (40 41 42 43 44 45 46 47 100 101 102 103 104 105 106 107) do <nul set /p "=%CSI%%%Cm %%C %CSI%0m"
echo.
call :info "256-color system colors (0-15)"
for /L %%I in (0,1,15) do <nul set /p "=%CSI%38;5;%%Im %%I %CSI%0m"
echo.
call :info "256-color 6x6x6 color cube slice (16-87)"
for /L %%I in (16,1,87) do <nul set /p "=%CSI%48;5;%%Im  %CSI%0m"
echo.
call :info "256-color grayscale (232-255)"
for /L %%I in (232,1,255) do <nul set /p "=%CSI%48;5;%%Im  %CSI%0m"
echo.
call :info "True Color foreground and background gradients"
setlocal EnableDelayedExpansion
for /L %%I in (0,1,71) do (set /a R=255-%%I*3,G=%%I*3& <nul set /p "=!CSI!38;2;!R!;!G!;100m !CSI!0m")
echo.
for /L %%I in (0,1,71) do (set /a R=%%I*3,B=255-%%I*3& <nul set /p "=!CSI!48;2;!R!;50;!B!m  !CSI!0m")
endlocal
echo.
call :sep
call :ok "Section C complete"
call :wait
exit /b

:D
call :head D "Cursor Movement (CUU/CUD/CUF/CUB/CUP/CHA/VPA)"
call :info "CUU/CUD/CUF/CUB - relative movement"
echo  1& echo  2& echo  3
echo %CSI%3A%CSI%4C INS%CSI%3B
call :info "CHA / VPA / cursor save and restore"
echo  XXXXXXXXXX
echo %CSI%1A%CSI%6G col6%CSI%1B
echo  [  ]%ESC%7%CSI%6C  %ESC%8
echo  %CSI%s%CSI%5C  %CSI%u
call :sep
call :ok "Section D complete"
call :wait
exit /b

:E
call :head E "Erase Operations (ED / EL / ECH)"
call :info "EL 0 / EL 1 / EL 2"
echo  AAAAABBBBBCCCCC
echo %CSI%1A%CSI%8G%CSI%0K%CSI%1B  ^(Expected: blank after column 8^)
echo  AAAAABBBBBCCCCC
echo %CSI%1A%CSI%8G%CSI%1K%CSI%1B  ^(Expected: columns 1-8 blank^)
echo  AAAAABBBBBCCCCC
echo %CSI%1A%CSI%2K%CSI%1B  ^(Expected: entire line blank^)
call :info "ECH - erase 4 characters"
echo  ABCDEFGHIJ
echo %CSI%1A%CSI%4G%CSI%4X%CSI%1B  ^(Expected: ABC  HIJ^)
call :sep
call :ok "Section E complete"
call :wait
exit /b

:F
call :head F "Automatic Wrap (DECAWM) and Pending Wrap"
call :info "DECAWM=on / off"
echo %CSI%?7h  AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAZ
echo X  ^(X should be on a new line^)
echo %CSI%?7l  BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBOVERWRITE
echo %CSI%?7h  ^(Expected: overwrite the last column without wrapping^)
call :info "Pending Wrap + CR / CUP"
echo  1234567890
echo %CSI%1A%CSI%10G!%CSI%1G*%CSI%1B
echo  AAAAAAAAAA
echo %CSI%1A%CSI%10G!%CSI%1B  ^(Positioning clears the pending-wrap state^)
call :sep
call :ok "Section F complete"
call :wait
exit /b

:G
call :head G "Wide Characters (CJK Double-Width Characters)"
call :info "Basic wide-character rendering"
echo  Chinese: 汉字测试渲染
echo  Korean: 안녕하세요
echo  Japanese: ひらがなカタカナ
echo  Emoji: 🌈 🚀 ⭐ 🔥 ✓
call :info "Mixed wide-character and ASCII alignment"
echo  Name  ^| Score  ^| Grade
echo  张三  ^| 99  ^| A+
echo  Bob  ^| 88  ^| B
echo  李四五  ^| 77  ^| C
call :info "Wide-character continuation-cell fallback"
echo  中文字
echo %CSI%1A%CSI%3G%CSI%1DX%CSI%1B  ^(Expected: replace the entire wide character without breaking alignment^)
call :sep
call :ok "Section G complete"
call :wait
exit /b

:H
call :head H "DEC Special Graphics Character Set"
call :info "G0=DEC Special Graphics"
echo %ESC%^(0  lqqqqqqqqqqqqqqqk
echo  x  Harbor Term  x
echo  tqqqqqqqqqqqqqqqu
echo  x  DEC Graphics  x
echo  mqqqqqqqqqqqqqqqj%ESC%^(B
call :info "Character mapping a-z"
echo  ASCII: abcdefghijklmnopqrstuvwxyz
echo  DEC:  %ESC%^(0abcdefghijklmnopqrstuvwxyz%ESC%^(B
call :sep
call :ok "Section H complete"
call :wait
exit /b

:I
call :head I "Scrolling Region (DECSTBM) and SU/SD/IL/DL"
echo %CSI%2J%CSI%H
for /L %%I in (1,1,15) do echo  %%I:
echo %CSI%4;10r%CSI%10;1H%CSI%2S%CSI%r%CSI%17;1H  Expected: lines 4-10 scroll up 2 lines
echo %CSI%4;10r%CSI%4;1H%CSI%1T%CSI%r%CSI%18;1H  Expected: region scrolls down 1 line
echo %CSI%5;1H%CSI%2L%CSI%19;1H  Insert 2 lines
echo %CSI%5;1H%CSI%2M%CSI%20;1H  Delete 2 lines
call :wait
echo %ESC%c
exit /b

:J
call :head J "Horizontal Margins (DECLRMM / DECSLRM)"
echo %CSI%2J%CSI%H%CSI%?69h%CSI%5;40s%CSI%2;5HABCDEFGHIJKLMNOPQRSTUVWXYZ0123456ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456
echo %CSI%4;20H%CSI%100DL%CSI%5;20H%CSI%2K%CSI%?69l%CSI%r%CSI%8;1H
echo  Expected: text wraps within columns 5-40
call :wait
echo %ESC%c
exit /b

:K
call :head K "Alternate Screen"
call :info "Switching to the alternate screen; press any key to return"
>nul ping 127.0.0.1 -n 2
echo %CSI%?1049h%CSI%2J%CSI%H%CSI%1;32m
echo =====================================
echo  Alternate Screen
echo  The primary screen should be fully restored.
echo  Press any key to return to the primary screen...
echo =====================================%CSI%0m
call :wait
echo %CSI%?1049l
call :ok "Section K complete"
call :wait
exit /b

:L
call :head L "Tab Stops (HTS / TBC / HT)"
call :info "Default tab stops"
echo  col:1	9	17	25	33
call :info "Custom tab stops at columns 6, 14, and 24; TBC 0 / 3"
echo %CSI%1;6H%ESC%H%CSI%1;14H%ESC%H%CSI%1;24H%ESC%H%CSI%1;1H  ^>	^>	^>	^>
echo %CSI%1;14H%CSI%0g%CSI%1;1H  ^>	^>	^>%CSI%3g
call :sep
call :ok "Section L complete"
call :wait
exit /b

:M
call :head M "Soft Reset (DECSTR) / Hard Reset (RIS)"
echo %CSI%1;31m%CSI%?7l%CSI%?25l%CSI%?1h%CSI%2;5r  Set: bold red / no wrap / hidden cursor / application cursor keys / scrolling region
echo %CSI%!p  Soft reset restores defaults without clearing the screen
>nul ping 127.0.0.1 -n 3
echo %ESC%c
echo  Hard reset complete
call :ok "Section M complete"
call :wait
exit /b

:N
call :head N "Application Cursor Keys (DECCKM) and Modifier Keys"
echo %CSI%?1l  Normal: \x1b[A \x1b[B \x1b[C \x1b[D
echo %CSI%?1h  Application: \x1bOA \x1bOB \x1bOC \x1bOD%CSI%?1l
echo  Shift+  \x1b[1;2A
echo  Alt+  \x1b[1;3A
echo  Ctrl+  \x1b[1;5A
echo  Ctrl+Home  \x1b[1;5H
echo  Ctrl+Delete  \x1b[3;5~
echo  Shift+Tab  \x1b[Z
echo  %ESC%=  DECKPAM  Enter=\x1bOM  0=\x1bOp  1=\x1bOq
echo  %ESC%^>  DECKPNM
call :sep
call :ok "Section N complete"
call :wait
exit /b

:O
call :head O "Insert/Delete Characters (ICH / DCH / ECH / REP)"
echo  ABCDEFGHIJ
echo %CSI%1A%CSI%4G%CSI%3@%CSI%1B  ^(ICH expected: ABC  DEFGHI^)
echo  ABCDEFGHIJ
echo %CSI%1A%CSI%4G%CSI%3P%CSI%1B  ^(DCH expected: ABCGHIJ^)
echo  ABCDEFGHIJ
echo %CSI%1A%CSI%4G%CSI%3X%CSI%1B  ^(ECH expected: ABC  GHIJ^)
echo  A%CSI%19b
echo  中%CSI%4b
call :sep
call :ok "Section O complete"
call :wait
exit /b

:P
call :head P "Rectangular Area Operations (DEC Extension 32)"
echo %CSI%2J%CSI%H
for /L %%I in (1,1,12) do echo %CSI%%%I;1H  %%I: ##################################################
echo %CSI%3;10;6;30$z%CSI%14;1H  DECERA
echo %CSI%42;3;10;5;20$x%CSI%15;1H  DECFRA
echo %CSI%3;10;5;20;;8;35$v%CSI%16;1H  DECCRA
echo %CSI%3;10;3;20;1$r%CSI%3;10;3;20;1$t%CSI%18;1H  DECCARA / DECRARA
call :wait
echo %ESC%c
call :ok "Section P complete"
exit /b

:Q
call :head Q "Character Protection (DECSCA) and Selective Erase"
echo %CSI%2J%CSI%H
echo  %CSI%0"qa%CSI%1"q%CSI%1;31mB%CSI%0m%CSI%0"qc%CSI%1"q%CSI%1;31mD%CSI%0m%CSI%0"qe
echo %CSI%1;1H%CSI%?2K%CSI%5;1H  Expected: a/c/e cleared; B/D remain
echo %CSI%7;1H%CSI%0"qnormal%CSI%1"q%CSI%1;34mPROT%CSI%0m%CSI%0"qnormal
echo %CSI%8;1H%CSI%0"qnormal%CSI%1"q%CSI%1;34mPROT%CSI%0m%CSI%0"qnormal
echo %CSI%11;1H%CSI%?2J%CSI%12;1H  Expected: normal cleared; PROT remains
call :sep
call :ok "Section Q complete"
call :wait
exit /b
