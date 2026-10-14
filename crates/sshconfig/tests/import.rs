use myssh_core::AuthMethod;
use myssh_sshconfig::*;
use std::path::PathBuf;

fn unix() -> ImportOptions {
    ImportOptions::new("/home/ana", "ana")
}

fn run(cfg: &str) -> ImportPreview {
    parse_config(cfg, &unix(), &MemFs::new())
}

fn get<'a>(p: &'a ImportPreview, alias: &str) -> &'a Candidate {
    p.candidates
        .iter()
        .find(|c| c.alias == alias)
        .unwrap_or_else(|| panic!("alias {alias} ausente"))
}

fn has_warn(ws: &[Warning], needle: &str) -> bool {
    ws.iter().any(|w| w.msg.contains(needle))
}

#[test]
fn basico() {
    let p = run("Host web\n  HostName 10.0.0.5\n  Port 2200\n  User deploy\n");
    assert_eq!(p.candidates.len(), 1);
    let c = get(&p, "web");
    assert_eq!(c.profile.name, "web");
    assert_eq!(c.profile.host, "10.0.0.5");
    assert_eq!(c.profile.port, 2200);
    assert_eq!(c.profile.username, "deploy");
    assert_eq!(c.profile.auth, AuthMethod::Password { credential: None });
    assert!(c.warnings.is_empty());
}

#[test]
fn defaults_sem_hostname_e_usuario() {
    let p = run("Host srv\n");
    let c = get(&p, "srv");
    assert_eq!(c.profile.host, "srv");
    assert_eq!(c.profile.username, "ana");
    assert_eq!(c.profile.port, 22);
}

#[test]
fn chaves_case_insensitive_igual_e_aspas() {
    let p = run("HOST=db\n  hostname = \"db host.example\"\n  PORT=2222\n  uSeR \"o'neil\"\n");
    let c = get(&p, "db");
    assert_eq!(c.profile.host, "db host.example");
    assert_eq!(c.profile.port, 2222);
    assert_eq!(c.profile.username, "o'neil");
}

#[test]
fn comentarios() {
    let p = run("# topo\nHost a # comentário\n  HostName a.example # fim\n  # Port 99\n");
    assert_eq!(p.candidates.len(), 1);
    let c = get(&p, "a");
    assert_eq!(c.profile.host, "a.example");
    assert_eq!(c.profile.port, 22);
}

#[test]
fn hash_dentro_de_aspas_e_preservado() {
    let p = run("Host a\n  User \"x#y\"\n");
    assert_eq!(get(&p, "a").profile.username, "x#y");
}

#[test]
fn aspas_nao_fechadas() {
    let p = run("Host a\n  User \"abc\n  Port 22\n");
    let c = get(&p, "a");
    assert!(has_warn(&c.warnings, "Aspas não fechadas"));
    assert_eq!(c.warnings[0].line, 2);
}

#[test]
fn varios_nomes_no_mesmo_host() {
    let p = run("Host a b\n  HostName x.example\n");
    assert_eq!(get(&p, "a").profile.host, "x.example");
    assert_eq!(get(&p, "b").profile.host, "x.example");
}

#[test]
fn curinga_nao_vira_perfil_mas_aplica_defaults() {
    let p = run("Host web1\n  HostName w1\n\nHost *\n  User comum\n  Port 2022\n");
    assert_eq!(p.candidates.len(), 1);
    let c = get(&p, "web1");
    assert_eq!(c.profile.username, "comum");
    assert_eq!(c.profile.port, 2022);
}

#[test]
fn primeiro_valor_vence() {
    // Host específico vem primeiro: vence o curinga que vem depois.
    let p = run("Host a\n  User u1\nHost *\n  User u2\n  Port 99\n");
    let c = get(&p, "a");
    assert_eq!(c.profile.username, "u1");
    assert_eq!(c.profile.port, 99);
    // Curinga primeiro: ele vence o bloco específico.
    let p = run("Host *\n  User u2\nHost a\n  User u1\n");
    assert_eq!(get(&p, "a").profile.username, "u2");
}

#[test]
fn global_antes_de_qualquer_host() {
    let p = run("User global\nHost a\n  HostName h\n");
    assert_eq!(get(&p, "a").profile.username, "global");
}

#[test]
fn curinga_parcial_e_negacao() {
    let cfg = "Host *.prod.example !db.prod.example\n  User prod\nHost app.prod.example db.prod.example dev1\n  Port 22\n";
    let p = run(cfg);
    assert_eq!(get(&p, "app.prod.example").profile.username, "prod");
    assert_eq!(get(&p, "db.prod.example").profile.username, "ana");
    assert_eq!(get(&p, "dev1").profile.username, "ana");
}

#[test]
fn interrogacao() {
    let p = run("Host srv?\n  User x\nHost srv1 srv22\n");
    assert_eq!(get(&p, "srv1").profile.username, "x");
    assert_eq!(get(&p, "srv22").profile.username, "ana");
}

#[test]
fn bloco_so_com_negacao_nao_vira_perfil() {
    let p = run("Host !foo\n  User x\n");
    assert!(p.candidates.is_empty());
}

#[test]
fn alias_duplicado() {
    let p = run("Host a\n  Port 1\nHost a\n  Port 2\n");
    assert_eq!(p.candidates.len(), 1);
    assert_eq!(get(&p, "a").profile.port, 1);
    assert!(has_warn(&p.global_warnings, "duplicado"));
}

#[test]
fn port_invalida() {
    let p = run("Host a\n  Port abc\nHost b\n  Port 0\nHost c\n  Port 70000\n");
    for a in ["a", "b", "c"] {
        let c = get(&p, a);
        assert_eq!(c.profile.port, 22);
        assert!(has_warn(&c.warnings, "inválida"));
    }
}

#[test]
fn keepalive_e_timeout() {
    let p = run(
        "Host a\n  ServerAliveInterval 45\n  ConnectTimeout 7\nHost b\n  ServerAliveInterval 0\nHost c\n  ServerAliveInterval x\n  ConnectTimeout 0\n",
    );
    assert_eq!(get(&p, "a").profile.keepalive_secs, Some(45));
    assert_eq!(get(&p, "a").profile.connect_timeout_secs, 7);
    assert_eq!(get(&p, "b").profile.keepalive_secs, None);
    let c = get(&p, "c");
    assert_eq!(c.warnings.len(), 2);
    assert_eq!(c.profile.keepalive_secs, Some(30));
    assert_eq!(c.profile.connect_timeout_secs, 15);
}

#[test]
fn forward_agent_importado_desligado_com_aviso() {
    let p = run("Host a\n  ForwardAgent yes\nHost b\n  ForwardAgent no\n");
    let a = get(&p, "a");
    assert!(!a.profile.agent_forwarding);
    assert!(has_warn(
        &a.warnings,
        "desabilitado por segurança, ative manualmente"
    ));
    assert!(get(&p, "b").warnings.is_empty());
}

#[test]
fn forward_agent_herdado_de_curinga() {
    let p = run("Host a\nHost *\n  ForwardAgent yes\n");
    let a = get(&p, "a");
    assert!(!a.profile.agent_forwarding);
    assert!(has_warn(&a.warnings, "ForwardAgent"));
}

// ---- IdentityFile -------------------------------------------------------

fn key_path(c: &Candidate) -> PathBuf {
    match &c.profile.auth {
        AuthMethod::KeyFile { path, passphrase } => {
            assert!(passphrase.is_none());
            path.clone()
        }
        o => panic!("auth inesperada: {o:?}"),
    }
}

#[test]
fn identity_file_til_unix() {
    let p = run("Host a\n  IdentityFile ~/.ssh/id_ed25519\n");
    assert_eq!(
        key_path(get(&p, "a")),
        PathBuf::from("/home/ana/.ssh/id_ed25519")
    );
}

#[test]
fn identity_file_tokens() {
    let p =
        run("Host a\n  HostName h.example\n  User bob\n  IdentityFile %d/.ssh/%r_%h_%u_100%%\n");
    assert_eq!(
        key_path(get(&p, "a")),
        PathBuf::from("/home/ana/.ssh/bob_h.example_ana_100%")
    );
}

#[test]
fn identity_file_absoluto_e_relativo() {
    let p = run("Host a\n  IdentityFile /keys/k\nHost b\n  IdentityFile keys/k\n");
    assert_eq!(key_path(get(&p, "a")), PathBuf::from("/keys/k"));
    assert_eq!(key_path(get(&p, "b")), PathBuf::from("/home/ana/keys/k"));
}

#[test]
fn identity_file_windows() {
    let o = ImportOptions::new("C:\\Users\\Ana", "Ana");
    let cfg = "Host a\n  IdentityFile ~/.ssh/id_rsa\n\
               Host b\n  IdentityFile ~\\.ssh\\id_rsa\n\
               Host c\n  IdentityFile %USERPROFILE%\\.ssh\\id_ed25519\n\
               Host d\n  IdentityFile \"D:\\chaves\\k\"\n\
               Host e\n  IdentityFile .ssh\\k\n";
    let p = parse_config(cfg, &o, &MemFs::new());
    assert_eq!(
        key_path(get(&p, "a")),
        PathBuf::from("C:\\Users\\Ana\\.ssh/id_rsa")
    );
    assert_eq!(
        key_path(get(&p, "b")),
        PathBuf::from("C:\\Users\\Ana\\.ssh\\id_rsa")
    );
    assert_eq!(
        key_path(get(&p, "c")),
        PathBuf::from("C:\\Users\\Ana\\.ssh\\id_ed25519")
    );
    assert_eq!(key_path(get(&p, "d")), PathBuf::from("D:\\chaves\\k"));
    assert_eq!(
        key_path(get(&p, "e")),
        PathBuf::from("C:\\Users\\Ana\\.ssh\\k")
    );
}

#[test]
fn varios_identity_files_avisa() {
    let p = run("Host a\n  IdentityFile ~/k1\n  IdentityFile ~/k2\n");
    let c = get(&p, "a");
    assert_eq!(key_path(c), PathBuf::from("/home/ana/k1"));
    assert!(has_warn(&c.warnings, "apenas o primeiro"));
}

// ---- comandos ----------------------------------------------------------

#[test]
fn diretivas_de_comando_nunca_executam() {
    let cfg = "Host a\n  ProxyCommand ssh -W %h:%p bastion\n  LocalCommand touch /tmp/x\n  PermitLocalCommand yes\n  KnownHostsCommand /bin/true\n  HostName a.example\n";
    let p = run(cfg);
    let c = get(&p, "a");
    assert_eq!(c.profile.host, "a.example");
    assert_eq!(c.profile.jump, None);
    assert!(c.jump_alias.is_none());
    assert_eq!(c.warnings.len(), 4);
    assert!(
        c.warnings
            .iter()
            .all(|w| w.msg.contains("ignorada: executa comando local"))
    );
    assert_eq!(c.warnings[0].line, 2);
    assert!(c.warnings[0].msg.contains("ProxyCommand"));
}

#[test]
fn proxycommand_em_curinga_vai_para_global() {
    let p = run("Host *\n  ProxyCommand nc %h %p\nHost a\n");
    assert!(has_warn(
        &p.global_warnings,
        "ignorada: executa comando local"
    ));
    assert!(get(&p, "a").jump_alias.is_none());
}

#[test]
fn match_ignorado_e_match_exec_avisa() {
    let cfg = "Host a\n  User u1\nMatch host a user x\n  User hacker\n  ProxyJump evil\nMatch exec \"curl x | sh\"\n  Port 1\nHost b\n  User u2\n";
    let p = run(cfg);
    assert_eq!(get(&p, "a").profile.username, "u1");
    assert_eq!(get(&p, "a").profile.port, 22);
    assert_eq!(get(&p, "b").profile.username, "u2");
    assert_eq!(p.candidates.len(), 2);
    assert!(has_warn(&p.global_warnings, "Match ignorado"));
    assert!(has_warn(
        &p.global_warnings,
        "Match exec ignorada: executa comando local"
    ));
}

#[test]
fn diretiva_desconhecida() {
    let p = run("Host a\n  Compression yes\n  HostName h\n");
    let c = get(&p, "a");
    assert_eq!(c.warnings.len(), 1);
    assert_eq!(c.warnings[0].line, 2);
    assert!(c.warnings[0].msg.contains("Compression"));
    assert!(c.warnings[0].msg.contains("ignorada"));
}

#[test]
fn diretiva_sem_valor() {
    let p = run("Host a\n  Port\n");
    assert!(has_warn(&get(&p, "a").warnings, "sem valor"));
}

// ---- ProxyJump ---------------------------------------------------------

fn resolved(cfg: &str) -> ImportPreview {
    let mut p = run(cfg);
    resolve_jumps(&mut p.candidates);
    p
}

#[test]
fn proxyjump_para_outro_host() {
    let p = resolved("Host bastion\n  HostName b.example\nHost app\n  ProxyJump bastion\n");
    let app = get(&p, "app");
    assert_eq!(app.jump_alias.as_deref(), Some("bastion"));
    assert_eq!(app.profile.jump, Some(get(&p, "bastion").profile.id));
    assert_eq!(p.candidates.len(), 2);
}

#[test]
fn proxyjump_literal_cria_candidato() {
    let p = resolved("Host app\n  ProxyJump admin@gw.example:2200\n");
    assert_eq!(p.candidates.len(), 2);
    let gw = get(&p, "gw.example");
    assert_eq!(gw.profile.host, "gw.example");
    assert_eq!(gw.profile.username, "admin");
    assert_eq!(gw.profile.port, 2200);
    assert!(has_warn(&gw.warnings, "Criado a partir do ProxyJump"));
    assert_eq!(get(&p, "app").profile.jump, Some(gw.profile.id));
}

#[test]
fn proxyjump_literal_herda_defaults_do_curinga() {
    let p = resolved("Host app\n  ProxyJump gw\nHost *\n  User comum\n");
    // "gw" não é Host explícito: vira candidato com defaults de "Host *".
    assert_eq!(get(&p, "gw").profile.username, "comum");
}

#[test]
fn proxyjump_ipv6() {
    let p = resolved("Host app\n  ProxyJump u@[::1]:2022\n");
    let gw = get(&p, "::1");
    assert_eq!(gw.profile.port, 2022);
    assert_eq!(gw.profile.username, "u");
}

#[test]
fn proxyjump_encadeado_por_hosts() {
    let cfg = "Host a\n  ProxyJump b\nHost b\n  ProxyJump c\nHost c\n  HostName c.example\n";
    let p = resolved(cfg);
    let (a, b, c) = (get(&p, "a"), get(&p, "b"), get(&p, "c"));
    assert_eq!(a.profile.jump, Some(b.profile.id));
    assert_eq!(b.profile.jump, Some(c.profile.id));
    assert_eq!(c.profile.jump, None);
    assert!(a.profile.validate().is_ok());
}

#[test]
fn proxyjump_multiplos_saltos_usa_ultimo_e_avisa() {
    let p = resolved("Host x\n  ProxyJump j1,j2\n");
    let x = get(&p, "x");
    assert_eq!(x.jump_alias.as_deref(), Some("j2"));
    assert!(has_warn(&x.warnings, "2 saltos"));
}

#[test]
fn proxyjump_none() {
    let p = resolved("Host x\n  ProxyJump none\nHost *\n  ProxyJump bastion\n");
    assert!(get(&p, "x").jump_alias.is_none());
    assert_eq!(p.candidates.len(), 1);
}

#[test]
fn proxyjump_proprio_host_ignorado() {
    let p = resolved("Host a\n  ProxyJump a\n");
    let a = get(&p, "a");
    assert!(a.jump_alias.is_none());
    assert!(a.profile.jump.is_none());
    assert!(has_warn(&a.warnings, "próprio host"));
}

#[test]
fn proxyjump_ciclo_e_cortado() {
    let p = resolved("Host a\n  ProxyJump b\nHost b\n  ProxyJump a\n");
    for al in ["a", "b"] {
        let c = get(&p, al);
        assert!(c.profile.jump.is_none());
        assert!(has_warn(&c.warnings, "Ciclo"));
    }
}

#[test]
fn proxyjump_invalido() {
    let p = resolved("Host a\n  ProxyJump u@:x\n");
    assert!(has_warn(&get(&p, "a").warnings, "inválido"));
}

#[test]
fn resolve_jumps_alias_inexistente() {
    let mut p = run("Host a\n  ProxyJump b\n");
    p.candidates.retain(|c| c.alias == "a");
    resolve_jumps(&mut p.candidates);
    let a = get(&p, "a");
    assert!(a.profile.jump.is_none());
    assert!(has_warn(&a.warnings, "não encontrado"));
}

// ---- Include -----------------------------------------------------------

fn fs_with(files: &[(&str, &str)]) -> MemFs {
    files.iter().fold(MemFs::new(), |fs, (p, c)| fs.with(p, c))
}

#[test]
fn include_relativo_com_glob() {
    let fs = fs_with(&[
        (
            "/home/ana/.ssh/conf.d/a.conf",
            "Host a\n  HostName a.example\n",
        ),
        (
            "/home/ana/.ssh/conf.d/b.conf",
            "Host b\n  HostName b.example\n",
        ),
        ("/home/ana/.ssh/outro", "Host nao\n"),
    ]);
    let p = parse_config("Include conf.d/*\nHost c\n", &unix(), &fs);
    let names: Vec<_> = p.candidates.iter().map(|c| c.alias.as_str()).collect();
    assert_eq!(names, ["a", "b", "c"]);
    assert_eq!(get(&p, "a").profile.host, "a.example");
}

#[test]
fn include_absoluto_e_til() {
    let fs = fs_with(&[
        ("/etc/ssh/extra", "Host e\n"),
        ("/home/ana/mais", "Host m\n"),
    ]);
    let p = parse_config("Include /etc/ssh/extra ~/mais\n", &unix(), &fs);
    assert_eq!(p.candidates.len(), 2);
}

#[test]
fn include_inexistente_e_silencioso() {
    let p = parse_config("Include nada/*\nHost a\n", &unix(), &MemFs::new());
    assert_eq!(p.candidates.len(), 1);
    assert!(p.global_warnings.is_empty());
}

#[test]
fn include_dentro_de_host_herda_contexto() {
    let fs = fs_with(&[("/home/ana/.ssh/extra", "User incluido\n")]);
    let p = parse_config("Host a\n  Include extra\nHost b\n", &unix(), &fs);
    assert_eq!(get(&p, "a").profile.username, "incluido");
    assert_eq!(get(&p, "b").profile.username, "ana");
}

#[test]
fn include_ciclo() {
    let fs = fs_with(&[
        ("/home/ana/.ssh/a", "Host ha\nInclude b\n"),
        ("/home/ana/.ssh/b", "Host hb\nInclude a\nInclude config\n"),
        ("/home/ana/.ssh/config", "Host nunca\n"),
    ]);
    let p = parse_config("Include a\nHost raiz\n", &unix(), &fs);
    let names: Vec<_> = p.candidates.iter().map(|c| c.alias.as_str()).collect();
    assert_eq!(names, ["ha", "hb", "raiz"]);
    let n = p
        .candidates
        .iter()
        .map(|c| {
            c.warnings
                .iter()
                .filter(|w| w.msg.contains("ciclo"))
                .count()
        })
        .sum::<usize>()
        + p.global_warnings
            .iter()
            .filter(|w| w.msg.contains("ciclo"))
            .count();
    assert_eq!(n, 2);
}

#[test]
fn include_auto_referencia() {
    let fs = fs_with(&[("/home/ana/.ssh/self", "Host s\nInclude self\n")]);
    let p = parse_config("Include self\n", &unix(), &fs);
    assert_eq!(p.candidates.len(), 1);
}

#[test]
fn include_limite_de_profundidade() {
    let mut fs = MemFs::new();
    for i in 1..=12 {
        let next = if i < 12 {
            format!("Include l{}\n", i + 1)
        } else {
            String::new()
        };
        fs = fs.with(
            &format!("/home/ana/.ssh/l{i}"),
            &format!("Host h{i}\n{next}"),
        );
    }
    let p = parse_config("Include l1\n", &unix(), &fs);
    assert_eq!(p.candidates.len(), 8);
    assert!(
        has_warn(&p.global_warnings, "profundidade máxima")
            || p.candidates
                .iter()
                .any(|c| has_warn(&c.warnings, "profundidade máxima"))
    );
}

#[test]
fn include_em_match_e_ignorado() {
    let fs = fs_with(&[("/home/ana/.ssh/x", "Host x\n")]);
    let p = parse_config("Match all\nInclude x\n", &unix(), &fs);
    assert!(p.candidates.is_empty());
}

#[test]
fn include_windows() {
    let o = ImportOptions::new("C:\\Users\\Ana", "Ana");
    let fs = fs_with(&[("C:\\Users\\Ana\\.ssh\\extra", "Host w\n")]);
    let p = parse_config("Include extra\n", &o, &fs);
    assert_eq!(p.candidates.len(), 1);
}

#[test]
fn aviso_do_include_aponta_arquivo_e_linha() {
    let fs = fs_with(&[("/home/ana/.ssh/extra", "Host a\n\n  Foo bar\n")]);
    let p = parse_config("Include extra\n", &unix(), &fs);
    let w = &get(&p, "a").warnings[0];
    assert_eq!(w.file, "/home/ana/.ssh/extra");
    assert_eq!(w.line, 3);
}

// ---- Exemplo realista ---------------------------------------------------

#[test]
fn exemplo_realista() {
    let cfg = r#"
# Configuração de exemplo
Include conf.d/*.conf

Host bastion
    HostName bastion.corp.example
    User ops
    IdentityFile ~/.ssh/id_corp
    ForwardAgent yes

Host app-prod app-stg
    HostName %h.corp.example
    ProxyJump bastion
    ServerAliveInterval 60

Host legacy
    HostName 192.168.1.10
    Port 2222
    ProxyCommand ssh -W %h:%p bastion
    Ciphers aes128-cbc

Match host *.internal
    User outro

Host *
    User ana-corp
    ServerAliveInterval 20
    AddKeysToAgent yes
"#;
    let fs = fs_with(&[(
        "/home/ana/.ssh/conf.d/gh.conf",
        "Host github.com\n  User git\n  IdentityFile ~/.ssh/id_gh\n",
    )]);
    let mut p = parse_config(cfg, &unix(), &fs);
    resolve_jumps(&mut p.candidates);
    let names: Vec<_> = p.candidates.iter().map(|c| c.alias.as_str()).collect();
    assert_eq!(
        names,
        ["github.com", "bastion", "app-prod", "app-stg", "legacy"]
    );
    assert_eq!(get(&p, "github.com").profile.username, "git");
    let b = get(&p, "bastion");
    assert_eq!(b.profile.username, "ops");
    assert_eq!(b.profile.keepalive_secs, Some(20));
    assert!(has_warn(&b.warnings, "ForwardAgent"));
    let prod = get(&p, "app-prod");
    assert_eq!(prod.profile.host, "app-prod.corp.example");
    assert_eq!(prod.profile.username, "ana-corp");
    assert_eq!(prod.profile.keepalive_secs, Some(60));
    assert_eq!(prod.profile.jump, Some(b.profile.id));
    let l = get(&p, "legacy");
    assert_eq!(l.profile.port, 2222);
    assert!(has_warn(&l.warnings, "ProxyCommand"));
    assert!(has_warn(&l.warnings, "Ciphers"));
    assert!(has_warn(&p.global_warnings, "Match ignorado"));
    assert!(has_warn(&p.global_warnings, "AddKeysToAgent"));
}

#[test]
fn bom_utf8_e_crlf() {
    let p = run("\u{feff}Host a\r\n  Port 2200\r\n");
    assert_eq!(get(&p, "a").profile.port, 2200);
}

#[test]
fn arquivo_vazio() {
    let p = run("");
    assert!(p.candidates.is_empty());
    assert!(p.global_warnings.is_empty());
}

// ---- Orçamento global de Include e avisos sem conteúdo bruto ------------

/// Conta as leituras e os bytes servidos (o orçamento vale por importação, com repetições).
struct Counting {
    inner: MemFs,
    reads: std::cell::Cell<usize>,
    bytes: std::cell::Cell<usize>,
}

impl IncludeSource for Counting {
    fn expand(&self, pattern: &str) -> Vec<String> {
        self.inner.expand(pattern)
    }
    fn read(&self, path: &str) -> std::io::Result<String> {
        let c = self.inner.read(path)?;
        self.reads.set(self.reads.get() + 1);
        self.bytes.set(self.bytes.get() + c.len());
        Ok(c)
    }
}

fn counting(inner: MemFs) -> Counting {
    Counting {
        inner,
        reads: Default::default(),
        bytes: Default::default(),
    }
}

#[test]
fn include_billion_laughs_respeita_orcamento_de_arquivos() {
    // l1 inclui l2 dez vezes, l2 inclui l3 dez vezes... (10^7 leituras sem orçamento).
    let mut fs = MemFs::new();
    for i in 1..=8 {
        let body = if i < 8 {
            format!("Include l{}\n", i + 1).repeat(10)
        } else {
            "Host folha\n".to_string()
        };
        fs = fs.with(&format!("/home/ana/.ssh/l{i}"), &body);
    }
    let c = counting(fs);
    let p = parse_config("Include l1\n", &unix(), &c);
    assert!(c.reads.get() <= 256, "leituras: {}", c.reads.get());
    assert!(
        p.global_warnings
            .iter()
            .any(|w| w.msg.contains("orçamento")),
        "{:?}",
        p.global_warnings
    );
}

#[test]
fn include_billion_laughs_respeita_orcamento_de_bytes() {
    // Um arquivo de ~900 KiB incluído 50 vezes.
    let big = format!("# {}\n", "x".repeat(900 * 1024));
    let root = "Include big\n".repeat(50);
    let c = counting(MemFs::new().with("/home/ana/.ssh/big", &big));
    let p = parse_config(&root, &unix(), &c);
    // Teto de 4 MiB aceitos; a última leitura (descartada) pode passar até 1 MiB.
    assert!(c.bytes.get() <= 5 * 1024 * 1024, "bytes: {}", c.bytes.get());
    assert!(c.reads.get() <= 5, "leituras: {}", c.reads.get());
    assert!(c.reads.get() >= 1);
    let all = p
        .global_warnings
        .iter()
        .chain(p.candidates.iter().flat_map(|c| c.warnings.iter()));
    assert!(all.into_iter().any(|w| w.msg.contains("orçamento")));
}

#[test]
fn include_de_arquivo_sem_formato_de_config_nao_aparece_nos_avisos() {
    let passwd = "root:x:0:0:root:/root:/bin/bash\n\
                  daemon:*:1:1:daemon:/usr/sbin:/usr/sbin/nologin\n\
                  \x1b[31mfake\u{202E}diretiva:com-segredo-s3cr3t\n";
    let fs = fs_with(&[("/etc/passwd", passwd)]);
    for cfg in ["Include /etc/passwd\n", "Host a\n  Include /etc/passwd\n"] {
        let p = parse_config(cfg, &unix(), &fs);
        let all: Vec<&Warning> = p
            .global_warnings
            .iter()
            .chain(p.candidates.iter().flat_map(|c| c.warnings.iter()))
            .collect();
        assert!(!all.is_empty());
        for w in all {
            assert!(!w.msg.contains("/root"), "{}", w.msg);
            assert!(!w.msg.contains("nologin"), "{}", w.msg);
            assert!(!w.msg.contains("s3cr3t"), "{}", w.msg);
            assert!(
                !w.msg.contains('\x1b') && !w.msg.contains('\u{202E}'),
                "{}",
                w.msg
            );
            assert!(w.msg.chars().count() <= 160, "{}", w.msg);
        }
    }
}

#[test]
fn nome_de_diretiva_ignorada_e_truncado() {
    let p = run(&format!("Host a\n  {} v\n", "d".repeat(500)));
    let c = get(&p, "a");
    assert!(c.warnings.iter().all(|w| w.msg.chars().count() <= 160));
    assert!(!c.warnings.is_empty());
}
