#!/bin/sh
# Instala a chave pública de teste, se montada, e inicia o sshd em foreground.
if [ -f /keys/id_ed25519.pub ]; then
  cp /keys/id_ed25519.pub /home/tester/.ssh/authorized_keys
  chown tester:tester /home/tester/.ssh/authorized_keys && chmod 600 /home/tester/.ssh/authorized_keys
fi
exec /usr/sbin/sshd -D -e
