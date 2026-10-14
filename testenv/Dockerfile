# Servidor SSH/SFTP de teste — NUNCA usar fora de desenvolvimento.
FROM alpine:3.22
RUN apk add --no-cache openssh openssh-sftp-server bash zsh vim procps htop tmux ncurses ncurses-terminfo-base coreutils \
 && ssh-keygen -A \
 && adduser -D -s /bin/bash tester && echo 'tester:tester-pass' | chpasswd \
 && mkdir -p /home/tester/.ssh && chmod 700 /home/tester/.ssh && chown -R tester:tester /home/tester/.ssh \
 && sed -i 's/^#\?PasswordAuthentication.*/PasswordAuthentication yes/; s/^#\?KbdInteractiveAuthentication.*/KbdInteractiveAuthentication yes/; s/^#\?AllowAgentForwarding.*/AllowAgentForwarding yes/' /etc/ssh/sshd_config \
 && echo 'Subsystem sftp internal-sftp' > /etc/ssh/sshd_config.d/sftp.conf \
 && sed -i 's/^Subsystem.*sftp-server.*//' /etc/ssh/sshd_config
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh
EXPOSE 22
ENTRYPOINT ["/entrypoint.sh"]
