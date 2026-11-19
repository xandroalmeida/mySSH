# Provisiona a VM de desenvolvimento: VS Build Tools (MSVC + Windows SDK),
# Git for Windows e Rust (stable + rustfmt/clippy), todos para a maquina inteira.
# Roda como SYSTEM no fim da instalacao; pode ser reexecutado como administrador.
# Log: C:\OEM\provision.log. Sucesso total: C:\OEM\provision.done

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
Start-Transcript -Path 'C:\OEM\provision.log' -Append | Out-Null

$dl = 'C:\OEM\downloads'
$rustRoot = 'C:\Rust'
$srcRoot = 'C:\src'
New-Item -ItemType Directory -Force $dl, $srcRoot | Out-Null
$failed = @()

function Get-File([string]$Url, [string]$Out) {
    Invoke-WebRequest -Uri $Url -OutFile $Out -UseBasicParsing
}

function Add-MachinePath([string]$Dir) {
    $path = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    if (($path -split ';') -notcontains $Dir) {
        [Environment]::SetEnvironmentVariable('Path', "$path;$Dir", 'Machine')
    }
    $env:Path = "$env:Path;$Dir"
}

function Step([string]$Name, [scriptblock]$Body) {
    Write-Host "== $Name"
    try { & $Body; Write-Host "== ${Name}: ok" }
    catch { Write-Host "== ${Name}: FALHOU: $_"; $script:failed += $Name }
}

Step 'Defender: exclusoes para builds' {
    # cargo cria milhares de arquivos; a varredura em tempo real deixa o build muito lento.
    Add-MpPreference -ExclusionPath $rustRoot, $srcRoot
}

Step 'Git for Windows' {
    $rel = Invoke-RestMethod 'https://api.github.com/repos/git-for-windows/git/releases/latest' `
        -Headers @{ 'User-Agent' = 'myssh-win11-provision' }
    $asset = $rel.assets | Where-Object { $_.name -match '^Git-[\d.]+-64-bit\.exe$' } | Select-Object -First 1
    if (-not $asset) { throw 'instalador 64-bit nao encontrado na release' }
    Get-File $asset.browser_download_url "$dl\git.exe"
    $p = Start-Process "$dl\git.exe" -ArgumentList '/VERYSILENT', '/NORESTART', '/NOCANCEL', '/SP-', '/SUPPRESSMSGBOXES' -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "instalador saiu com $($p.ExitCode)" }
    $git = 'C:\Program Files\Git\cmd\git.exe'
    # O repositorio vem do compartilhamento Z: (dono diferente do usuario da VM).
    & $git config --system --add safe.directory '*'
    & $git config --system core.autocrlf false
}

Step 'Visual Studio Build Tools (C++)' {
    Get-File 'https://aka.ms/vs/17/release/vs_BuildTools.exe' "$dl\vs_BuildTools.exe"
    $p = Start-Process "$dl\vs_BuildTools.exe" -ArgumentList `
        '--quiet', '--wait', '--norestart', '--nocache',
        '--add', 'Microsoft.VisualStudio.Workload.VCTools', '--includeRecommended' -Wait -PassThru
    if ($p.ExitCode -notin 0, 3010) { throw "instalador saiu com $($p.ExitCode)" }
}

Step 'Rust (rustup, stable)' {
    [Environment]::SetEnvironmentVariable('RUSTUP_HOME', "$rustRoot\rustup", 'Machine')
    [Environment]::SetEnvironmentVariable('CARGO_HOME', "$rustRoot\cargo", 'Machine')
    $env:RUSTUP_HOME = "$rustRoot\rustup"
    $env:CARGO_HOME = "$rustRoot\cargo"
    Get-File 'https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe' "$dl\rustup-init.exe"
    & "$dl\rustup-init.exe" -y --no-modify-path --profile minimal --default-toolchain stable -c rustfmt -c clippy
    if ($LASTEXITCODE -ne 0) { throw "rustup-init saiu com $LASTEXITCODE" }
    Add-MachinePath "$rustRoot\cargo\bin"
    # Usuarios comuns precisam gravar no registro do cargo e atualizar o toolchain.
    icacls $rustRoot /grant '*S-1-5-32-545:(OI)(CI)M' /T /Q | Out-Null
    icacls $srcRoot /grant '*S-1-5-32-545:(OI)(CI)M' /T /Q | Out-Null
}

Step 'OpenSSH Server' {
    & (Join-Path $PSScriptRoot 'enable-ssh.ps1')
}

Remove-Item -Recurse -Force $dl -ErrorAction SilentlyContinue

if ($failed.Count -eq 0) {
    Set-Content 'C:\OEM\provision.done' (Get-Date -Format o)
    Write-Host '== provisionamento concluido'
} else {
    Write-Host "== provisionamento incompleto: $($failed -join ', ')"
}
Stop-Transcript | Out-Null
