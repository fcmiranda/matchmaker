# matchmaker shell integration for nushell
export-env {
    $env.config = ($env.config? | default {} | merge {
        hooks: {
            env_change: {
                PWD: [
                    {|before, after| mm add $after }
                ]
            }
        }
    })
}

def --env z [...rest: string] {
    if ($rest | is-empty) {
        cd ~
    } else if ($rest | length) == 1 and ($rest.0 | path exists) {
        cd $rest.0
    } else {
        let target = (mm list ($rest | str join " ") | lines | first)
        if ($target | is-not-empty) {
            cd $target
        }
    }
}
