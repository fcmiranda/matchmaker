# matchmaker shell integration for zsh
mm_chpwd() {
    mm add "$PWD" >/dev/null 2>&1 &!
}
autoload -U add-zsh-hook
add-zsh-hook chpwd mm_chpwd

z() {
    if [ "$#" -eq 0 ]; then
        cd ~ || return
    elif [ "$#" -eq 1 ] && [ -d "$1" ]; then
        cd "$1" || return
    else
        local dir
        dir="$(mm list "$@" | head -n 1)"
        if [ -n "$dir" ]; then
            cd "$dir" || return
        else
            echo "mm: no matching directory found" >&2
            return 1
        fi
    fi
}

zi() {
    local dir
    dir="$(mm list | mm --frecency)"
    if [ -n "$dir" ]; then
        cd "$dir" || return
    fi
}
