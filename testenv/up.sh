#!/bin/sh
# Sobe os servidores de teste:
#  - myssh-testsshd         bastion/servidor principal em 127.0.0.1:2222
#  - myssh-testsshd-target  destino do jump host: SEM porta publicada, só alcançável pela rede docker
#                           "myssh-testnet" a partir do bastion (myssh-testsshd-target:22), com host keys próprias.
# Usuários (ambos): tester / tester-pass ou chave testenv/keys/id_ed25519;
#   mfa     = chave pública + keyboard-interactive (1 prompt "Password:", resposta fixa mfa-code-123456);
#   /home/tester/small = tmpfs de 2 MiB (só no bastion) para testar disco cheio;
#   kionly  = só keyboard-interactive (1 prompt, resposta ki-pass).
set -e
cd "$(dirname "$0")"
mkdir -p keys
[ -f keys/id_ed25519 ] || ssh-keygen -q -t ed25519 -N '' -C mysh-test -f keys/id_ed25519
[ -f keys/id_ed25519_pass ] || ssh-keygen -q -t ed25519 -N 'test-passphrase' -C mysh-test-pass -f keys/id_ed25519_pass
cat keys/id_ed25519.pub keys/id_ed25519_pass.pub > keys/authorized.pub
docker build -q -t myssh-testsshd . >/dev/null
docker network inspect myssh-testnet >/dev/null 2>&1 || docker network create myssh-testnet >/dev/null
docker rm -f myssh-testsshd myssh-testsshd-target >/dev/null 2>&1 || true
docker run -d --name myssh-testsshd --network myssh-testnet -p 127.0.0.1:2222:22 --tmpfs /home/tester/small:size=2m,uid=1000,gid=1000,mode=0755 -v "$PWD/keys/authorized.pub:/keys/id_ed25519.pub:ro" myssh-testsshd >/dev/null
docker run -d --name myssh-testsshd-target --network myssh-testnet -e REGEN_HOSTKEYS=1 -v "$PWD/keys/authorized.pub:/keys/id_ed25519.pub:ro" myssh-testsshd >/dev/null
sleep 1
echo "myssh-testsshd em 127.0.0.1:2222 (bastion); myssh-testsshd-target:22 só pela rede docker myssh-testnet"
