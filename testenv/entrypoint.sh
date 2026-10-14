#!/bin/sh
# Instala a chave pública de teste, se montada, e inicia o sshd em foreground.
# REGEN_HOSTKEYS=1 gera host keys novas neste container (o container "target" não pode
# compartilhar as chaves do bastion, que vêm da imagem).
if [ -n "$REGEN_HOSTKEYS" ]; then
  rm -f /etc/ssh/ssh_host_*
  ssh-keygen -A >/dev/null
fi
if [ -f /keys/id_ed25519.pub ]; then
  for u in tester mfa; do
    cp /keys/id_ed25519.pub /home/$u/.ssh/authorized_keys
    chown $u:$u /home/$u/.ssh/authorized_keys && chmod 600 /home/$u/.ssh/authorized_keys
  done
fi
exec /usr/sbin/sshd.pam -D -e
