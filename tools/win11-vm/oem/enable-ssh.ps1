# Habilita o OpenSSH Server na VM (shell padrao: PowerShell) e autoriza a chave do
# host (authorized_keys ao lado deste script, gerado por up.sh). Idempotente.
# Precisa de administrador. Log: C:\OEM\enable-ssh.log
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force 'C:\OEM' | Out-Null
Start-Transcript -Path 'C:\OEM\enable-ssh.log' -Append | Out-Null
try {
    $cap = Get-WindowsCapability -Online | Where-Object Name -like 'OpenSSH.Server*' | Select-Object -First 1
    if ($cap.State -ne 'Installed') { Add-WindowsCapability -Online -Name $cap.Name | Out-Null }

    New-Item -Path 'HKLM:\SOFTWARE\OpenSSH' -Force | Out-Null
    New-ItemProperty -Path 'HKLM:\SOFTWARE\OpenSSH' -Name DefaultShell -PropertyType String -Force `
        -Value 'C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe' | Out-Null

    if (-not (Get-NetFirewallRule -Name 'myssh-sshd' -ErrorAction SilentlyContinue)) {
        New-NetFirewallRule -Name 'myssh-sshd' -DisplayName 'OpenSSH Server (myssh-win11)' `
            -Direction Inbound -Protocol TCP -LocalPort 22 -Action Allow | Out-Null
    }

    $key = Join-Path $PSScriptRoot 'authorized_keys'
    if (Test-Path $key) {
        New-Item -ItemType Directory -Force 'C:\ProgramData\ssh' | Out-Null
        $dst = 'C:\ProgramData\ssh\administrators_authorized_keys'
        Copy-Item $key $dst -Force
        # sshd exige que so Administrators e SYSTEM tenham acesso a este arquivo.
        icacls $dst /inheritance:r /grant '*S-1-5-32-544:F' /grant '*S-1-5-18:F' | Out-Null
    } else {
        Write-Host "aviso: $key nao encontrado; so senha vai funcionar"
    }

    Set-Service sshd -StartupType Automatic
    Restart-Service sshd
    Write-Host '== ssh: ok'
} catch {
    Write-Host "== ssh: FALHOU: $_"
    throw
} finally {
    Stop-Transcript | Out-Null
}
