#!/usr/bin/env bash
# Demo automatizada da GUI. Uso: poc/egui/demo.sh [dialogo|completo] [binário]
#   dialogo  : sem fingerprint esperado -> captura o diálogo e rejeita.
#   completo : fingerprint esperado (via ssh-keyscan) -> vim, top, resize, stty.
set -euo pipefail
MODE="${1:-completo}"
HERE="$(cd "$(dirname "$0")" && pwd)"
BIN="${2:-$HERE/../../target/debug/poc-egui}"
KEYS="${MYSSH_TEST_KEYS:-$HERE/../../testenv/keys}"
DATA="$(mktemp -d)"
export MYSSH_POC_DEMO=1
export MYSSH_POC_HOST=127.0.0.1 MYSSH_POC_PORT=2222 MYSSH_POC_USER=tester
export MYSSH_POC_KEY="$KEYS/id_ed25519"
export MYSSH_POC_OUT="$HERE/evidence"
export MYSSH_POC_DATA_DIR="$DATA"
mkdir -p "$MYSSH_POC_OUT"
if [ "$MODE" = completo ]; then
  line="$(ssh-keyscan -p 2222 -t ed25519 127.0.0.1 2>/dev/null | grep -v '^#' | head -1)"
  MYSSH_POC_FINGERPRINT="$(echo "$line" | cut -d' ' -f2- | ssh-keygen -lf - | awk '{print $2}')"
  export MYSSH_POC_FINGERPRINT
  echo "fingerprint esperado: $MYSSH_POC_FINGERPRINT"
fi
"$BIN"
rm -rf "$DATA"
