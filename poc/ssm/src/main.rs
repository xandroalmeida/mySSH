//! POC: SSH até uma instância EC2 via SSM Session Manager, sem aws CLI nem session-manager-plugin.
//!
//! ```text
//! poc-ssm <instance-id> [--region R] [--profile P] [--port 22]
//!         [--host-key SHA256:...] [--user ubuntu --key ~/.ssh/id] [--cmd "uname -a"]
//! ```
//! Sem `--host-key` (ou com valor diferente) a conexão é recusada e o fingerprint apresentado é
//! mostrado, para ser conferido por outro canal. Sem `--key`, para depois do handshake SSH.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use aws_config::{BehaviorVersion, Region};
use russh::client::{self, AuthResult};
use russh::keys::{Algorithm, EcdsaCurve, HashAlg, PrivateKeyWithHashAlg, decode_secret_key};
use russh::{ChannelMsg, Disconnect};
use tokio::io::AsyncWriteExt;

struct Args {
    target: String,
    port: u16,
    region: Option<String>,
    profile: Option<String>,
    host_key: Option<String>,
    user: String,
    key: Option<PathBuf>,
    cmd: String,
}

fn parse_args() -> Result<Args> {
    let mut it = std::env::args().skip(1);
    let mut a = Args {
        target: String::new(),
        port: 22,
        region: None,
        profile: None,
        host_key: None,
        user: "ubuntu".into(),
        key: None,
        cmd: "uname -a; whoami".into(),
    };
    while let Some(arg) = it.next() {
        let mut val = || it.next().with_context(|| format!("{arg} sem valor"));
        match arg.as_str() {
            "--port" => a.port = val()?.parse()?,
            "--region" => a.region = Some(val()?),
            "--profile" => a.profile = Some(val()?),
            "--host-key" => a.host_key = Some(val()?),
            "--user" => a.user = val()?,
            "--key" => a.key = Some(val()?.into()),
            "--cmd" => a.cmd = val()?,
            s if s.starts_with('-') => bail!("opção desconhecida: {s}"),
            s => a.target = s.to_string(),
        }
    }
    if a.target.is_empty() {
        bail!(
            "uso: poc-ssm <instance-id> [--region R] [--profile P] [--port 22] [--host-key SHA256:...] [--user U --key ARQ] [--cmd CMD]"
        );
    }
    Ok(a)
}

struct Handler {
    expected: Option<String>,
    seen: Arc<Mutex<Option<String>>>,
}

impl client::Handler for Handler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } = key else {
            return Ok(false);
        };
        let fp = myssh_ssh::fingerprint_sha256(key);
        *self.seen.lock().unwrap() = Some(fp.clone());
        Ok(self.expected.as_deref() == Some(fp.as_str()))
    }
}

fn ssh_config() -> Arc<client::Config> {
    let mut preferred = russh::Preferred::DEFAULT;
    preferred.key = std::borrow::Cow::Borrowed(&[
        Algorithm::Ed25519,
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP256,
        },
        Algorithm::Rsa {
            hash: Some(HashAlg::Sha512),
        },
        Algorithm::Rsa {
            hash: Some(HashAlg::Sha256),
        },
    ]);
    Arc::new(client::Config {
        preferred,
        nodelay: true,
        ..Default::default()
    })
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("erro: {e:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<ExitCode> {
    let a = parse_args()?;
    let t0 = Instant::now();
    let step = |msg: &str| eprintln!("[{:>6.2}s] {msg}", t0.elapsed().as_secs_f64());

    let mut loader = aws_config::defaults(BehaviorVersion::latest());
    if let Some(r) = &a.region {
        loader = loader.region(Region::new(r.clone()));
    }
    if let Some(p) = &a.profile {
        loader = loader.profile_name(p);
    }
    let cfg = loader.load().await;
    let ssm = aws_sdk_ssm::Client::new(&cfg);

    step(&format!("StartSession {} (porta {})", a.target, a.port));
    let sess = poc_ssm::start_session(&ssm, &a.target, a.port).await?;
    let session_id = sess.session_id.clone();
    step(&format!("sessão {session_id}; abrindo data channel"));

    let res = ssh_over(sess, &a, &step).await;

    match ssm.terminate_session().session_id(&session_id).send().await {
        Ok(_) => step("TerminateSession ok"),
        Err(e) => step(&format!(
            "TerminateSession falhou: {}",
            aws_sdk_ssm::error::DisplayErrorContext(&e)
        )),
    }
    res
}

async fn ssh_over(sess: poc_ssm::SsmSession, a: &Args, step: &impl Fn(&str)) -> Result<ExitCode> {
    let (stream, task) = poc_ssm::open(sess).await?;
    step("handshake com o agente SSM concluído; iniciando SSH");

    let seen = Arc::new(Mutex::new(None));
    let handler = Handler {
        expected: a.host_key.clone(),
        seen: seen.clone(),
    };
    let res = client::connect_stream(ssh_config(), stream, handler).await;
    let fp = seen.lock().unwrap().clone();
    let mut session = match res {
        Ok(s) => s,
        Err(e) => {
            task.abort();
            if let Some(fp) = fp.filter(|fp| a.host_key.as_deref() != Some(fp)) {
                eprintln!(
                    "host key apresentada: {fp}\n\
                     não confere com --host-key (ou ausente). Confira por outro canal e rode com --host-key {fp}"
                );
                return Ok(ExitCode::from(2));
            }
            bail!("SSH: {e}");
        }
    };
    step(&format!(
        "SSH handshake ok; host key confere ({})",
        fp.unwrap_or_default()
    ));

    let Some(key_path) = &a.key else {
        step("sem --key: encerrando após o handshake");
        let _ = session
            .disconnect(Disconnect::ByApplication, "", "en")
            .await;
        drop(session);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), task).await;
        return Ok(ExitCode::SUCCESS);
    };
    let text = tokio::fs::read_to_string(key_path)
        .await
        .context("leitura da chave privada")?;
    let key = decode_secret_key(&text, None)
        .context("chave privada (passphrase não suportada na POC)")?;
    let hash = session
        .best_supported_rsa_hash()
        .await
        .ok()
        .flatten()
        .flatten();
    let auth = session
        .authenticate_publickey(
            a.user.clone(),
            PrivateKeyWithHashAlg::new(Arc::new(key), hash),
        )
        .await?;
    if !matches!(auth, AuthResult::Success) {
        bail!("autenticação por chave recusada para '{}'", a.user);
    }
    step(&format!("autenticado como {}; exec: {}", a.user, a.cmd));

    let mut ch = session.channel_open_session().await?;
    ch.exec(true, a.cmd.as_bytes()).await?;
    let mut out = tokio::io::stdout();
    let mut err = tokio::io::stderr();
    let mut code = None;
    while let Some(msg) = ch.wait().await {
        match msg {
            ChannelMsg::Data { data } => out.write_all(&data).await?,
            ChannelMsg::ExtendedData { data, .. } => err.write_all(&data).await?,
            ChannelMsg::ExitStatus { exit_status } => code = Some(exit_status),
            _ => {}
        }
    }
    out.flush().await?;
    step(&format!("comando terminou (exit {code:?})"));
    let _ = session
        .disconnect(Disconnect::ByApplication, "", "en")
        .await;
    drop(session);
    match tokio::time::timeout(std::time::Duration::from_secs(5), task).await {
        Ok(Ok(Err(e))) => step(&format!("data channel: {e:#}")),
        Err(_) => step("data channel não fechou em 5s"),
        _ => step("data channel fechado"),
    }
    Ok(if code == Some(0) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
