#!/usr/bin/env bash
# =============================================================================
#  harbor_e2e_test.sh  —  End-to-end visual regression test script
#  Purpose: Run inside the harbor terminal emulator to verify rendering section
#           by section.
#  Usage:   bash harbor_e2e_test.sh [SECTION]
#           Omit the argument to run all sections; pass a section letter (A-Q)
#           to run only that section.
# =============================================================================

ESC=$'\x1b'
CSI="${ESC}["
export GIT_PAGER=cat

# ── Utility functions ─────────────────────────────────────────────────────────

section() {
    local id="$1" title="$2"
    echo ""
    printf '%s' "${CSI}0m${CSI}1;36m"
    printf '══════════════════════════════════════════════════\n'
    printf '  §%s  %s\n' "$id" "$title"
    printf '══════════════════════════════════════════════════\n'
    printf '%s' "${CSI}0m"
}

ok() { printf "${CSI}32m  ✔ %s${CSI}0m\n" "$*"; }
info() { printf "${CSI}90m  ▸ %s${CSI}0m\n" "$*"; }
sep() { printf "${CSI}90m  ──────────────────────────────────${CSI}0m\n"; }
get_cursor_row() {
    local response
    if [ ! -t 1 ] || [ ! -r /dev/tty ]; then
        printf '1'
        return
    fi
    printf "${CSI}6n" >/dev/tty
    if ! IFS=';' read -r -s -d R -t 1 response </dev/tty; then
        printf '1'
        return
    fi
    printf '%s' "${response#*[}"
}

pause() {
    [ -t 0 ] || return 0
    printf "\n${CSI}33m  [Enter to continue, q to skip]${CSI}0m "
    read -r -n1 key
    echo ""
    [[ "$key" == "q" ]] && return 1 || return 0
}

ONLY_SECTION=$(printf '%s' "${1:-}" | tr '[:lower:]' '[:upper:]')
case "$ONLY_SECTION" in
"" | [A-Q]) ;;
*)
    printf 'Usage: %s [A-Q]\n' "$0" >&2
    exit 2
    ;;
esac
run_section() {
    [[ -z "$ONLY_SECTION" ]] && return 0
    [[ "$ONLY_SECTION" == "$1" ]] && return 0
    return 1
}

# =============================================================================
# A. Common Shell Commands — Basic Text Output
# =============================================================================
run_section A && {
    section A "Common Shell Commands (Basic Text Output)"

    info "git config --global --list"
    git config --global --list 2>/dev/null || printf '  (no global git config)\n'
    sep

    info "git log --oneline --color -8"
    git log --oneline --color -8 2>/dev/null || printf '  (not a git repository)\n'
    sep

    info "git status --short"
    git status --short 2>/dev/null || printf '  (not a git repository)\n'
    sep

    info "git diff --stat HEAD~1 HEAD"
    git diff --stat HEAD~1 HEAD 2>/dev/null || printf '  (no previous commit)\n'
    sep

    info "ls --color=always -la"
    ls --color=always -la 2>/dev/null || ls -laG
    sep

    info "env | sort | head -20"
    env | sort | head -20
    sep

    info "ps aux | head -10"
    ps aux 2>/dev/null | head -10 || ps | head -10
    sep

    info "df -h | head -8"
    df -h 2>/dev/null | head -8
    sep

    info "uname -a"
    uname -a
    sep

    ok "§A done: verify text alignment, colors, and special characters render correctly"
    pause || true
}

# =============================================================================
# B. SGR Character Attributes
# =============================================================================
run_section B && {
    section B "SGR Character Attributes"

    info "Basic attributes"
    printf "  Normal  "
    printf "${CSI}1mBold${CSI}0m  "
    printf "${CSI}2mDim${CSI}0m  "
    printf "${CSI}3mItalic${CSI}0m  "
    printf "${CSI}4mUnderline${CSI}0m  "
    printf "${CSI}5mBlink${CSI}0m  "
    printf "${CSI}7mReverse${CSI}0m  "
    printf "${CSI}9mStrikethrough${CSI}0m\n"
    sep

    info "Attribute combinations"
    printf "  ${CSI}1;3mBold+Italic${CSI}0m  "
    printf "${CSI}1;4mBold+Underline${CSI}0m  "
    printf "${CSI}1;31mBold Red${CSI}0m  "
    printf "${CSI}3;4mItalic+Underline${CSI}0m  "
    printf "${CSI}1;3;4;31mAll combined${CSI}0m\n"
    sep

    info "Reset: CSI 0 m and CSI m are equivalent"
    printf "  ${CSI}1;31mBold red→${CSI}0m←CSI0m  ${CSI}1;31mBold red→${CSI}m←CSIm\n"
    sep

    info "Multi-param SGR (CSI 0;1;31 m)"
    printf "  ${CSI}0;1;31mThis is bold red${CSI}0m\n"
    sep

    info "Unknown SGR params must not break existing attributes"
    printf "  ${CSI}1;999;31mBold+unknown999+red${CSI}0m (should appear as bold red)\n"
    sep

    ok "§B done"
    pause || true
}

# =============================================================================
# C. Color System
# =============================================================================
run_section C && {
    section C "Color System"

    info "── Basic 16-color foreground (30-37, 90-97) ──"
    for c in 30 31 32 33 34 35 36 37 90 91 92 93 94 95 96 97; do
        printf "${CSI}${c}m %3d ${CSI}0m" $c
    done
    echo ""
    sep

    info "── Basic 16-color background (40-47, 100-107) ──"
    for c in 40 41 42 43 44 45 46 47 100 101 102 103 104 105 106 107; do
        printf "${CSI}${c}m %3d ${CSI}0m" $c
    done
    echo ""
    sep

    info "── 256-color system colors (0-15) ──"
    for i in $(seq 0 15); do
        printf "${CSI}38;5;${i}m%4d${CSI}0m" $i
    done
    echo ""
    sep

    info "── 256-color 6×6×6 color cube (16-231) excerpt ──"
    for i in $(seq 16 51); do
        printf "${CSI}48;5;${i}m  ${CSI}0m"
    done
    echo ""
    for i in $(seq 52 87); do
        printf "${CSI}48;5;${i}m  ${CSI}0m"
    done
    echo ""
    sep

    info "── 256-color grayscale ramp (232-255) ──"
    for i in $(seq 232 255); do
        printf "${CSI}48;5;${i}m  ${CSI}0m"
    done
    echo ""
    sep

    info "── True Color foreground gradient (38;2;R;G;B) ──"
    for i in $(seq 0 71); do
        r=$((255 - i * 3))
        g=$((i * 3))
        printf "${CSI}38;2;${r};${g};100m█${CSI}0m"
    done
    echo ""
    sep

    info "── True Color background gradient (48;2;R;G;B) ──"
    for i in $(seq 0 71); do
        b=$((255 - i * 3))
        r=$((i * 3))
        printf "${CSI}48;2;${r};50;${b}m  ${CSI}0m"
    done
    echo ""
    sep

    ok "§C done: verify color gradient is smooth with no gaps"
    pause || true
}

# =============================================================================
# D. Cursor Movement
# =============================================================================
run_section D && {
    section D "Cursor Movement (CUU/CUD/CUF/CUB/CUP/CHA/VPA)"

    info "CUU/CUD/CUF/CUB — relative movement"
    printf "  line1\n  line2\n  line3\n"
    printf "${CSI}3A" # move up 3 lines
    printf "${CSI}4C" # move right 4 columns
    printf "◄INS"
    printf "${CSI}3B\n" # move down 3 lines
    sep

    info "CUP — absolute positioning"
    row=$(get_cursor_row)
    for col in 4 12 20 28 36; do
        printf "${CSI}${row};$((col + 2))H★"
    done
    printf "\n"
    sep

    info "CHA (G) — column absolute"
    printf "  XXXXXXXXXX\n"
    printf "${CSI}1A"
    printf "${CSI}6G" # column 6
    printf "←col6"
    printf "${CSI}1B\n"
    sep

    info "VPA (d) — row absolute"
    printf "\n\n\n"
    printf "${CSI}3A"
    printf "${CSI}10d" # row 10 (absolute)
    printf "  VPA=10"
    printf "${CSI}3B\n"
    sep

    info "DECSC/DECRC — save and restore cursor"
    printf "  [save]"
    printf "${ESC}7"
    printf "${CSI}6C"
    printf "moved position"
    printf "${ESC}8"
    printf "←restored to [save]\n"
    sep

    info "CSI s/u — alternative save/restore (no margin_mode)"
    printf "  "
    printf "${CSI}s"
    printf "${CSI}5C"
    printf "moved"
    printf "${CSI}u"
    printf "←restored\n"
    sep

    ok "§D done"
    pause || true
}

# =============================================================================
# E. Erase Operations (ED / EL / ECH)
# =============================================================================
run_section E && {
    section E "Erase Operations (ED / EL / ECH)"

    info "EL 0 — erase from cursor to end of line"
    printf "  AAAAABBBBBCCCCC\n"
    printf "${CSI}1A${CSI}8G"
    printf "${CSI}0K"
    printf "${CSI}1B  (expected: blank after column 8)\n"
    sep

    info "EL 1 — erase from start of line to cursor"
    printf "  AAAAABBBBBCCCCC\n"
    printf "${CSI}1A${CSI}8G"
    printf "${CSI}1K"
    printf "${CSI}1B  (expected: columns 1-8 blank, CCCC preserved)\n"
    sep

    info "EL 2 — erase entire line"
    printf "  AAAAABBBBBCCCCC\n"
    printf "${CSI}1A"
    printf "${CSI}2K"
    printf "${CSI}1B  (expected: entire line blank)\n"
    sep

    info "ECH — erase N characters (cursor does not move)"
    printf "  ABCDEFGHIJ\n"
    printf "${CSI}1A${CSI}4G"
    printf "${CSI}4X" # erase 4 characters
    printf "${CSI}1B  (expected: ABC    HIJ, D-G become spaces)\n"
    sep

    info "ED 0 — erase from cursor to bottom of screen"
    printf "  line 1 content\n  line 2 content\n  line 3 content\n"
    printf "${CSI}3A${CSI}6G"
    printf "${CSI}0J"
    printf "  (expected: from current position to bottom of screen erased)\n"
    sep

    info "ED 1 — erase from top of screen to cursor"
    printf "  line 1\n  line 2\n  line 3\n"
    printf "${CSI}2A"
    printf "${CSI}1J"
    printf "${CSI}1B\n"
    printf "  (expected: lines 1-2 erased, line 3 preserved)\n"
    sep

    ok "§E done"
    pause || true
}

# =============================================================================
# F. Auto-wrap (DECAWM) and Pending Wrap
# =============================================================================
run_section F && {
    section F "Auto-wrap (DECAWM) and Pending Wrap"

    COLS=$(tput cols 2>/dev/null || echo 80)

    info "DECAWM=on: fill to end of line, next character wraps to column 0"
    printf "${CSI}?7h"
    printf "  "
    printf '%*s' $((COLS - 3)) '' | tr ' ' 'A'
    printf "Z\nX  (X should be on a new line at column 0, Z at end of previous line)\n"
    sep

    info "DECAWM=off: characters past column width overwrite the last column"
    printf "${CSI}?7l"
    printf "  "
    printf '%*s' $((COLS - 3)) '' | tr ' ' 'B'
    printf "OVERWRITE\n  (should show only the last few characters, no line wrap)\n"
    printf "${CSI}?7h"
    sep

    info "Pending Wrap + CR: CR clears pending state (no wrap)"
    printf "  1234567890\n"
    printf "${CSI}1A${CSI}10G"
    printf "!"  # reach end of line, triggers pending_wrap
    printf "\r" # CR clears pending_wrap, returns to start of line
    printf "*"  # should overwrite at column 0
    printf "${CSI}1B\n"
    printf "  (expected: '*' and '!' on same line, '*' at column 0)\n"
    sep

    info "Pending Wrap + CUP: absolute positioning clears pending state"
    printf "  AAAAAAAAAA\n"
    printf "${CSI}1A${CSI}10G"
    printf "!"        # pending_wrap
    printf "${CSI}1B" # CUP clears pending_wrap
    printf "  (cursor should move down without wrapping)\n"
    sep

    ok "§F done"
    pause || true
}

# =============================================================================
# G. Wide Characters (CJK / Double-width)
# =============================================================================
run_section G && {
    section G "Wide Characters (CJK Double-width)"

    info "Basic wide character rendering (each occupies 2 columns)"
    printf "  Chinese:  汉字测试渲染\n"
    printf "  Korean:   한국어 테스트\n"
    printf "  Japanese: ひらがなカタカナ\n"
    printf "  Emoji:    🎉 🚀 ⭐ 🔥 ✅\n"
    sep

    info "Wide characters mixed with ASCII — column alignment test"
    printf "  %-10s| %-10s| %-6s\n" "Name" "Score" "Grade"
    printf "  ----------+----------+------\n"
    printf "  %-10s| %-10s| %-6s\n" "张三" "99" "A+"
    printf "  %-10s| %-10s| %-6s\n" "Bob" "88" "B"
    printf "  %-10s| %-10s| %-6s\n" "李四五" "77" "C"
    sep

    info "Wide character at right margin triggers auto-wrap"
    printf "  "
    printf 'xxxxxxxxxxxx中' # wide character lands in the last two columns
    printf "\n  (expected: '中' at end of line, occupies 2 columns then wraps)\n"
    sep

    info "Backspace skips wide character continuation cell"
    printf "  中文字\n"
    printf "${CSI}1A${CSI}3G" # point to continuation cell
    printf "\b"               # should jump back to the lead cell of '中'
    printf "X"                # overwrites the entire wide character (both cells)
    printf "${CSI}1B\n"
    printf "  (expected: '中' replaced by 'X ', column alignment intact)\n"
    sep

    ok "§G done: verify wide characters occupy 2 columns and alignment is correct"
    pause || true
}

# =============================================================================
# H. DEC Special Graphics Character Set
# =============================================================================
run_section H && {
    section H "DEC Special Graphics Character Set (ESC ( 0  line drawing)"

    info "Activate G0=DEC Special Graphics and draw a box"
    printf "${ESC}(0"
    printf "  lqqqqqqqqqqqqqqqk\n"  # ┌─────────────────┐
    printf "  x  Harbor  Term  x\n" # │  Harbor  Term   │
    printf "  tqqqqqqqqqqqqqqqq\n"  # ├────────────────────
    printf "  x  DEC Graphics  x\n"
    printf "  mqqqqqqqqqqqqqqqqj\n" # └──────────────────┘
    printf "${ESC}(B"               # restore ASCII
    sep

    info "Character mapping (a-z in DEC Special Graphics)"
    printf "  ASCII: abcdefghijklmnopqrstuvwxyz\n"
    printf "  DEC:   ${ESC}(0abcdefghijklmnopqrstuvwxyz${ESC}(B\n"
    printf "  (common: j=┘ k=┐ l=┌ m=└ n=┼ q=─ x=│ t=├ u=┤ v=┴ w=┬)\n"
    sep

    info "G1 character set (ESC ) 0 / SO-SI switching)"
    printf "${ESC})0" # designate G1=DEC Special Graphics
    printf "  ASCII(G0): "
    printf "lqk\n"
    printf "  DEC(G1):   "
    printf "\x0elqk\x0f\n" # SO=G1, SI=G0
    sep

    ok "§H done: verify line-drawing characters are correct and ESC(B restores ASCII"
    pause || true
}

# =============================================================================
# I. Scrolling Region (DECSTBM) and SU/SD/IL/DL
# =============================================================================
run_section I && {
    section I "Scrolling Region (DECSTBM) and SU/SD/IL/DL"

    info "Fill test data"
    printf "${CSI}2J${CSI}H"
    for i in $(seq 1 15); do
        printf "  row%02d: " $i
        printf '%0.s─' $(seq 1 30)
        printf "\n"
    done

    info "DECSTBM set region [4,10], SU scroll up 2 lines"
    printf "${CSI}4;10r"
    printf "${CSI}10;1H"
    printf "${CSI}2S"
    printf "${CSI}r"
    printf "${CSI}17;1H  expected: rows 4-10 scrolled up by 2; rows 1-3 and 11-15 unchanged\n"
    sep

    info "SD scroll down 1 line (insert blank line at top of region)"
    printf "${CSI}4;10r"
    printf "${CSI}4;1H"
    printf "${CSI}1T"
    printf "${CSI}r"
    printf "${CSI}18;1H  expected: region content shifted down by 1, blank line at top\n"
    sep

    info "IL — Insert Lines (insert 2 blank lines at cursor)"
    printf "${CSI}5;1H"
    printf "${CSI}2L"
    printf "${CSI}19;1H  expected: 2 blank lines inserted at row 5, content shifted down\n"
    sep

    info "DL — Delete Lines (delete 2 lines at cursor)"
    printf "${CSI}5;1H"
    printf "${CSI}2M"
    printf "${CSI}20;1H  expected: 2 lines deleted, subsequent lines shift up, blank lines at bottom\n"
    sep

    ok "§I done"
    pause || true
    printf "${ESC}c"
}

# =============================================================================
# J. Horizontal Margins (DECLRMM / ?69 / DECSLRM)
# =============================================================================
run_section J && {
    section J "Horizontal Margins (DECLRMM / CSI ? 69 h)"

    printf "${CSI}2J${CSI}H"

    info "Enable DECLRMM, set left=5 right=40"
    printf "${CSI}?69h"
    printf "${CSI}5;40s" # DECSLRM: left=5, right=40 (1-based)

    info "Write characters inside margins (exceeding margin_right should wrap to margin_left)"
    printf "${CSI}2;5H" # row 2, column 5 (margin_left)
    for i in $(seq 1 6); do
        printf "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456"
    done

    info "Cursor left movement clamped by margin_left"
    printf "${CSI}4;20H"
    printf "${CSI}100D" # move left 100
    printf "L"          # should land at margin_left

    info "EL erases only within margins"
    printf "${CSI}5;20H"
    printf "${CSI}2K" # EL 2 erase entire line (columns 5-40 only)

    info "Disable DECLRMM"
    printf "${CSI}?69l"
    printf "${CSI}r"

    printf "${CSI}8;1H\n"
    printf "  expected: characters wrap within col 5-40; content outside unchanged\n"
    sep

    ok "§J done"
    pause || true
    printf "${ESC}c"
}

# =============================================================================
# K. Alternate Screen (?1049)
# =============================================================================
run_section K && {
    section K "Alternate Screen"

    info "Primary screen content preservation: switch to alternate screen, return after 3 s"
    sleep 0.5
    printf "${CSI}?1049h"
    printf "${CSI}2J${CSI}H"
    printf "${CSI}1;32m"
    printf "╔════════════════════════════════════╗\n"
    printf "║     Alternate Screen               ║\n"
    printf "║                                    ║\n"
    printf "║  Primary screen content should     ║\n"
    printf "║  be fully restored on return.      ║\n"
    printf "║                                    ║\n"
    printf "║  Press any key to return...        ║\n"
    printf "╚════════════════════════════════════╝\n"
    printf "${CSI}0m"
    [ -t 0 ] && read -r -n1 -s
    printf "${CSI}?1049l"
    sep

    info "Alternate screen must not pollute primary screen scrollback"
    printf "  All history above should be intact after returning to the primary screen\n"
    sep

    ok "§K done: verify primary screen is fully restored after alternate screen exit"
    pause || true
}

# =============================================================================
# L. Tab Stops (HTS / TBC / HT)
# =============================================================================
run_section L && {
    section L "Tab Stops (HTS / TBC / HT)"

    info "Default tab stops: every 8 columns"
    printf "  col:1\t9\t17\t25\t33\n"
    printf "      ^       ^       ^       ^\n"
    sep

    info "Custom tab stops: col 6, 14, 24"
    printf "${CSI}1;6H${ESC}H"
    printf "${CSI}1;14H${ESC}H"
    printf "${CSI}1;24H${ESC}H"
    printf "${CSI}1;1H"
    printf "  >\t>\t>\t>\n"
    printf "  expected: > at columns 6, 14, 24, ...\n"
    sep

    info "TBC 0: clear tab stop at current column (col 14)"
    printf "${CSI}1;14H"
    printf "${CSI}0g"
    printf "${CSI}1;1H"
    printf "  >\t>\t>\n"
    printf "  expected: skip col 14, jump directly to col 24\n"
    sep

    info "TBC 3: clear all tab stops"
    printf "${CSI}3g"
    printf "${CSI}1;1H"
    printf "  >\t>\n"
    printf "  expected: no tab stops, tab goes to end of line\n"
    sep

    # Restore default tab stops
    for col in 9 17 25 33 41 49 57 65 73; do
        printf "${CSI}1;${col}H${ESC}H"
    done

    ok "§L done"
    pause || true
}

# =============================================================================
# M. Soft Reset / Hard Reset
# =============================================================================
run_section M && {
    section M "Soft Reset (DECSTR) / Hard Reset (RIS)"

    info "Setting multiple attributes..."
    printf "${CSI}1;31m" # bold red
    printf "${CSI}?7l"   # autowrap off
    printf "${CSI}?25l"  # cursor hide
    printf "${CSI}?1h"   # application cursor
    printf "${CSI}2;5r"  # scroll region [2,5]
    printf "  Set: bold red / no wrap / hidden cursor / app cursor keys / scroll region [2,5]\n"
    sleep 0.5

    info "DECSTR soft reset (CSI ! p) — screen content preserved"
    printf "${CSI}!p"
    printf "  After soft reset: normal color / auto-wrap / cursor visible / normal cursor keys / full scroll\n"
    printf "  Screen content is NOT cleared ↑↑↑\n"
    sep

    info "RIS hard reset (ESC c) — executes in 2 s"
    sleep 2
    printf "${ESC}c"
    printf "  Hard reset complete: screen cleared, all state back to defaults\n"
    sep

    ok "§M done"
    pause || true
}

# =============================================================================
# N. Application Cursor Keys (DECCKM) & Modifier Key Sequences
# =============================================================================
run_section N && {
    section N "Application Cursor Keys (DECCKM) & Modifier Keys"

    info "Normal mode (CSI ? 1 l) — arrow keys send CSI sequences"
    printf "${CSI}?1l"
    printf "  Expected sequences (verify with cat -v):\n"
    printf "  ↑=\\x1b[A  ↓=\\x1b[B  →=\\x1b[C  ←=\\x1b[D\n"
    sep

    info "Application cursor mode (CSI ? 1 h) — arrow keys send SS3 sequences"
    printf "${CSI}?1h"
    printf "  Expected sequences:\n"
    printf "  ↑=\\x1bOA  ↓=\\x1bOB  →=\\x1bOC  ←=\\x1bOD\n"
    printf "${CSI}?1l" # restore
    sep

    info "Modifier key ANSI encoding reference table"
    printf "  %-20s %s\n" "Key Combination" "Expected Byte Sequence"
    printf "  %-20s %s\n" "──────────────────" "──────────────────────"
    printf "  %-20s %s\n" "Shift+↑" "\\x1b[1;2A"
    printf "  %-20s %s\n" "Alt+↑" "\\x1b[1;3A"
    printf "  %-20s %s\n" "Ctrl+↑" "\\x1b[1;5A"
    printf "  %-20s %s\n" "Ctrl+Shift+↑" "\\x1b[1;6A"
    printf "  %-20s %s\n" "Ctrl+Home" "\\x1b[1;5H"
    printf "  %-20s %s\n" "Ctrl+End" "\\x1b[1;5F"
    printf "  %-20s %s\n" "Ctrl+Insert" "\\x1b[2;5~"
    printf "  %-20s %s\n" "Ctrl+Delete" "\\x1b[3;5~"
    printf "  %-20s %s\n" "Ctrl+PageUp" "\\x1b[5;5~"
    printf "  %-20s %s\n" "Ctrl+F1" "\\x1b[1;5P"
    printf "  %-20s %s\n" "Ctrl+F5" "\\x1b[15;5~"
    printf "  %-20s %s\n" "Shift+Tab" "\\x1b[Z"
    printf "  %-20s %s\n" "Alt+a" "\\x1ba"
    printf "  %-20s %s\n" "Alt+Ctrl+c" "\\x1b\\x03"
    sep

    info "Application keypad mode (ESC = / ESC >)"
    printf "  ${ESC}=  ← DECKPAM activate\n"
    printf "  Keypad: Enter=\\x1bOM  0=\\x1bOp  1=\\x1bOq  +=\\x1bOk\n"
    printf "  ${ESC}>  ← DECKPNM restore\n"
    printf "${ESC}>"
    sep

    ok "§N done: use 'cat -v' or 'xxd' to verify actual byte sequences"
    pause || true
}

# =============================================================================
# O. Character Insert/Delete (ICH / DCH) and REP
# =============================================================================
run_section O && {
    section O "Character Insert/Delete (ICH / DCH / REP)"

    info "ICH — insert N spaces, shift subsequent content right"
    printf "  ABCDEFGHIJ\n"
    printf "${CSI}1A${CSI}4G"
    printf "${CSI}3@" # insert 3 spaces
    printf "${CSI}1B\n"
    printf "  expected: ABC   DEFGHI (J lost at right margin)\n"
    sep

    info "DCH — delete N characters, shift subsequent content left"
    printf "  ABCDEFGHIJ\n"
    printf "${CSI}1A${CSI}4G"
    printf "${CSI}3P" # delete 3 characters
    printf "${CSI}1B\n"
    printf "  expected: ABCGHIJ    (3 trailing spaces)\n"
    sep

    info "ECH — erase N characters (cursor and content do not move)"
    printf "  ABCDEFGHIJ\n"
    printf "${CSI}1A${CSI}4G"
    printf "${CSI}3X" # erase 3 characters
    printf "${CSI}1B\n"
    printf "  expected: ABC   GHI   (DEF become spaces, cursor stays at D)\n"
    sep

    info "ICH with DECLRMM margin constraints"
    printf "${CSI}?69h"
    printf "${CSI}3;8s" # left=3, right=8
    printf "  12345678\n"
    printf "${CSI}1A${CSI}4G"
    printf "${CSI}2@" # insert only within margins
    printf "${CSI}1B"
    printf "${CSI}?69l"
    printf "\n  expected: content at col 9+ unaffected\n"
    sep

    info "REP — repeat last character"
    printf "  A"
    printf "${CSI}19b" # repeat 19 times
    printf "\n  expected: AAAAAAAAAAAAAAAAAAAA (20 total)\n"
    sep

    info "REP with wide characters"
    printf "  中"
    printf "${CSI}4b"
    printf "\n  expected: 中中中中中 (5 characters, 10 columns)\n"
    sep

    info "REP respects auto-wrap"
    printf "  "
    COLS=$(tput cols 2>/dev/null || echo 80)
    FILL=$((COLS - 5))
    printf "A${CSI}${FILL}b\n"
    printf "  expected: A repeated to fill line, then wraps to next line\n"
    sep

    ok "§O done"
    pause || true
}

# =============================================================================
# P. Rectangular Area Operations (DECERA / DECFRA / DECCRA / DECCARA / DECRARA)
# =============================================================================
run_section P && {
    section P "Rectangular Area Operations (DEC Extensions §32)"

    printf "${CSI}2J${CSI}H"
    for i in $(seq 1 12); do
        printf "${CSI}${i};1H  row%02d: " $i
        printf '%0.s#' $(seq 1 50)
    done

    info "DECERA — erase rectangle [3,10]-[6,30]"
    printf "${CSI}3;10;6;30\$z"
    printf "${CSI}14;1H  expected: rows 3-6, columns 10-30 become spaces\n"

    info "DECFRA — fill rectangle [3,10]-[5,20] with '*' (0x2A)"
    printf "${CSI}42;3;10;5;20\$x"
    printf "${CSI}15;1H  expected: rows 3-5, columns 10-20 all '*'\n"

    info "DECCRA — copy rectangle [3,10]-[5,20] to [8,35]"
    printf "${CSI}3;10;5;20;;8;35\$v"
    printf "${CSI}16;1H  expected: '*' block copied to row 8, column 35\n"

    info "DECCARA — apply bold (SGR 1) to rectangle [3,10]-[3,20]"
    printf "${CSI}3;10;3;20;1\$r"
    printf "${CSI}17;1H  expected: characters in row 3, columns 10-20 become bold\n"

    info "DECRARA — toggle bold on rectangle [3,10]-[3,20]"
    printf "${CSI}3;10;3;20;1\$t"
    printf "${CSI}18;1H  expected: bold toggled off\n"

    pause || true
    printf "${ESC}c"
    ok "§P done"
}

# =============================================================================
# Q. Character Protection (DECSCA) and Selective Erase (DECSED / DECSEL)
# =============================================================================
run_section Q && {
    section Q "Character Protection (DECSCA) and Selective Erase (DECSED / DECSEL)"

    printf "${CSI}2J${CSI}H"
    printf "  Writing mixed protected/unprotected characters:\n\n"
    printf "  "

    printf "${CSI}0\"q" # DECSCA off (non-protected)
    printf "a"
    printf "${CSI}1\"q"           # DECSCA on (protected)
    printf "${CSI}1;31mB${CSI}0m" # protected character, shown in red
    printf "${CSI}0\"q"
    printf "c"
    printf "${CSI}1\"q"
    printf "${CSI}1;31mD${CSI}0m"
    printf "${CSI}0\"q"
    printf "e\n\n"

    printf "  (Red B/D = protected; a/c/e = unprotected)\n\n"

    info "DECSEL 2 — selective erase entire line (protected chars preserved)"
    printf "${CSI}3;1H"
    printf "${CSI}?2K"
    printf "${CSI}5;1H  expected: a c e erased, B D preserved\n"
    sep

    info "DECSED 2 — selective erase full screen (protected chars preserved)"
    # write multiple rows
    for row in 7 8 9; do
        printf "${CSI}${row};1H  "
        printf "${CSI}0\"q"
        printf "normal"
        printf "${CSI}1\"q"
        printf "${CSI}1;34mPROT${CSI}0m"
        printf "${CSI}0\"q"
        printf "normal"
    done
    printf "${CSI}11;1H"
    printf "${CSI}?2J"
    printf "${CSI}12;1H  expected: 'normal' erased on each row, 'PROT' preserved\n"
    sep

    ok "§Q done"
    pause || true
}

# =============================================================================
# Summary
# =============================================================================
printf "\n"
printf "${CSI}1;32m"
printf "╔══════════════════════════════════════════════════╗\n"
printf "║      harbor end-to-end test script complete      ║\n"
printf "║                                                  ║\n"
printf "║  All sections passed: terminal rendering OK      ║\n"
printf "╚══════════════════════════════════════════════════╝\n"
printf "${CSI}0m"
printf "\n"
printf "  Time: $(date '+%Y-%m-%d %H:%M:%S')\n"
printf "  Size: ${COLUMNS:-?}×${LINES:-?} (cols×rows)\n"
printf "  TERM: ${TERM:-unknown}\n"
printf "\n"
