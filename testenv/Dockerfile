# Servidor SSH/SFTP de teste — NUNCA usar fora de desenvolvimento.
FROM alpine:3.22
RUN apk add --no-cache openssh openssh-server-pam openssh-sftp-server linux-pam bash zsh vim procps htop tmux ncurses ncurses-terminfo-base coreutils \
 && ssh-keygen -A \
 && adduser -D -s /bin/bash tester && echo 'tester:tester-pass' | chpasswd \
 && adduser -D -s /bin/bash mfa && echo 'mfa:mfa-code-123456' | chpasswd \
 && adduser -D -s /bin/bash kionly && echo 'kionly:ki-pass' | chpasswd \
 && for u in tester mfa kionly; do mkdir -p /home/$u/.ssh && chmod 700 /home/$u/.ssh && chown -R $u:$u /home/$u/.ssh; done \
 && sed -i 's/^#\?PasswordAuthentication.*/PasswordAuthentication yes/; s/^#\?KbdInteractiveAuthentication.*/KbdInteractiveAuthentication yes/; s/^#\?AllowAgentForwarding.*/AllowAgentForwarding yes/; s/^#\?UsePAM.*/UsePAM yes/; s/^#\?AllowTcpForwarding.*/AllowTcpForwarding yes/' /etc/ssh/sshd_config \
 && grep -q '^UsePAM' /etc/ssh/sshd_config || echo 'UsePAM yes' >> /etc/ssh/sshd_config \
 && grep -q '^AllowTcpForwarding' /etc/ssh/sshd_config || echo 'AllowTcpForwarding yes' >> /etc/ssh/sshd_config
RUN echo 'Subsystem sftp internal-sftp' > /etc/ssh/sshd_config.d/sftp.conf \
 && sed -i 's/^Subsystem.*sftp-server.*//' /etc/ssh/sshd_config
# Usuários de teste de MFA (blocos Match ficam no fim do arquivo):
#  - mfa: chave pública E depois keyboard-interactive (1 prompt "Password:"; o "segundo fator" é o código fixo
#    de teste "mfa-code-123456", que é a senha Unix desse usuário);
#  - kionly: só keyboard-interactive (PasswordAuthentication no), uma rodada, senha "ki-pass".
RUN printf '%s\n' \
  'Match User mfa' \
  '    AuthenticationMethods publickey,keyboard-interactive' \
  'Match User kionly' \
  '    PasswordAuthentication no' \
  '    AuthenticationMethods keyboard-interactive' \
  >> /etc/ssh/sshd_config
COPY pam-sshd /etc/pam.d/sshd
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh
EXPOSE 22
ENTRYPOINT ["/entrypoint.sh"]
