# VM Windows 11 (desenvolvimento)

Windows 11 LTSC em KVM via [dockur/windows](https://github.com/dockur/windows), para
compilar e testar a versão Windows no Linux. Só para desenvolvimento.

## Uso

```sh
./tools/win11-vm/up.sh     # sobe (1ª vez: baixa a ISO e instala sozinho, 20-40 min)
./tools/win11-vm/down.sh   # desliga, mantendo o disco
```

- Tela: http://127.0.0.1:8006 ou RDP em `127.0.0.1:3389` (usuário `Docker`, senha `admin`,
  padrão do dockur; as portas só escutam em 127.0.0.1).
- Linha de comando: `./tools/win11-vm/ssh.sh '<comando PowerShell>'` (porta 2223, chave gerada
  por `up.sh` em `~/vms/myssh-win11/ssh/`; host key fixada no 1º acesso). Prefira SSH a digitar
  pelo noVNC, que não repassa bem Shift/Ctrl.
- Disco e ISO ficam em `~/vms/myssh-win11` (`MYSSH_WIN11_DIR`). Recursos: `MYSSH_WIN11_RAM`
  (4G), `MYSSH_WIN11_CPUS` (4), `MYSSH_WIN11_DISK` (40G); o disco só cresce na 1ª instalação.
- Apagar tudo: `./tools/win11-vm/down.sh && rm -rf ~/vms/myssh-win11`.

## O que já vem instalado

No fim da instalação, `oem/provision.ps1` (em `C:\OEM`) instala o VS Build Tools (C++),
o Git for Windows, o Rust stable com rustfmt/clippy (em `C:\Rust`, para todos os usuários) e o
OpenSSH Server (`oem/enable-ssh.ps1`), e exclui `C:\Rust` e `C:\src` da varredura do Defender.
Numa VM instalada antes do SSH: dois cliques em `Z:\tools\win11-vm\oem\enable-ssh.bat`. O log fica em `C:\OEM\provision.log`;
`C:\OEM\provision.done` indica que tudo deu certo. Se algo falhar, rode de novo num
PowerShell de administrador: `powershell -ExecutionPolicy Bypass -File C:\OEM\provision.ps1`.

## Compilar e testar

O repositório do host aparece em somente leitura como `Z:` (pasta "Shared" na área de
trabalho; `\\host.lan\Data`, que também funciona via SSH). Compilar direto ali é lento; clone
para o disco da VM:

```sh
./tools/win11-vm/ssh.sh 'powershell -ExecutionPolicy Bypass -File \\host.lan\Data\tools\win11-vm\oem\sync-repo.ps1'
./tools/win11-vm/ssh.sh '$env:CARGO_BUILD_JOBS=2; cd C:\src\mySSH; cargo test --workspace'
```

`sync-repo.ps1` clona ou atualiza `C:\src\mySSH` e só traz o que está commitado no host.
`CARGO_BUILD_JOBS=2` poupa RAM no link. A interface (`cargo run -p myssh-app`) precisa rodar
na sessão gráfica (noVNC/RDP), não via SSH.

Servidor de teste: com `./testenv/up.sh` rodando no host, a VM está na rede `myssh-testnet` e
alcança o bastion e o alvo **pelo IP** (`docker inspect -f '{{(index .NetworkSettings.Networks
"myssh-testnet").IPAddress}}' myssh-testsshd`): o resolvedor do Windows não envia nomes de um
rótulo só (`myssh-testsshd`) ao DNS. O `127.0.0.1:2222` do host não é visível de dentro da VM.
As chaves de teste estão em `Z:\testenv\keys`. Os testes de integração (`--ignored`) não rodam
na VM: eles leem as host keys com `docker exec`, que não existe no Windows.

## Limitações

- Windows 11 LTSC **Evaluation**: licença de 90 dias; depois, apague o disco e reinstale.

- Sem GPU: o wgpu usa o adaptador por software do Windows (WARP), e a interface fica mais lenta.
- 4 threads e 7,7 GB de RAM divididos com o host: o 1º build é lento. Pare outros containers
  pesados enquanto a VM estiver ligada.
