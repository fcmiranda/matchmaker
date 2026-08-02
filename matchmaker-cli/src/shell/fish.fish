# matchmaker shell integration for fish
function __mm_pwd_change --on-variable PWD
    mm add "$PWD" >/dev/null 2>&1 &
end

function z
    if test (count $argv) -eq 0
        cd ~
    else if test (count $argv) -eq 1 -a -d "$argv[1]"
        cd "$argv[1]"
    else
        set -l target (mm list $argv | head -n 1)
        if test -n "$target"
            cd "$target"
        else
            echo "mm: no matching directory found" >&2
            return 1
        end
    end
end

function zi
    set -l target (mm list | mm --frecency)
    if test -n "$target"
        cd "$target"
    end
end
