//! Descoberta e execução de shells locais. Todo acesso a disco/PTY ocorre fora da UI.
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use myssh_core::SessionState;
use myssh_ssh::{SessionCommand, SessionEvent, SessionHandle};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use tokio::sync::mpsc;

use crate::i18n;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shell {
    pub path: PathBuf,
    pub name: String,
}

impl Shell {
    pub fn from_path(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        Self { path, name }
    }
}

fn executable(path: &Path) -> bool {
    let Ok(meta) = path.metadata() else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn add(shells: &mut Vec<Shell>, path: PathBuf, name: Option<&str>) {
    if !path.is_absolute() || !executable(&path) || shells.iter().any(|s| s.path == path) {
        return;
    }
    let name = name.map(str::to_owned).unwrap_or_else(|| {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    });
    shells.push(Shell { path, name });
}

pub fn discover() -> Vec<Shell> {
    let mut shells = Vec::new();
    #[cfg(unix)]
    {
        if let Some(shell) = std::env::var_os("SHELL") {
            add(&mut shells, shell.into(), None);
        }
        if let Ok(contents) = std::fs::read_to_string("/etc/shells") {
            for line in contents.lines() {
                let line = line.split('#').next().unwrap_or_default().trim();
                if !line.is_empty() {
                    add(&mut shells, line.into(), None);
                }
            }
        }
        for path in [
            "/bin/zsh",
            "/bin/bash",
            "/bin/sh",
            "/usr/bin/fish",
            "/usr/local/bin/fish",
            "/opt/homebrew/bin/fish",
        ] {
            add(&mut shells, path.into(), None);
        }
    }
    #[cfg(windows)]
    {
        if let Some(root) = std::env::var_os("SystemRoot") {
            let system = PathBuf::from(root).join("System32");
            add(&mut shells, system.join("cmd.exe"), Some("cmd.exe"));
            add(
                &mut shells,
                system.join("WindowsPowerShell/v1.0/powershell.exe"),
                Some("Windows PowerShell"),
            );
        }
        if let Some(root) = std::env::var_os("ProgramFiles") {
            add(
                &mut shells,
                PathBuf::from(root).join("PowerShell/7/pwsh.exe"),
                Some("PowerShell 7"),
            );
        }
        if let Some(paths) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&paths) {
                add(&mut shells, dir.join("pwsh.exe"), Some("PowerShell"));
                add(
                    &mut shells,
                    dir.join("powershell.exe"),
                    Some("Windows PowerShell"),
                );
            }
        }
    }
    shells
}

/// Reutiliza o protocolo do emulador sem criar conexão SSH nem alterar contratos de core.
pub fn spawn(
    rt: &tokio::runtime::Handle,
    shell: Shell,
    notify: Arc<dyn Fn() + Send + Sync>,
) -> SessionHandle {
    let (cmd_tx, mut cmd_rx) = mpsc::channel(64);
    let (evt_tx, evt_rx) = mpsc::channel(64);
    rt.spawn_blocking(move || {
        let emit = |event| {
            let sent = evt_tx.blocking_send(event).is_ok();
            notify();
            sent
        };
        // Nunca expõe erros do SO: podem conter caminhos/ambiente do usuário.
        let setup = || {
            let pair = native_pty_system()
                .openpty(PtySize {
                    rows: 24,
                    cols: 80,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .ok()?;
            let reader = pair.master.try_clone_reader().ok()?;
            let writer = pair.master.take_writer().ok()?;
            let mut command = CommandBuilder::new(&shell.path);
            #[cfg(unix)]
            command.arg("-i");
            command.env("TERM", "xterm-256color");
            if let Some(home) = directories::UserDirs::new() {
                command.cwd(home.home_dir());
            }
            let child = pair.slave.spawn_command(command).ok()?;
            drop(pair.slave);
            Some((pair.master, reader, writer, child))
        };
        let Some((master, mut reader, mut writer, mut child)) = setup() else {
            emit(SessionEvent::Closed {
                error: Some(i18n::LOCAL_START_ERROR().into()),
            });
            return;
        };
        let (output_tx, mut output_rx) = mpsc::channel(16);
        // Leitura bloqueante isolada: resize/cancelamento continuam sendo processados.
        let reader_thread = std::thread::spawn(move || {
            let mut buf = [0; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if output_tx.blocking_send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    // Em alguns Unix, o fechamento do PTY é reportado como EIO.
                    Err(_) => break,
                }
            }
        });
        // Um shell que deixou de ler stdin não pode impedir resize/desconexão.
        let (input_tx, mut input_rx) = mpsc::channel::<Vec<u8>>(64);
        let write_failed = Arc::new(AtomicBool::new(false));
        let writer_failed = write_failed.clone();
        let writer_thread = std::thread::spawn(move || {
            while let Some(bytes) = input_rx.blocking_recv() {
                if writer
                    .write_all(&bytes)
                    .and_then(|_| writer.flush())
                    .is_err()
                {
                    writer_failed.store(true, Ordering::Release);
                    break;
                }
            }
        });
        emit(SessionEvent::State(SessionState::Connected));
        let mut error = None;
        let mut exited_at = None;
        'io: loop {
            while let Ok(cmd) = cmd_rx.try_recv() {
                let ok = match cmd {
                    SessionCommand::Input(bytes) => input_tx.try_send(bytes).is_ok(),
                    SessionCommand::Resize(s) => master
                        .resize(PtySize {
                            rows: s.rows,
                            cols: s.cols,
                            pixel_width: s.pix_w,
                            pixel_height: s.pix_h,
                        })
                        .is_ok(),
                    SessionCommand::Disconnect => break 'io,
                };
                if !ok {
                    error = Some(i18n::LOCAL_IO_ERROR().into());
                    break 'io;
                }
            }
            if cmd_rx.is_closed() || evt_tx.is_closed() {
                break;
            }
            if write_failed.load(Ordering::Acquire) {
                error = Some(i18n::LOCAL_IO_ERROR().into());
                break;
            }
            // Limita a drenagem por rodada para não atrasar comandos sob saída contínua.
            for _ in 0..16 {
                match output_rx.try_recv() {
                    Ok(bytes) => {
                        if !emit(SessionEvent::Data(bytes)) {
                            break 'io;
                        }
                    }
                    Err(mpsc::error::TryRecvError::Disconnected) => break 'io,
                    Err(mpsc::error::TryRecvError::Empty) => break,
                }
            }
            match child.try_wait() {
                Ok(Some(_)) => {
                    let at = exited_at.get_or_insert_with(std::time::Instant::now);
                    // Drena a saída final sem esperar indefinidamente por descendentes com o PTY aberto.
                    if at.elapsed() >= Duration::from_millis(200) {
                        break;
                    }
                }
                Ok(None) => {}
                Err(_) => {
                    error = Some(i18n::LOCAL_IO_ERROR().into());
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // Não sinaliza um PID que já foi coletado por try_wait.
        if !matches!(child.try_wait(), Ok(Some(_))) {
            let _ = child.kill();
            let _ = child.wait();
        }
        drop(input_tx);
        drop(writer_thread);
        drop(master);
        drop(output_rx);
        // O leitor encerra no EOF; não bloqueia o fechamento se um descendente reteve o PTY.
        drop(reader_thread);
        emit(SessionEvent::State(SessionState::Disconnected));
        emit(SessionEvent::Closed { error });
    });
    SessionHandle::new(cmd_tx, evt_rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descoberta_so_oferece_executaveis_absolutos_sem_duplicatas() {
        let shells = discover();
        for (i, shell) in shells.iter().enumerate() {
            assert!(shell.path.is_absolute() && executable(&shell.path));
            assert!(!shells[..i].iter().any(|s| s.path == shell.path));
        }
        #[cfg(unix)]
        assert!(shells.iter().any(|s| s.path == Path::new("/bin/sh")));
        #[cfg(windows)]
        assert!(shells.iter().any(|s| s.path.ends_with("cmd.exe")));
    }

    #[test]
    fn descoberta_rejeita_pastas_caminhos_relativos_e_arquivos_sem_execucao() {
        let dir = tempfile::tempdir().unwrap();
        let mut shells = Vec::new();
        add(&mut shells, dir.path().to_owned(), None);
        add(&mut shells, PathBuf::from("sh"), None);
        add(&mut shells, dir.path().join("missing"), None);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = dir.path().join("not-executable");
            std::fs::write(&path, "").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            add(&mut shells, path, None);
        }
        assert!(shells.is_empty());
    }

    async fn event(handle: &mut SessionHandle) -> SessionEvent {
        tokio::time::timeout(Duration::from_secs(10), handle.evt_rx.recv())
            .await
            .expect("timeout do console")
            .expect("evento do console")
    }

    #[tokio::test]
    async fn falha_de_abertura_nao_expoe_caminho_do_executavel() {
        let dir = tempfile::tempdir().unwrap();
        let mut handle = spawn(
            &tokio::runtime::Handle::current(),
            Shell {
                path: dir.path().join("private-shell-does-not-exist"),
                name: "test".into(),
            },
            Arc::new(|| {}),
        );
        let SessionEvent::Closed { error: Some(error) } = event(&mut handle).await else {
            panic!("erro de abertura esperado");
        };
        assert_eq!(error, i18n::LOCAL_START_ERROR());
        assert!(!error.contains("private-shell"));
    }

    #[cfg(unix)]
    fn sh() -> Shell {
        Shell {
            path: "/bin/sh".into(),
            name: "sh".into(),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn console_real_recebe_comandos_redimensiona_e_preserva_saida_antes_do_exit() {
        let mut handle = spawn(&tokio::runtime::Handle::current(), sh(), Arc::new(|| {}));
        assert!(matches!(
            event(&mut handle).await,
            SessionEvent::State(SessionState::Connected)
        ));
        handle
            .cmd_tx
            .send(SessionCommand::Resize(myssh_ssh::PtySize {
                cols: 101,
                rows: 37,
                pix_w: 0,
                pix_h: 0,
            }))
            .await
            .unwrap();
        handle
            .cmd_tx
            .send(SessionCommand::Input(
                b"stty size; printf '__MYSSH_%s__\\n' LOCAL_OK; exit\r".to_vec(),
            ))
            .await
            .unwrap();
        let mut output = Vec::new();
        loop {
            match event(&mut handle).await {
                SessionEvent::Data(bytes) => output.extend(bytes),
                SessionEvent::Closed { error } => {
                    assert!(error.is_none());
                    break;
                }
                _ => {}
            }
        }
        let output = String::from_utf8_lossy(&output);
        assert!(output.contains("37 101"), "resize do PTY não foi aplicado");
        assert!(output.contains("__MYSSH_LOCAL_OK__"), "saída final ausente");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn desconectar_nao_espera_shell_que_parou_de_ler_entrada() {
        let mut handle = spawn(&tokio::runtime::Handle::current(), sh(), Arc::new(|| {}));
        assert!(matches!(
            event(&mut handle).await,
            SessionEvent::State(SessionState::Connected)
        ));
        handle
            .cmd_tx
            .send(SessionCommand::Input(
                b"stty -echo; printf '__MYSSH_%s__\\n' READY; sleep 30\r".to_vec(),
            ))
            .await
            .unwrap();
        let mut output = Vec::new();
        while !String::from_utf8_lossy(&output).contains("__MYSSH_READY__") {
            if let SessionEvent::Data(bytes) = event(&mut handle).await {
                output.extend(bytes);
            }
        }
        handle
            .cmd_tx
            .send(SessionCommand::Input(vec![b'x'; 1 << 20]))
            .await
            .unwrap();
        handle
            .cmd_tx
            .send(SessionCommand::Disconnect)
            .await
            .unwrap();
        loop {
            if let SessionEvent::Closed { .. } = event(&mut handle).await {
                break;
            }
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn soltar_handle_encerra_console_ativo() {
        let mut handle = spawn(&tokio::runtime::Handle::current(), sh(), Arc::new(|| {}));
        assert!(matches!(
            event(&mut handle).await,
            SessionEvent::State(SessionState::Connected)
        ));
        let cmd = handle.cmd_tx.clone();
        drop(handle);
        tokio::time::timeout(Duration::from_secs(10), cmd.closed())
            .await
            .expect("console deve encerrar ao fechar aba");
    }
}
