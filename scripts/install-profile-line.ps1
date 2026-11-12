# Adds one dot-source line to your PowerShell profile so the `bridge` command works in every new window.
# Safe to run twice: if the line is already there it changes nothing. Backs up the profile before editing it.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\install-profile-line.ps1 -Line '. "$env:APPDATA\Bridge\bridge-functions.ps1"'
param(
    [Parameter(Mandatory = $true)]
    [string]$Line
)

$ErrorActionPreference = 'Stop'
$profilePath = $PROFILE.CurrentUserCurrentHost

if (Test-Path -LiteralPath $profilePath) {
    $existing = @(Get-Content -LiteralPath $profilePath)
    if ($existing | Where-Object { $_.Trim() -eq $Line.Trim() }) {
        Write-Host "Already set up: your profile already has this line:"
        Write-Host "  $Line"
        Write-Host "Profile: $profilePath"
        exit 0
    }
    $backup = "$profilePath.bak-$(Get-Date -Format 'yyyy-MM-dd-HHmmss')"
    Copy-Item -LiteralPath $profilePath -Destination $backup
    Write-Host "Backed up your profile to: $backup"
    # Append in the file's own encoding (profiles can be UTF-16 or UTF-8 with/without BOM).
    $bytes = [System.IO.File]::ReadAllBytes($profilePath)
    $encoding = New-Object System.Text.UTF8Encoding($false)
    if ($bytes.Length -ge 2 -and $bytes[0] -eq 0xFF -and $bytes[1] -eq 0xFE) { $encoding = [System.Text.Encoding]::Unicode }
    elseif ($bytes.Length -ge 2 -and $bytes[0] -eq 0xFE -and $bytes[1] -eq 0xFF) { $encoding = [System.Text.Encoding]::BigEndianUnicode }
    $raw = [System.IO.File]::ReadAllText($profilePath)
    $prefix = if ($raw.Length -gt 0 -and -not $raw.EndsWith("`n")) { "`r`n" } else { '' }
    [System.IO.File]::AppendAllText($profilePath, $prefix + $Line + "`r`n", $encoding)
} else {
    $dir = Split-Path -Parent $profilePath
    if (-not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
    Set-Content -LiteralPath $profilePath -Value $Line -Encoding UTF8
    Write-Host "Created your profile: $profilePath"
}

Write-Host "Added this line to your profile:"
Write-Host "  $Line"
Write-Host "Open a new PowerShell window, then type: bridge"
