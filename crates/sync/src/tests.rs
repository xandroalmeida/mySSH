use crate::*;
use std::time::Duration;

fn opts() -> PlanOptions {
    PlanOptions::new(Direction::LocalToRemote)
}

fn find<'a>(p: &'a Plan, path: &str) -> &'a PlanItem {
    p.items
        .iter()
        .find(|i| i.path.as_str() == path)
        .unwrap_or_else(|| panic!("sem item {path}: {:?}", p.items))
}

fn is_create(p: &Plan, path: &str) -> bool {
    find(p, path).action == Action::Create
}

fn conflict(p: &Plan, path: &str) -> ConflictReason {
    match &find(p, path).action {
        Action::Conflict { reason } => reason.clone(),
        a => panic!("{path}: esperava conflito, veio {a:?}"),
    }
}

fn skip(p: &Plan, path: &str) -> SkipReason {
    match &find(p, path).action {
        Action::Skip { reason } => reason.clone(),
        a => panic!("{path}: esperava skip, veio {a:?}"),
    }
}

#[tokio::test]
async fn cria_arquivos_e_dirs_com_espacos_e_unicode() {
    let mut s = MemTree::new();
    s.add_file("meu arquivo.txt", 10, Some(100));
    s.add_file("pasta ção/ñandú 日本.txt", 20, Some(100));
    let d = MemTree::new();
    let p = plan(&s, &d, &opts()).await.unwrap();
    assert!(is_create(&p, "meu arquivo.txt"));
    assert!(is_create(&p, "pasta ção"));
    assert!(is_create(&p, "pasta ção/ñandú 日本.txt"));
    // Pai antes do filho.
    let pos = |n: &str| p.items.iter().position(|i| i.path.as_str() == n).unwrap();
    assert!(pos("pasta ção") < pos("pasta ção/ñandú 日本.txt"));
    let sum = p.summary();
    assert_eq!((sum.create, sum.bytes_to_transfer), (3, 30));
}

#[tokio::test]
async fn arquivo_grande_no_metadado() {
    let mut s = MemTree::new();
    s.add_file("big.iso", 8 * 1024 * 1024 * 1024 * 1024, Some(10));
    let p = plan(&s, &MemTree::new(), &opts()).await.unwrap();
    assert_eq!(p.summary().bytes_to_transfer, 8 * 1024u64.pow(4));
}

#[tokio::test]
async fn igual_dentro_da_tolerancia_e_update_fora() {
    let mut s = MemTree::new();
    let mut d = MemTree::new();
    s.add_file("igual", 5, Some(100));
    d.add_file("igual", 5, Some(101)); // 1 s < 2 s
    s.add_file("novo", 5, Some(110));
    d.add_file("novo", 5, Some(100));
    s.add_file("tam", 6, Some(100));
    d.add_file("tam", 5, Some(100));
    let p = plan(&s, &d, &opts()).await.unwrap();
    assert_eq!(skip(&p, "igual"), SkipReason::Identical);
    assert_eq!(
        find(&p, "novo").action,
        Action::Update {
            reason: UpdateReason::SourceNewer
        }
    );
    assert_eq!(
        find(&p, "tam").action,
        Action::Update {
            reason: UpdateReason::SizeDiffers
        }
    );
}

#[tokio::test]
async fn tolerancia_usa_maior_resolucao() {
    // Destino FAT-like com resolução de 10 s: 8 s de diferença ainda é igual.
    let mut s = MemTree::new();
    let mut d = MemTree::new().with_resolution(Duration::from_secs(10));
    s.add_file("f", 5, Some(108));
    d.add_file("f", 5, Some(100));
    let p = plan(&s, &d, &opts()).await.unwrap();
    assert_eq!(skip(&p, "f"), SkipReason::Identical);
}

#[tokio::test]
async fn destino_mais_novo_e_sempre_conflito_nunca_skip() {
    let mut s = MemTree::new();
    let mut d = MemTree::new();
    s.add_file("f", 5, Some(100));
    d.add_file("f", 9, Some(500));
    s.add_file("g", 5, Some(100));
    d.add_file("g", 5, Some(500));
    let p = plan(&s, &d, &opts()).await.unwrap();
    assert_eq!(conflict(&p, "f"), ConflictReason::DestModifiedAfter);
    assert_eq!(conflict(&p, "g"), ConflictReason::DestNewer);
    assert_eq!(p.summary().conflict, 2);
}

#[tokio::test]
async fn sem_mtime_e_conflito() {
    let mut s = MemTree::new();
    let mut d = MemTree::new();
    s.add_file("f", 5, None);
    d.add_file("f", 5, Some(1));
    let p = plan(&s, &d, &opts()).await.unwrap();
    assert_eq!(conflict(&p, "f"), ConflictReason::MtimeUnknown);
}

#[tokio::test]
async fn tipo_diferente_e_conflito_nunca_sobrescreve() {
    let mut s = MemTree::new();
    let mut d = MemTree::new();
    s.add_file("x", 1, Some(1));
    d.add_dir("x");
    s.add_dir("y");
    d.add_file("y", 1, Some(1));
    let mut o = opts();
    o.mirror_deletes = true;
    let p = plan(&s, &d, &o).await.unwrap();
    assert_eq!(conflict(&p, "x"), ConflictReason::TypeMismatch);
    assert_eq!(conflict(&p, "y"), ConflictReason::TypeMismatch);
    assert_eq!(p.summary().delete, 0);
    assert_eq!(p.summary().update, 0);
}

#[tokio::test]
async fn symlinks_ignorados_e_follow_nao_suportado() {
    let mut s = MemTree::new();
    s.add_symlink("link");
    s.add_symlink("d/link2");
    s.add_file("ok", 1, Some(1));
    let p = plan(&s, &MemTree::new(), &opts()).await.unwrap();
    assert_eq!(skip(&p, "link"), SkipReason::Symlink);
    assert_eq!(skip(&p, "d/link2"), SkipReason::Symlink);
    assert!(is_create(&p, "ok"));
    let mut o = opts();
    o.follow_symlinks = true;
    assert!(matches!(
        plan(&s, &MemTree::new(), &o).await,
        Err(PlanError::FollowSymlinksUnsupported)
    ));
}

#[tokio::test]
async fn symlink_no_destino_e_conflito() {
    let mut s = MemTree::new();
    let mut d = MemTree::new();
    s.add_file("f", 1, Some(1));
    d.add_symlink("f");
    let p = plan(&s, &d, &opts()).await.unwrap();
    assert_eq!(conflict(&p, "f"), ConflictReason::DestSpecial);
}

#[tokio::test]
async fn exclusao_padrao_git_e_sem_exclusao() {
    let mut s = MemTree::new();
    s.add_file(".git/HEAD", 1, Some(1));
    s.add_file("src/.git/x", 1, Some(1));
    s.add_file("a.txt", 1, Some(1));
    let p = plan(&s, &MemTree::new(), &opts()).await.unwrap();
    assert!(p.items.iter().all(|i| !i.path.as_str().contains(".git")));
    assert!(is_create(&p, "a.txt"));

    let mut o = opts();
    o.exclusions = Exclusions::none();
    let p = plan(&s, &MemTree::new(), &o).await.unwrap();
    assert!(is_create(&p, ".git/HEAD"));
}

#[tokio::test]
async fn exclusao_do_usuario_nao_percorre_dir() {
    let mut s = MemTree::new();
    s.add_file("node_modules/a/b.js", 1, Some(1));
    s.add_file("x.log", 1, Some(1));
    s.add_file("keep.txt", 1, Some(1));
    let mut o = opts();
    o.exclusions.add("node_modules/").unwrap();
    o.exclusions.add("*.log").unwrap();
    let p = plan(&s, &MemTree::new(), &o).await.unwrap();
    assert_eq!(p.items.len(), 1);
    assert!(is_create(&p, "keep.txt"));
}

#[tokio::test]
async fn sem_espelho_nunca_deleta() {
    let s = MemTree::new();
    let mut d = MemTree::new();
    d.add_file("so_no_destino", 1, Some(1));
    let p = plan(&s, &d, &opts()).await.unwrap();
    assert!(p.items.is_empty());
}

#[tokio::test]
async fn espelho_deleta_filhos_antes_do_pai_e_respeita_exclusoes() {
    let s = MemTree::new();
    let mut d = MemTree::new();
    d.add_file("velho.txt", 7, Some(1));
    d.add_file("dir/sub/a", 1, Some(1));
    d.add_file("dir/b", 2, Some(1));
    d.add_file(".git/HEAD", 1, Some(1));
    d.add_file("com_excl/x", 1, Some(1));
    d.add_file("com_excl/.git/y", 1, Some(1));
    let mut o = opts();
    o.mirror_deletes = true;
    let p = plan(&s, &d, &o).await.unwrap();
    let pos = |n: &str| p.items.iter().position(|i| i.path.as_str() == n).unwrap();
    assert!(pos("dir/sub/a") < pos("dir/sub"));
    assert!(pos("dir/sub") < pos("dir"));
    assert!(pos("dir/b") < pos("dir"));
    assert_eq!(find(&p, "dir").action, Action::Delete);
    assert_eq!(find(&p, "velho.txt").action, Action::Delete);
    assert!(p.items.iter().all(|i| !i.path.as_str().starts_with(".git")));
    assert_eq!(find(&p, "com_excl/x").action, Action::Delete);
    assert_eq!(skip(&p, "com_excl"), SkipReason::ContainsExcluded);
    let sum = p.summary();
    assert_eq!(sum.bytes_to_delete, 7 + 1 + 2 + 1);
}

#[tokio::test]
async fn colisao_de_caixa_na_origem_e_no_destino() {
    let mut s = MemTree::new();
    s.add_file("Foo.txt", 1, Some(1));
    s.add_file("foo.txt", 1, Some(1));
    s.add_file("Bar", 1, Some(1));
    s.add_file("ok", 1, Some(1));
    let mut d = MemTree::new();
    d.add_file("BAR", 1, Some(1));

    // Destino case-sensitive: nada colide.
    let p = plan(&s, &d, &opts()).await.unwrap();
    assert!(is_create(&p, "Foo.txt") && is_create(&p, "foo.txt") && is_create(&p, "Bar"));

    let mut o = opts();
    o.case_insensitive_dest = true;
    let p = plan(&s, &d, &o).await.unwrap();
    assert!(matches!(
        conflict(&p, "Foo.txt"),
        ConflictReason::SourceCollision { .. }
    ));
    assert!(matches!(
        conflict(&p, "foo.txt"),
        ConflictReason::SourceCollision { .. }
    ));
    assert_eq!(
        conflict(&p, "Bar"),
        ConflictReason::DestCollision {
            existing: "BAR".into()
        }
    );
    assert!(is_create(&p, "ok"));
}

#[tokio::test]
async fn colisao_nfc_nfd() {
    let nfc = "caf\u{e9}.txt";
    let nfd = "cafe\u{301}.txt";
    assert_ne!(nfc, nfd);
    let mut s = MemTree::new();
    s.add_file(nfc, 1, Some(1));
    let mut d = MemTree::new();
    d.add_file(nfd, 1, Some(1));
    let p = plan(&s, &d, &opts()).await.unwrap();
    assert!(is_create(&p, nfc)); // Linux: nomes distintos

    let mut o = opts();
    o.unicode_normalizing_dest = true;
    let p = plan(&s, &d, &o).await.unwrap();
    assert!(matches!(
        conflict(&p, nfc),
        ConflictReason::DestCollision { .. }
    ));

    // Colisão dentro da própria origem.
    let mut s2 = MemTree::new();
    s2.add_file(nfc, 1, Some(1));
    s2.add_file(nfd, 1, Some(1));
    let p = plan(&s2, &MemTree::new(), &o).await.unwrap();
    assert!(matches!(
        conflict(&p, nfc),
        ConflictReason::SourceCollision { .. }
    ));
    assert!(matches!(
        conflict(&p, nfd),
        ConflictReason::SourceCollision { .. }
    ));
}

#[tokio::test]
async fn espelho_nao_apaga_nome_que_colide() {
    let mut s = MemTree::new();
    s.add_file("Foo", 1, Some(1));
    let mut d = MemTree::new();
    d.add_file("foo", 1, Some(1));
    let mut o = opts();
    o.case_insensitive_dest = true;
    o.mirror_deletes = true;
    let p = plan(&s, &d, &o).await.unwrap();
    assert_eq!(p.summary().delete, 0);
    assert_eq!(p.summary().conflict, 1);
}

#[test]
fn nomes_windows() {
    for bad in [
        "a<b", "a>b", "a:b", "a\"b", "a|b", "a?b", "a*b", "CON", "con.txt", "NUL", "Com1",
        "lpt9.log", "fim.", "fim ", "aux", "x\u{1}y",
    ] {
        assert!(is_invalid_windows_name(bad), "{bad:?}");
    }
    for ok in [
        "COM0", "COM10", "console", "a b", "ção", "nul_x", ".git", "a.b",
    ] {
        assert!(!is_invalid_windows_name(ok), "{ok:?}");
    }
}

#[tokio::test]
async fn destino_windows_pula_nomes_invalidos() {
    let mut s = MemTree::new();
    s.add_file("CON", 1, Some(1));
    s.add_file("dir./x", 1, Some(1));
    s.add_file("bom.txt", 1, Some(1));
    let mut o = opts();
    o.dest_windows = true;
    let p = plan(&s, &MemTree::new(), &o).await.unwrap();
    assert!(matches!(
        skip(&p, "CON"),
        SkipReason::WindowsInvalidName { .. }
    ));
    assert!(matches!(
        skip(&p, "dir."),
        SkipReason::WindowsInvalidName { .. }
    ));
    assert!(p.items.iter().all(|i| i.path.as_str() != "dir./x"));
    assert!(is_create(&p, "bom.txt"));
}

#[tokio::test]
async fn nomes_maliciosos_do_remoto_sao_pulados() {
    let mut s = MemTree::new();
    let mk = |n: &str| Entry {
        name: n.into(),
        kind: EntryKind::File,
        size: 1,
        mtime: None,
        mtime_resolution: Duration::from_secs(1),
        perms: None,
    };
    for n in ["..", ".", "a\\b", "x/y", "", "C:\\evil", "nul\0"] {
        s.add_raw("", mk(n));
    }
    s.add_file("ok", 1, Some(1));
    let p = plan(&s, &MemTree::new(), &opts()).await.unwrap();
    let invalid = p
        .items
        .iter()
        .filter(|i| {
            matches!(
                i.action,
                Action::Skip {
                    reason: SkipReason::InvalidName { .. }
                }
            )
        })
        .count();
    assert_eq!(invalid, 7);
    assert!(is_create(&p, "ok"));
    assert_eq!(p.items.len(), 8);
}

#[tokio::test]
async fn modo_conteudo_marca_requer_hash() {
    let mut s = MemTree::new();
    let mut d = MemTree::new();
    s.add_file("mesmo", 5, Some(1));
    d.add_file("mesmo", 5, Some(999));
    s.add_file("difere", 6, Some(1));
    d.add_file("difere", 5, Some(1));
    let mut o = opts();
    o.compare = CompareMode::Content;
    let mut p = plan(&s, &d, &o).await.unwrap();
    assert!(find(&p, "mesmo").action.needs_hash());
    assert!(!find(&p, "difere").action.needs_hash());
    assert_eq!(p.summary().needs_hash, 1);

    struct H(Vec<u8>);
    impl ContentHasher for H {
        async fn hash(&self, _: &RelPath) -> Result<Vec<u8>, TreeError> {
            Ok(self.0.clone())
        }
    }
    assert_eq!(p.resolve_hashes(&H(vec![1]), &H(vec![1])).await.unwrap(), 1);
    assert_eq!(skip(&p, "mesmo"), SkipReason::Identical);
    assert_eq!(p.summary().needs_hash, 0);
}

#[tokio::test]
async fn limites_de_profundidade_e_entradas() {
    let mut s = MemTree::new();
    s.add_file("a/b/c/d/e.txt", 1, Some(1));
    let mut o = opts();
    o.max_depth = 2;
    let p = plan(&s, &MemTree::new(), &o).await.unwrap();
    assert!(is_create(&p, "a/b"));
    assert_eq!(skip(&p, "a/b/c"), SkipReason::MaxDepth);
    assert!(p.items.iter().all(|i| i.path.as_str() != "a/b/c/d"));

    let mut s = MemTree::new();
    for i in 0..10 {
        s.add_file(&format!("f{i}"), 1, Some(1));
    }
    let mut o = opts();
    o.max_entries = 5;
    assert!(matches!(
        plan(&s, &MemTree::new(), &o).await,
        Err(PlanError::TooManyEntries(5))
    ));
}

#[tokio::test]
async fn raizes_invalidas() {
    let mut s = MemTree::new();
    s.add_file("f", 1, Some(1));
    // raiz da origem inexistente não existe em MemTree; simula via arquivo no destino
    let d = MemTree::new();
    assert!(plan(&s, &d, &opts()).await.is_ok());
}

#[tokio::test]
async fn revalidate_detecta_alteracoes() {
    let mut s = MemTree::new();
    let mut d = MemTree::new();
    s.add_file("novo", 5, Some(100));
    s.add_file("upd", 5, Some(200));
    d.add_file("upd", 5, Some(100));
    s.add_file("estavel", 5, Some(100));
    d.add_file("estavel", 5, Some(100));
    d.add_file("velho", 3, Some(1));
    let mut o = opts();
    o.mirror_deletes = true;
    let p = plan(&s, &d, &o).await.unwrap();
    assert!(p.revalidate(&s, &d).await.unwrap().is_empty());

    d.add_file("novo", 1, Some(1)); // apareceu no destino
    d.add_file("upd", 5, Some(150)); // destino mudou
    s.add_file("upd", 6, Some(200)); // origem mudou
    d.remove("velho");
    let ch = p.revalidate(&s, &d).await.unwrap();
    let has = |path: &str, side: Side| ch.iter().any(|c| c.path.as_str() == path && c.side == side);
    assert!(has("novo", Side::Dest));
    assert!(has("upd", Side::Dest));
    assert!(has("upd", Side::Source));
    assert!(has("velho", Side::Dest));
    assert!(ch.iter().all(|c| c.path.as_str() != "estavel"));
}

#[tokio::test]
async fn journal_roundtrip_e_retomada() {
    let mut s = MemTree::new();
    s.add_file("a b/c.txt", 1, Some(1));
    s.add_symlink("l");
    let mut d = MemTree::new();
    d.add_file("sobra", 1, Some(1));
    let mut o = opts();
    o.mirror_deletes = true;
    let p = plan(&s, &d, &o).await.unwrap();
    let mut j = Journal::from_plan(&p);
    assert_eq!(j.items.len(), 3); // 2 creates + 1 delete; symlink fora
    assert!(!j.is_complete());
    let cp = RelPath::parse("a b/c.txt").unwrap();
    assert!(j.mark(&cp, Op::Create, Status::Done));
    assert!(j.mark(
        &RelPath::parse("sobra").unwrap(),
        Op::Delete,
        Status::Failed("permissão negada".into())
    ));
    let json = j.to_json().unwrap();
    assert!(!json.contains('/') || json.contains("a b/c.txt"));
    let mut j2 = Journal::from_json(&json).unwrap();
    assert_eq!(j2.status(&cp, Op::Create), Some(&Status::Done));
    let rem: Vec<_> = j2.remaining().map(|i| i.path.to_string()).collect();
    assert_eq!(rem, vec!["a b", "sobra"]);
    assert!(j2.mark(&RelPath::parse("a b").unwrap(), Op::Create, Status::Skipped));
    assert!(j2.mark(&RelPath::parse("sobra").unwrap(), Op::Delete, Status::Done));
    assert!(j2.is_complete());
    assert!(Journal::from_json("{\"items\":[{\"path\":\"../x\",\"kind\":\"File\",\"op\":\"Create\",\"status\":\"Pending\"}]}").is_err());
}

#[tokio::test]
async fn plano_serializa() {
    let mut s = MemTree::new();
    s.add_file("f", 1, Some(1));
    let p = plan(&s, &MemTree::new(), &opts()).await.unwrap();
    let j = serde_json::to_string(&p).unwrap();
    let back: Plan = serde_json::from_str(&j).unwrap();
    assert_eq!(back.items, p.items);
}

// ------------------------------------------------------------ LocalTree

#[tokio::test]
async fn local_tree_lista_e_nao_segue_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path();
    std::fs::write(r.join("a b.txt"), b"hello").unwrap();
    std::fs::create_dir_all(r.join("sub/çã")).unwrap();
    std::fs::write(r.join("sub/çã/x"), b"1").unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(r.join("sub"), r.join("lnk")).unwrap();
        std::os::unix::fs::symlink("/nonexistent", r.join("quebrado")).unwrap();
    }
    let t = LocalTree::new(r);
    let mut l = t.list_dir(&RelPath::root()).await.unwrap();
    l.sort_by(|a, b| a.name.cmp(&b.name));
    let f = l.iter().find(|e| e.name == "a b.txt").unwrap();
    assert_eq!((f.kind, f.size), (EntryKind::File, 5));
    assert!(f.mtime.is_some());
    #[cfg(unix)]
    {
        assert_eq!(
            l.iter().find(|e| e.name == "lnk").unwrap().kind,
            EntryKind::Symlink
        );
        assert_eq!(
            t.stat(&RelPath::parse("quebrado").unwrap())
                .await
                .unwrap()
                .unwrap()
                .kind,
            EntryKind::Symlink
        );
        // Não atravessa o link.
        assert!(
            t.stat(&RelPath::parse("lnk/çã").unwrap())
                .await
                .unwrap()
                .is_some()
        );
    }
    assert!(
        t.stat(&RelPath::parse("nada").unwrap())
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        t.list_dir(&RelPath::parse("a b.txt").unwrap())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn plano_local_para_local() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    std::fs::write(a.path().join("novo.txt"), b"abc").unwrap();
    std::fs::create_dir(a.path().join(".git")).unwrap();
    std::fs::write(a.path().join(".git/cfg"), b"x").unwrap();
    std::fs::create_dir(a.path().join("d")).unwrap();
    std::fs::write(a.path().join("d/f"), b"12").unwrap();
    std::fs::write(b.path().join("sobra"), b"zz").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(a.path(), a.path().join("ciclo")).unwrap();

    let src = LocalTree::new(a.path());
    let dst = LocalTree::new(b.path());
    let p = plan(&src, &dst, &opts()).await.unwrap();
    assert!(is_create(&p, "novo.txt"));
    assert!(is_create(&p, "d/f"));
    assert!(p.items.iter().all(|i| !i.path.as_str().starts_with(".git")));
    assert!(p.items.iter().all(|i| i.action != Action::Delete));
    #[cfg(unix)]
    assert_eq!(skip(&p, "ciclo"), SkipReason::Symlink);

    let mut o = opts();
    o.mirror_deletes = true;
    let p = plan(&src, &dst, &o).await.unwrap();
    assert_eq!(find(&p, "sobra").action, Action::Delete);
    assert!(p.revalidate(&src, &dst).await.unwrap().is_empty());

    std::fs::write(b.path().join("sobra"), b"zzzzzz").unwrap();
    let ch = p.revalidate(&src, &dst).await.unwrap();
    assert_eq!(ch.len(), 1);
    assert_eq!(ch[0].side, Side::Dest);
}

#[tokio::test]
async fn origem_inexistente_e_destino_arquivo() {
    let a = tempfile::tempdir().unwrap();
    let gone = LocalTree::new(a.path().join("nao_existe"));
    let ok = LocalTree::new(a.path());
    assert!(matches!(
        plan(&gone, &ok, &opts()).await,
        Err(PlanError::SourceMissing)
    ));
    std::fs::write(a.path().join("f"), b"x").unwrap();
    let file_root = LocalTree::new(a.path().join("f"));
    assert!(matches!(
        plan(&ok, &file_root, &opts()).await,
        Err(PlanError::DestNotDir)
    ));
    assert!(matches!(
        plan(&file_root, &ok, &opts()).await,
        Err(PlanError::SourceNotDir)
    ));
    // Destino inexistente = vazio.
    assert!(plan(&ok, &gone, &opts()).await.unwrap().items.len() == 1);
}
