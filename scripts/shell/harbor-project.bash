# Opt-in only: source /path/harbor-project.bash wsl Ubuntu
# or on a direct SSH bash: source /path/harbor-project.bash ssh user@host
# Requires bash, od, tr. No history parsing, eval, file writes or installation.
# Refuse competing preexec/prompt hooks rather than silently replacing them.
if [[ $- != *i* || -n $(trap -p DEBUG) || -n ${PROMPT_COMMAND[*]} || $- == *T* ]]; then
    printf '%s\n' 'Harbor integration needs interactive bash without DEBUG/PROMPT_COMMAND/functrace hooks; use a disposable clean shell.' >&2
    return 1
fi
if [[ ${1-} != wsl && ${1-} != ssh ]] || [[ ! ${2-} =~ ^[a-zA-Z0-9][a-zA-Z0-9._@-]*$ ]]; then
    printf '%s\n' 'Usage: source harbor-project.bash {wsl|ssh} {distribution|user@host-or-alias}' >&2
    return 1
fi
for __harbor_name in pi codex claude opencode; do
    if declare -F "$__harbor_name" >/dev/null || alias "$__harbor_name" &>/dev/null; then
        printf '%s\n' 'Harbor refuses to replace existing agent aliases/functions.' >&2
        unset __harbor_name
        return 1
    fi
done
__harbor_kind=$1
__harbor_target=$2
__harbor_hex() { printf '%s' "$1" | od -An -v -tx1 | tr -d ' \n'; }
__harbor_busy() { printf '\033]777;harbor;1;busy\007'; }
__harbor_emit() {
    local action=$1 part payload
    shift
    payload="harbor;1;$action;$(__harbor_hex "$__harbor_kind");$(__harbor_hex "$__harbor_target");$(__harbor_hex "$PWD")"
    for part in "$@"; do payload+=";$(__harbor_hex "$part")"; done
    # Parser OSC budget is 4096 bytes. Over-budget recognition remains unknown.
    if (( ${#payload} <= 4000 )); then printf '\033]777;%s\007' "$payload"; else __harbor_busy; fi
}
__harbor_prompt() { __harbor_emit context; }
__harbor_agent() {
    local agent=$1 status
    shift
    if (( ${#FUNCNAME[@]} == 2 )); then __harbor_emit agent "$agent" "$@"; else __harbor_emit record "$agent" "$@"; fi
    command "$agent" "$@"
    status=$?
    # Only the prompt revalidates context after exit; compound commands remain unknown.
    __harbor_busy
    return "$status"
}
pi() { __harbor_agent pi "$@"; }
codex() { __harbor_agent codex "$@"; }
claude() { __harbor_agent claude "$@"; }
opencode() { __harbor_agent opencode "$@"; }
PROMPT_COMMAND=__harbor_prompt
trap '__harbor_busy' DEBUG
unset __harbor_name
