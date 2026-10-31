#!/usr/bin/env bash
# Sobe a VM Windows 11 (dockur/windows). Na primeira vez baixa a ISO e instala
# sozinho (20-40 min); acompanhe em http://127.0.0.1:8006
set -euo pipefail
cd "$(dirname "$0")"

[ -r /dev/kvm ] && [ -w /dev/kvm ] || { echo "sem acesso a /dev/kvm" >&2; exit 1; }

dir="${MYSSH_WIN11_DIR:-$HOME/vms/myssh-win11}"
mkdir -p "$dir/ssh"
chmod 700 "$dir/ssh"

# Chave para ./ssh.sh; a pública vai para C:\OEM e vira administrators_authorized_keys.
[ -f "$dir/ssh/id_ed25519" ] || ssh-keygen -q -t ed25519 -N '' -C myssh-win11 -f "$dir/ssh/id_ed25519"
cp "$dir/ssh/id_ed25519.pub" oem/authorized_keys

# Mesma rede do testenv, para a VM alcançar os servidores SSH de teste.
docker network inspect myssh-testnet >/dev/null 2>&1 || docker network create myssh-testnet >/dev/null

docker compose up -d
echo "VM: http://127.0.0.1:8006  (RDP: 127.0.0.1:3389, usuário Docker, senha admin; SSH: ./ssh.sh)"
echo "Disco: $dir"
