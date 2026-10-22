use super::*;
use crate::migrate::{MIGRATIONS, migrate};
use myssh_core::trust::{HostEndpoint, HostKey, HostTrustStore, TrustVerdict};
use myssh_core::{AuthMethod, CredentialKind, SecretRef};

fn store() -> Store {
    Store::open_in_memory().unwrap()
}

fn prof(name: &str, host: &str, user: &str) -> Profile {
    Profile::new(name, host, user)
}

fn cred(label: &str) -> Credential {
    Credential {
        id: CredentialId::new(),
        label: label.into(),
        kind: CredentialKind::Password,
        secret: SecretRef("ref-opaca".into()),
    }
}

// ---------- migrações ----------

#[test]
fn migracao_preserva_dados_e_versiona() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite");
    {
        // Banco "antigo": só a migração v1.
        let mut conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        migrate(&mut conn, &MIGRATIONS[..1]).unwrap();
        conn.execute("INSERT INTO settings (key, value) VALUES ('a', 'b')", [])
            .unwrap();
    }
    // Reabre com as migrações atuais + uma extra por cima do banco existente.
    let mut conn = Connection::open(&path).unwrap();
    let mut all: Vec<&str> = MIGRATIONS.to_vec();
    all.push("ALTER TABLE settings ADD COLUMN nota TEXT;");
    migrate(&mut conn, &all).unwrap();
    let v: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, all.len() as i64);
    let val: String = conn
        .query_row("SELECT value FROM settings WHERE key='a'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(val, "b");
    // Reaplicar é no-op.
    migrate(&mut conn, &all).unwrap();
}

#[test]
fn migracao_v2_perfis_antigos_ficam_diretos() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite");
    let id = ProfileId::new();
    {
        let mut conn = Connection::open(&path).unwrap();
        migrate(&mut conn, &MIGRATIONS[..1]).unwrap();
        conn.execute(
            "INSERT INTO profiles (id, name, host, port, username, auth_json, favorite, \
             environment, theme_json, agent_forwarding, keepalive_secs, connect_timeout_secs) \
             VALUES (?1, 'velho', 'h', 22, 'u', '\"Agent\"', 0, 'none', ?2, 0, 30, 15)",
            params![
                id.to_string(),
                serde_json::to_string(&myssh_core::ThemeOverride::default()).unwrap()
            ],
        )
        .unwrap();
    }
    let s = Store::open(&path).unwrap();
    let p = s.get_profile(id).unwrap().unwrap();
    assert_eq!(p.name, "velho");
    assert_eq!(p.transport, myssh_core::Transport::Direct);
}

#[test]
fn perfil_ssm_roundtrip() {
    use myssh_core::{AwsCredentials, AwsSsm, Transport};
    let s = store();
    let mut p = prof("ec2", "i-0123456789abcdef0", "ubuntu");
    p.transport = Transport::AwsSsm(AwsSsm {
        region: "us-east-1".into(),
        credentials: AwsCredentials::Sso {
            start_url: "https://d-123.awsapps.com/start".into(),
            sso_region: "us-east-1".into(),
            account_id: "123456789012".into(),
            role_name: "Admin".into(),
        },
    });
    s.create_profile(&p).unwrap();
    assert_eq!(s.get_profile(p.id).unwrap().unwrap(), p);
    p.transport = Transport::AwsSsm(AwsSsm {
        region: String::new(),
        credentials: AwsCredentials::Profile { name: None },
    });
    s.update_profile(&p).unwrap();
    assert_eq!(s.get_profile(p.id).unwrap().unwrap(), p);
}

#[test]
fn migracao_falha_e_reverte_em_transacao() {
    let mut conn = Connection::open_in_memory().unwrap();
    migrate(&mut conn, &MIGRATIONS[..1]).unwrap();
    let mut all: Vec<&str> = MIGRATIONS.to_vec();
    all.push("CREATE TABLE x (a); SELECT erro_de_sintaxe FROM;");
    assert!(migrate(&mut conn, &all).is_err());
    let existe: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='x'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(existe, 0);
    let v: i64 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, MIGRATIONS.len() as i64);
}

#[test]
fn banco_mais_novo_e_recusado() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "user_version", 99).unwrap();
    assert!(matches!(
        migrate(&mut conn, MIGRATIONS),
        Err(StoreError::DatabaseTooNew)
    ));
}

#[test]
fn arquivo_persiste_entre_aberturas_e_usa_wal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db.sqlite");
    let p = prof("a", "h", "u");
    {
        let s = Store::open(&path).unwrap();
        s.create_profile(&p).unwrap();
        let mode: String = s
            .lock()
            .unwrap()
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
    }
    let s = Store::open(&path).unwrap();
    assert_eq!(s.get_profile(p.id).unwrap().unwrap(), p);
}

#[test]
fn foreign_keys_ligadas() {
    let s = store();
    let v: i64 = s
        .lock()
        .unwrap()
        .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, 1);
}

// ---------- perfis ----------

#[test]
fn crud_perfil_roundtrip_completo() {
    let s = store();
    let folder = Folder::new("Prod", None);
    s.create_folder(&folder).unwrap();
    let c = cred("senha");
    s.create_credential(&c).unwrap();
    let jump = prof("bastion", "b.example", "ops");
    s.create_profile(&jump).unwrap();

    let mut p = prof("db", "db.example", "root");
    p.port = 2200;
    p.auth = AuthMethod::Password {
        credential: Some(c.id),
    };
    p.folder = Some(folder.id);
    p.tags = vec!["db".into(), "pg".into(), "DB".into()];
    p.favorite = true;
    p.environment = Environment::Production;
    p.theme.accent = Some([1, 2, 3]);
    p.theme.palette = Some("solarized".into());
    p.jump = Some(jump.id);
    p.agent_forwarding = true;
    p.keepalive_secs = None;
    p.connect_timeout_secs = 7;
    s.create_profile(&p).unwrap();

    let mut got = s.get_profile(p.id).unwrap().unwrap();
    assert_eq!(got.tags, vec!["db", "pg"]); // dedup sem diferenciar caixa
    p.tags = got.tags.clone();
    assert_eq!(got, p);

    got.name = "db2".into();
    got.auth = AuthMethod::KeyFile {
        path: "/k/id".into(),
        passphrase: None,
    };
    got.tags = vec!["x".into()];
    s.update_profile(&got).unwrap();
    assert_eq!(s.get_profile(p.id).unwrap().unwrap(), got);
    assert!(s.profiles_using_credential(c.id).unwrap().is_empty());

    s.delete_profile(p.id).unwrap();
    assert!(s.get_profile(p.id).unwrap().is_none());
    assert!(matches!(s.delete_profile(p.id), Err(StoreError::NotFound)));
    assert!(matches!(s.update_profile(&p), Err(StoreError::NotFound)));
}

#[test]
fn criar_perfil_duplicado_e_invalido() {
    let s = store();
    let p = prof("a", "h", "u");
    s.create_profile(&p).unwrap();
    assert!(matches!(
        s.create_profile(&p),
        Err(StoreError::AlreadyExists)
    ));
    let mut bad = prof(" ", "h", "u");
    assert!(matches!(
        s.create_profile(&bad),
        Err(StoreError::InvalidProfile(_))
    ));
    bad.name = "ok".into();
    bad.jump = Some(ProfileId::new());
    assert!(matches!(
        s.create_profile(&bad),
        Err(StoreError::InvalidReference)
    ));
}

#[test]
fn ciclo_de_jump_e_detectado() {
    let s = store();
    let a = prof("a", "a", "u");
    let mut b = prof("b", "b", "u");
    let c = prof("c", "c", "u");
    s.create_profile(&a).unwrap();
    b.jump = Some(a.id);
    s.create_profile(&b).unwrap();
    s.create_profile(&c).unwrap();

    // A -> B -> A
    let mut a2 = a.clone();
    a2.jump = Some(b.id);
    assert!(matches!(s.update_profile(&a2), Err(StoreError::JumpCycle)));

    // A -> C -> B -> A (ciclo mais longo)
    let mut c2 = c.clone();
    c2.jump = Some(b.id);
    s.update_profile(&c2).unwrap();
    a2.jump = Some(c.id);
    assert!(matches!(s.update_profile(&a2), Err(StoreError::JumpCycle)));

    // Auto-referência (validate do core)
    let mut a3 = a.clone();
    a3.jump = Some(a.id);
    assert!(matches!(
        s.update_profile(&a3),
        Err(StoreError::InvalidProfile(_))
    ));

    // Banco inalterado após falhas.
    assert_eq!(s.get_profile(a.id).unwrap().unwrap().jump, None);
}

#[test]
fn apagar_jump_host_zera_referencia() {
    let s = store();
    let a = prof("a", "a", "u");
    let mut b = prof("b", "b", "u");
    s.create_profile(&a).unwrap();
    b.jump = Some(a.id);
    s.create_profile(&b).unwrap();
    s.delete_profile(a.id).unwrap();
    assert_eq!(s.get_profile(b.id).unwrap().unwrap().jump, None);
}

#[test]
fn busca_e_filtros() {
    let s = store();
    let f = Folder::new("Trabalho", None);
    s.create_folder(&f).unwrap();
    let mut a = prof("Banco Principal", "db1.corp.example", "postgres");
    a.tags = vec!["Banco".into(), "critico".into()];
    a.folder = Some(f.id);
    a.favorite = true;
    a.environment = Environment::Production;
    let mut b = prof("Web", "www.example", "deploy");
    b.tags = vec!["web".into()];
    b.environment = Environment::Staging;
    let c = prof("Árvore", "x.example", "ÉRICO");
    for p in [&a, &b, &c] {
        s.create_profile(p).unwrap();
    }
    let ids = |flt: ProfileFilter| -> Vec<ProfileId> {
        s.list_profiles(&flt)
            .unwrap()
            .iter()
            .map(|p| p.id)
            .collect()
    };
    let search = |q: &str| {
        ids(ProfileFilter {
            search: Some(q.into()),
            ..Default::default()
        })
    };
    assert_eq!(ids(ProfileFilter::default()).len(), 3);
    assert_eq!(search("PRINCIPAL"), vec![a.id]); // nome
    assert_eq!(search("DB1.CORP"), vec![a.id]); // host
    assert_eq!(search("DEPLOY"), vec![b.id]); // usuário
    assert_eq!(search("crit"), vec![a.id]); // tag
    assert_eq!(search("érico"), vec![c.id]); // unicode
    assert_eq!(search("árv"), vec![c.id]);
    assert!(search("nada").is_empty());
    assert_eq!(
        ids(ProfileFilter {
            folder: Some(f.id),
            ..Default::default()
        }),
        vec![a.id]
    );
    assert_eq!(
        ids(ProfileFilter {
            favorite: Some(true),
            ..Default::default()
        }),
        vec![a.id]
    );
    assert_eq!(
        ids(ProfileFilter {
            environment: Some(Environment::Staging),
            ..Default::default()
        }),
        vec![b.id]
    );
    assert_eq!(
        ids(ProfileFilter {
            tag: Some("WEB".into()),
            ..Default::default()
        }),
        vec![b.id]
    );
    // combinados
    assert!(
        ids(ProfileFilter {
            tag: Some("web".into()),
            favorite: Some(true),
            ..Default::default()
        })
        .is_empty()
    );
}

// ---------- pastas e credenciais ----------

#[test]
fn pastas_crud_ciclo_e_remocao() {
    let s = store();
    let a = Folder::new("A", None);
    s.create_folder(&a).unwrap();
    let mut b = Folder::new("B", Some(a.id));
    s.create_folder(&b).unwrap();
    let mut a2 = a.clone();
    a2.parent = Some(b.id);
    assert!(matches!(s.update_folder(&a2), Err(StoreError::FolderCycle)));
    a2.parent = Some(a.id);
    assert!(matches!(s.update_folder(&a2), Err(StoreError::FolderCycle)));
    b.name = "B2".into();
    s.update_folder(&b).unwrap();
    assert!(matches!(
        s.create_folder(&Folder::new("  ", None)),
        Err(StoreError::EmptyFolderName)
    ));

    let mut p = prof("p", "h", "u");
    p.folder = Some(a.id);
    s.create_profile(&p).unwrap();
    s.delete_folder(a.id).unwrap();
    assert_eq!(s.get_profile(p.id).unwrap().unwrap().folder, None);
    let folders = s.list_folders().unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].parent, None);
    assert!(matches!(s.delete_folder(a.id), Err(StoreError::NotFound)));
}

#[test]
fn credenciais_crud_e_protecao_de_uso() {
    let s = store();
    let mut c = cred("prod");
    s.create_credential(&c).unwrap();
    assert_eq!(s.get_credential(c.id).unwrap().unwrap(), c);
    c.label = "prod2".into();
    s.update_credential(&c).unwrap();
    assert_eq!(s.list_credentials().unwrap(), vec![c.clone()]);

    let mut p = prof("p", "h", "u");
    p.auth = AuthMethod::Password {
        credential: Some(c.id),
    };
    let mut q = prof("q", "h2", "u");
    q.auth = AuthMethod::KeyFile {
        path: "/k".into(),
        passphrase: Some(c.id),
    };
    s.create_profile(&p).unwrap();
    s.create_profile(&q).unwrap();
    let mut used = s.profiles_using_credential(c.id).unwrap();
    used.sort();
    let mut expected = vec![p.id, q.id];
    expected.sort();
    assert_eq!(used, expected);
    match s.delete_credential(c.id) {
        Err(StoreError::CredentialInUse(v)) => assert_eq!(v.len(), 2),
        other => panic!("esperava CredentialInUse, veio {other:?}"),
    }
    s.delete_profile(p.id).unwrap();
    s.delete_profile(q.id).unwrap();
    s.delete_credential(c.id).unwrap();
    assert!(s.get_credential(c.id).unwrap().is_none());
    assert!(matches!(
        s.delete_credential(c.id),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn perfil_com_credencial_inexistente_e_recusado() {
    let s = store();
    let mut p = prof("p", "h", "u");
    p.auth = AuthMethod::Password {
        credential: Some(CredentialId::new()),
    };
    assert!(matches!(
        s.create_profile(&p),
        Err(StoreError::InvalidReference)
    ));
}

#[test]
fn settings_roundtrip() {
    let s = store();
    assert_eq!(s.get_setting("tema").unwrap(), None);
    s.set_setting("tema", "escuro").unwrap();
    s.set_setting("tema", "claro").unwrap();
    assert_eq!(s.get_setting("tema").unwrap().as_deref(), Some("claro"));
    s.delete_setting("tema").unwrap();
    assert_eq!(s.get_setting("tema").unwrap(), None);
}

#[test]
fn erros_nao_vazam_host_ou_usuario() {
    let s = store();
    let p = prof("a", "segredo-host.interno", "usuario-secreto");
    s.create_profile(&p).unwrap();
    let msg = s.create_profile(&p).unwrap_err().to_string();
    assert!(!msg.contains("segredo-host") && !msg.contains("usuario-secreto"));
    let e = Store::open("/caminho/inexistente-xyz/db.sqlite").unwrap_err();
    assert!(!e.to_string().contains("inexistente-xyz"));
}

// ---------- host keys ----------

fn ep() -> HostEndpoint {
    HostEndpoint::new("Servidor.Example", 22)
}

fn key(alg: &str, k: &str) -> HostKey {
    HostKey {
        algorithm: alg.into(),
        key_base64: k.into(),
        fingerprint_sha256: format!("SHA256:{k}"),
    }
}

#[test]
fn veredito_unknown_e_trusted() {
    let s = store();
    let k = key("ssh-ed25519", "AAA");
    assert_eq!(s.verify(&ep(), &k).unwrap(), TrustVerdict::Unknown);
    s.trust(&ep(), &k).unwrap();
    assert_eq!(s.verify(&ep(), &k).unwrap(), TrustVerdict::Trusted);
    // host normalizado; porta diferente é outro endpoint
    let up = HostEndpoint {
        host: "SERVIDOR.example".into(),
        port: 22,
    };
    assert_eq!(s.verify(&up, &k).unwrap(), TrustVerdict::Trusted);
    assert_eq!(
        s.verify(&HostEndpoint::new("servidor.example", 2222), &k)
            .unwrap(),
        TrustVerdict::Unknown
    );
    s.trust(&ep(), &k).unwrap(); // idempotente
}

#[test]
fn veredito_changed_e_trust_falha_replace_funciona() {
    let s = store();
    let old = key("ssh-ed25519", "AAA");
    let new = key("ssh-ed25519", "BBB");
    s.trust(&ep(), &old).unwrap();
    assert_eq!(
        s.verify(&ep(), &new).unwrap(),
        TrustVerdict::Changed {
            previous: old.clone()
        }
    );
    assert!(s.trust(&ep(), &new).is_err());
    // continua com a chave antiga
    assert_eq!(s.verify(&ep(), &old).unwrap(), TrustVerdict::Trusted);
    s.replace(&ep(), &new).unwrap();
    assert_eq!(s.verify(&ep(), &new).unwrap(), TrustVerdict::Trusted);
    assert!(matches!(
        s.verify(&ep(), &old).unwrap(),
        TrustVerdict::Changed { .. }
    ));
}

#[test]
fn veredito_outro_algoritmo() {
    let s = store();
    let ed = key("ssh-ed25519", "AAA");
    s.trust(&ep(), &ed).unwrap();
    let rsa = key("ssh-rsa", "RRR");
    assert_eq!(
        s.verify(&ep(), &rsa).unwrap(),
        TrustVerdict::OtherAlgorithmKnown {
            known: vec![ed.clone()]
        }
    );
    // Confiar explicitamente em outro algoritmo é permitido (não é conflito).
    s.trust(&ep(), &rsa).unwrap();
    assert_eq!(s.verify(&ep(), &rsa).unwrap(), TrustVerdict::Trusted);
    assert_eq!(s.verify(&ep(), &ed).unwrap(), TrustVerdict::Trusted);
}

#[test]
fn veredito_revoked() {
    let s = store();
    let k = key("ssh-ed25519", "AAA");
    s.trust(&ep(), &k).unwrap();
    s.revoke_host_key(&ep(), "ssh-ed25519").unwrap();
    assert_eq!(s.verify(&ep(), &k).unwrap(), TrustVerdict::Revoked);
    assert!(s.trust(&ep(), &k).is_err()); // só replace reabilita
    assert!(s.list().unwrap().is_empty());
    // outra chave do mesmo algoritmo ainda é "mudou"
    assert!(matches!(
        s.verify(&ep(), &key("ssh-ed25519", "ZZZ")).unwrap(),
        TrustVerdict::Changed { .. }
    ));
    // só revogadas de outro algoritmo → Unknown
    assert_eq!(
        s.verify(&ep(), &key("ssh-rsa", "RRR")).unwrap(),
        TrustVerdict::Unknown
    );
    s.replace(&ep(), &k).unwrap();
    assert_eq!(s.verify(&ep(), &k).unwrap(), TrustVerdict::Trusted);
    assert!(matches!(
        s.revoke_host_key(&ep(), "ssh-rsa"),
        Err(StoreError::NotFound)
    ));
}

#[test]
fn forget_e_list() {
    let s = store();
    s.trust(&ep(), &key("ssh-ed25519", "AAA")).unwrap();
    s.trust(&ep(), &key("ssh-rsa", "RRR")).unwrap();
    let other = HostEndpoint::new("outro", 22);
    s.trust(&other, &key("ssh-ed25519", "CCC")).unwrap();
    assert_eq!(s.list().unwrap().len(), 3);
    s.forget(&ep()).unwrap();
    let l = s.list().unwrap();
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].0, other);
    assert_eq!(
        s.verify(&ep(), &key("ssh-ed25519", "AAA")).unwrap(),
        TrustVerdict::Unknown
    );
}

#[test]
fn store_e_send_sync() {
    fn assert_ss<T: Send + Sync>() {}
    assert_ss::<Store>();
}

// ---------- exportação/importação ----------

fn populated() -> (Store, Profile, Profile, Folder) {
    let s = store();
    let f = Folder::new("Infra", None);
    s.create_folder(&f).unwrap();
    let c = cred("pw");
    s.create_credential(&c).unwrap();
    let jump = {
        let mut j = prof("bastion", "bastion.example", "ops");
        j.auth = AuthMethod::KeyFile {
            path: "/home/u/.ssh/id".into(),
            passphrase: Some({
                let pc = Credential {
                    kind: CredentialKind::KeyPassphrase,
                    ..cred("pp")
                };
                s.create_credential(&pc).unwrap();
                pc.id
            }),
        };
        j
    };
    s.create_profile(&jump).unwrap();
    let mut p = prof("app", "app.example", "deploy");
    p.auth = AuthMethod::Password {
        credential: Some(c.id),
    };
    p.folder = Some(f.id);
    p.tags = vec!["app".into(), "prod".into()];
    p.jump = Some(jump.id);
    p.environment = Environment::Production;
    s.create_profile(&p).unwrap();
    (s, p, jump, f)
}

#[test]
fn export_nao_contem_segredos_nem_referencias() {
    let (s, p, jump, _) = populated();
    let json = s.export_config_json().unwrap();
    assert!(json.contains("\"schema_version\": 1"));
    assert!(!json.contains("ref-opaca"));
    assert!(!json.contains("credential"));
    assert!(!json.contains("passphrase"));
    let cfg = s.export_config().unwrap();
    let ep = cfg.profiles.iter().find(|x| x.id == p.id).unwrap();
    assert_eq!(ep.auth, ExportAuth::AskPassword);
    let ej = cfg.profiles.iter().find(|x| x.id == jump.id).unwrap();
    assert_eq!(
        ej.auth,
        ExportAuth::KeyFile {
            path: "/home/u/.ssh/id".into()
        }
    );
    assert_eq!(ep.tags, vec!["app", "prod"]);
}

#[test]
fn importar_em_banco_vazio_preserva_ids_e_relacoes() {
    let (src, p, jump, f) = populated();
    let json = src.export_config_json().unwrap();
    let dst = store();
    let r = dst.import_config_json(&json).unwrap();
    assert_eq!(
        r,
        ImportReport {
            folders: 1,
            profiles: 2,
            ids_regenerated: 0
        }
    );
    let got = dst.get_profile(p.id).unwrap().unwrap();
    assert_eq!(got.auth, AuthMethod::Password { credential: None });
    assert_eq!(got.jump, Some(jump.id));
    assert_eq!(got.folder, Some(f.id));
    assert_eq!(got.tags, p.tags);
    assert_eq!(
        dst.get_profile(jump.id).unwrap().unwrap().auth,
        AuthMethod::KeyFile {
            path: "/home/u/.ssh/id".into(),
            passphrase: None
        }
    );
    assert!(dst.list_credentials().unwrap().is_empty());
}

#[test]
fn importar_colisao_gera_novos_ids_sem_sobrescrever() {
    let (s, p, jump, f) = populated();
    let before = s.get_profile(p.id).unwrap().unwrap();
    let json = s.export_config_json().unwrap();
    let r = s.import_config_json(&json).unwrap();
    assert_eq!(r.ids_regenerated, 3);
    assert_eq!(s.get_profile(p.id).unwrap().unwrap(), before); // intacto
    let all = s.list_profiles(&ProfileFilter::default()).unwrap();
    assert_eq!(all.len(), 4);
    assert_eq!(s.list_folders().unwrap().len(), 2);
    // cópia importada aponta para a cópia do jump, não para o original
    let copy = all
        .iter()
        .find(|x| x.name == "app" && x.id != p.id)
        .unwrap();
    assert_ne!(copy.jump, Some(jump.id));
    assert!(copy.jump.is_some());
    assert_ne!(copy.folder, Some(f.id));
    assert!(copy.folder.is_some());
}

#[test]
fn importacao_valida_schema_e_campos_e_e_atomica() {
    let s = store();
    let base =
        |profiles: &str| format!(r#"{{"schema_version":1,"folders":[],"profiles":[{profiles}]}}"#);
    let id = ProfileId::new();
    let good = |id: ProfileId, extra: &str| {
        format!(
            r#"{{"id":"{id}","name":"n","host":"h","port":22,"username":"u","auth":{{"type":"agent"}},
            "folder":null,"tags":[],"favorite":false,"environment":"None","theme":{{"palette":null,
            "font_family":null,"font_size_tenths":null,"accent":null}},"jump":null,
            "agent_forwarding":false,"keepalive_secs":null,"connect_timeout_secs":5{extra}}}"#
        )
    };
    // schema futuro / lixo / campo desconhecido
    assert!(matches!(
        s.import_config_json(r#"{"schema_version":2,"folders":[],"profiles":[]}"#),
        Err(StoreError::UnsupportedSchema(2))
    ));
    assert!(matches!(
        s.import_config_json("não é json"),
        Err(StoreError::InvalidConfig(_))
    ));
    assert!(matches!(
        s.import_config_json(&base(&good(id, r#","senha":"x""#))),
        Err(StoreError::InvalidConfig(_))
    ));
    // perfil inválido (porta 0)
    let invalid = good(id, "").replace("\"port\":22", "\"port\":0");
    assert!(matches!(
        s.import_config_json(&base(&invalid)),
        Err(StoreError::InvalidProfile(_))
    ));
    // jump inexistente
    let dangling = good(id, "").replace(
        "\"jump\":null",
        &format!("\"jump\":\"{}\"", ProfileId::new()),
    );
    assert!(matches!(
        s.import_config_json(&base(&dangling)),
        Err(StoreError::InvalidConfig(_))
    ));
    // ids duplicados
    let dup = format!("{},{}", good(id, ""), good(id, ""));
    assert!(matches!(
        s.import_config_json(&base(&dup)),
        Err(StoreError::InvalidConfig(_))
    ));
    // ciclo A->B->A
    let (a, b) = (ProfileId::new(), ProfileId::new());
    let ga = good(a, "").replace("\"jump\":null", &format!("\"jump\":\"{b}\""));
    let gb = good(b, "").replace("\"jump\":null", &format!("\"jump\":\"{a}\""));
    assert!(matches!(
        s.import_config_json(&base(&format!("{ga},{gb}"))),
        Err(StoreError::JumpCycle)
    ));
    // pasta inexistente
    let nofolder = good(id, "").replace(
        "\"folder\":null",
        &format!("\"folder\":\"{}\"", FolderId::new()),
    );
    assert!(s.import_config_json(&base(&nofolder)).is_err());
    // nada foi gravado
    assert!(
        s.list_profiles(&ProfileFilter::default())
            .unwrap()
            .is_empty()
    );
    // e o válido passa
    assert_eq!(
        s.import_config_json(&base(&good(id, ""))).unwrap().profiles,
        1
    );
}
