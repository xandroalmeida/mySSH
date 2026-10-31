#!/usr/bin/env bash
# SSH na VM (PowerShell remoto). Ex.: ./tools/win11-vm/ssh.sh 'cargo --version'
# A host key é fixada no 1º acesso em $MYSSH_WIN11_DIR/ssh/known_hosts.
set -euo pipefail
dir="${MYSSH_WIN11_DIR:-$HOME/vms/myssh-win11}/ssh"
exec ssh -i "$dir/id_ed25519" -p 2223 \
  -o UserKnownHostsFile="$dir/known_hosts" -o StrictHostKeyChecking=accept-new \
  -o IdentitiesOnly=yes Docker@127.0.0.1 "$@"
