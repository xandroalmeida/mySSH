#!/usr/bin/env bash
# Desliga a VM (o Windows recebe sinal de desligamento; o disco é mantido).
# Para apagar tudo: ./down.sh && rm -rf "${MYSSH_WIN11_DIR:-$HOME/vms/myssh-win11}"
set -euo pipefail
cd "$(dirname "$0")"
docker compose down
