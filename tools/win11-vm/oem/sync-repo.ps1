# Clona (primeira vez) ou atualiza C:\src\mySSH a partir do repositorio do host
# (Z: = \\host.lan\Data; o caminho UNC funciona tambem via SSH, onde Z: nao existe).
# So traz o que esta commitado no host.
# Uso: powershell -ExecutionPolicy Bypass -File \\host.lan\Data\tools\win11-vm\oem\sync-repo.ps1
# (Sem $ErrorActionPreference='Stop': no PowerShell 5.1 o progresso do git em stderr viraria erro.)
$src = '\\host.lan\Data'
$dst = 'C:\src\mySSH'
if (Test-Path "$dst\.git") {
    $branch = git -C $dst rev-parse --abbrev-ref HEAD
    git -C $dst fetch --quiet origin
    if ($LASTEXITCODE -eq 0) { git -C $dst merge --ff-only --quiet "origin/$branch" }
} else {
    git clone --quiet --no-hardlinks $src $dst
}
if ($LASTEXITCODE -ne 0) { Write-Error "git falhou ($LASTEXITCODE)"; exit 1 }
git -C $dst log -1 --oneline
