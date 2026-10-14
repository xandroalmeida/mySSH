#!/bin/sh
# Sobe o servidor de teste em 127.0.0.1:2222 (usuário tester / senha tester-pass ou chave testenv/keys/id_ed25519).
set -e
cd "$(dirname "$0")"
mkdir -p keys
[ -f keys/id_ed25519 ] || ssh-keygen -q -t ed25519 -N '' -C mysh-test -f keys/id_ed25519
[ -f keys/id_ed25519_pass ] || ssh-keygen -q -t ed25519 -N 'test-passphrase' -C mysh-test-pass -f keys/id_ed25519_pass
cat keys/id_ed25519.pub keys/id_ed25519_pass.pub > keys/authorized.pub
docker build -q -t myssh-testsshd . >/dev/null
docker rm -f myssh-testsshd >/dev/null 2>&1 || true
docker run -d --name myssh-testsshd -p 127.0.0.1:2222:22 -v "$PWD/keys/authorized.pub:/keys/id_ed25519.pub:ro" myssh-testsshd >/dev/null
sleep 1
echo "myssh-testsshd em 127.0.0.1:2222"
