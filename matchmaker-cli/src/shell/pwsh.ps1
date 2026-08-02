# matchmaker shell integration for powershell
function Set-Location-With-Mm {
    param($Path)
    Set-Location $Path
    mm add (Get-Location).Path | Out-Null
}

function z {
    param([string]$Path)
    if ([string]::IsNullOrEmpty($Path)) {
        Set-Location ~
    } elseif (Test-Path -Path $Path -PathType Container) {
        Set-Location $Path
    } else {
        $target = (mm list $Path | Select-Object -First 1)
        if ($target) {
            Set-Location $target
        }
    }
}
