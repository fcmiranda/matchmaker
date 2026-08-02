# matchmaker shell integration for zsh
mm_chpwd() {
    mm add "$PWD" >/dev/null 2>&1 &!
}
autoload -U add-zsh-hook
add-zsh-hook chpwd mm_chpwd

z() {
    if [ "$#" -eq 0 ]; then
        local dir
        dir="$(mm -o jump)"
        if [ -n "$dir" ]; then
            cd "$dir" || return
        fi
    elif [ "$#" -eq 1 ] && [ -d "$1" ]; then
        cd "$1" || return
    else
        local dir
        dir="$(mm list "$@" | head -n 1)"
        if [ -n "$dir" ]; then
            cd "$dir" || return
        else
            dir="$(mm -o jump "$@")"
            if [ -n "$dir" ]; then
                cd "$dir" || return
            fi
        fi
    fi
}

zi() {
    local dir
    dir="$(mm -o jump "$@")"
    if [ -n "$dir" ]; then
        cd "$dir" || return
    fi
}
