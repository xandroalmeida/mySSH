//! Testes contra a AWS de verdade (ignorados por padrão).
//!
//! SSM: `MYSSH_TEST_SSM_TARGET` (instância EC2 `i-…` com SSM Agent e ec2-instance-connect),
//! `MYSSH_TEST_SSM_REGION`, `MYSSH_TEST_SSM_USER` e `MYSSH_TEST_SSM_HOSTKEY` (fingerprint SHA256
//! esperado, obtido por outro canal). Autentica com chave efêmera pelo EC2 Instance Connect: nada
//! a preparar no servidor. Opcional: `MYSSH_TEST_SSM_PROFILE` (perfil AWS). Credenciais pela
//! cadeia padrão do `aws-config`.

use std::sync::Arc;
use std::time::Duration;

use myssh_core::trust::{HostEndpoint, HostKey, HostTrustStore, TrustStoreError, TrustVerdict};
use myssh_core::{AwsCredentials, AwsSsm, SessionState};
use myssh_secrets::{MemoryStore, Vault};
use myssh_ssh::{AuthMaterial, ConnectRequest, PtySize, SessionCommand, SessionEvent, spawn};

/// Confia só na chave com o fingerprint fixado pelo teste.
struct Pinned(String);

impl HostTrustStore for Pinned {
    fn verify(&self, _: &HostEndpoint, key: &HostKey) -> Result<TrustVerdict, TrustStoreError> {
        Ok(if key.fingerprint_sha256 == self.0 {
            TrustVerdict::Trusted
        } else {
            TrustVerdict::Unknown
        })
    }
    fn trust(&self, _: &HostEndpoint, _: &HostKey) -> Result<(), TrustStoreError> {
        Ok(())
    }
    fn replace(&self, _: &HostEndpoint, _: &HostKey) -> Result<(), TrustStoreError> {
        Ok(())
    }
    fn forget(&self, _: &HostEndpoint) -> Result<(), TrustStoreError> {
        Ok(())
    }
    fn list(&self) -> Result<Vec<(HostEndpoint, HostKey)>, TrustStoreError> {
        Ok(vec![])
    }
}

fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| panic!("defina {k}"))
}

#[test]
#[ignore = "precisa de AWS + instância com SSM (ver cabeçalho)"]
fn shell_via_ssm_com_instance_connect() {
    if std::env::var_os("MYSSH_TEST_SSM_TARGET").is_none() {
        eprintln!("MYSSH_TEST_SSM_TARGET não definido: nada a fazer");
        return;
    }
    let rt = tokio::runtime::Runtime::new().unwrap();
    let target = env("MYSSH_TEST_SSM_TARGET");
    let ssm = AwsSsm {
        region: env("MYSSH_TEST_SSM_REGION"),
        credentials: AwsCredentials::Profile {
            name: std::env::var("MYSSH_TEST_SSM_PROFILE").ok(),
        },
    };
    let vault = Arc::new(Vault::new(Arc::new(MemoryStore::new())));
    let user = env("MYSSH_TEST_SSM_USER");
    let (auth, public) = AuthMaterial::ephemeral().unwrap();
    let tunnel = Arc::new(
        myssh_aws::SsmTunnel::new(&target, 22, ssm, vault)
            .with_instance_connect(user.clone(), public),
    );
    let req = ConnectRequest {
        endpoint: HostEndpoint::new(&target, 22),
        username: user,
        auth,
        pty: PtySize {
            cols: 80,
            rows: 24,
            pix_w: 0,
            pix_h: 0,
        },
        term: "xterm".into(),
        connect_timeout: Duration::from_secs(60),
        keepalive: None,
        jumps: vec![],
        tunnel: Some(tunnel),
    };
    let trust: Arc<dyn HostTrustStore> = Arc::new(Pinned(env("MYSSH_TEST_SSM_HOSTKEY")));
    let mut h = spawn(rt.handle(), req, trust, Arc::new(|| {}));
    let out = rt.block_on(async {
        let mut out = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
        loop {
            match tokio::time::timeout_at(deadline, h.evt_rx.recv()).await {
                // Input antes da shell abrir é descartado pela sessão: envia ao conectar.
                Ok(Some(SessionEvent::State(SessionState::Connected))) => h
                    .cmd_tx
                    .send(SessionCommand::Input(
                        b"echo SSM_OK_$((40+2)); exit\n".to_vec(),
                    ))
                    .await
                    .unwrap(),
                Ok(Some(SessionEvent::Data(d))) => out.extend(d),
                Ok(Some(SessionEvent::Closed { error })) => {
                    assert!(error.is_none(), "sessão fechou com erro: {error:?}");
                    break;
                }
                Ok(Some(SessionEvent::HostKeyUnknown { key, .. })) => {
                    panic!("host key inesperada: {}", key.fingerprint_sha256)
                }
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(_) => panic!("timeout; saída até aqui: {}", String::from_utf8_lossy(&out)),
            }
        }
        out
    });
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("SSM_OK_42"), "saída: {text}");
}

#[test]
#[ignore = "precisa de rede (AWS sso-oidc)"]
fn sso_registro_real_e_start_url_invalida() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let vault = Vault::new(Arc::new(MemoryStore::new()));
    // RegisterClient é público e funciona sem Identity Center; a start URL inexistente falha
    // em StartDeviceAuthorization com erro traduzido, sem vazar segredo.
    let err = rt
        .block_on(myssh_aws::sso::begin_login(
            &vault,
            "https://d-0000000000.awsapps.com/start",
            "us-east-1",
        ))
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("StartDeviceAuthorization"), "{msg}");
    // O registro do cliente ficou no cofre e é reaproveitado na próxima tentativa.
    let err2 = rt
        .block_on(myssh_aws::sso::begin_login(
            &vault,
            "https://d-0000000000.awsapps.com/start",
            "us-east-1",
        ))
        .unwrap_err();
    assert!(err2.to_string().contains("StartDeviceAuthorization"));
}
