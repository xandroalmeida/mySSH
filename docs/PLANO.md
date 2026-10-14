# Plano por marcos

| Marco | Aceite (fluxo integrado, servidor real) |
|---|---|
| M0 ✅ | Janela + SSH real + validação de host (fingerprint SHA-256, aceitar/rejeitar) + PTY + vim/top interativos + resize correto. Evidência registrada em `docs/decisoes/0001-stack.md`. |
| M1 ✅ | Perfis em SQLite (CRUD), senha/chave, abas, ≥2 sessões simultâneas, desconectar, estados e erros claros. |
| M2 ✅ | Keyring do SO, agent, keyboard-interactive/MFA, bloqueio, jump host, importação limitada de ~/.ssh/config, tema por perfil, painéis divididos. |
| M3 ✅ | SFTP: navegação local/remota, upload/download recursivo, fila, progresso, cancelamento, política de sobrescrita. |
| M4 | Sync unidirecional com dry-run, exclusões por padrão, conflitos, revalidação, espelho protegido. |
| M5 | CI 3 SOs, instaladores, docs de uso, revisão final de segurança. |
