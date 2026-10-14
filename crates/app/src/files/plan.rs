//! Montagem de pedidos de transferência e política de conflito (lógica pura).

use std::path::{Path, PathBuf};

use super::contract::*;
use super::model::Entry;
use crate::i18n;

pub fn overwrite_key(o: Overwrite) -> &'static str {
    match o {
        Overwrite::Ask => "ask",
        Overwrite::Overwrite => "overwrite",
        Overwrite::Skip => "skip",
        Overwrite::Rename => "rename",
    }
}

pub fn parse_overwrite(s: &str) -> Option<Overwrite> {
    match s {
        "ask" => Some(Overwrite::Ask),
        "overwrite" => Some(Overwrite::Overwrite),
        "skip" => Some(Overwrite::Skip),
        "rename" => Some(Overwrite::Rename),
        _ => None,
    }
}

pub fn overwrite_label(o: Overwrite) -> &'static str {
    match o {
        Overwrite::Ask => i18n::FILES_POLICY_ASK,
        Overwrite::Overwrite => i18n::FILES_POLICY_OVERWRITE,
        Overwrite::Skip => i18n::FILES_POLICY_SKIP,
        Overwrite::Rename => i18n::FILES_POLICY_RENAME,
    }
}

pub const POLICIES: [Overwrite; 4] = [
    Overwrite::Ask,
    Overwrite::Overwrite,
    Overwrite::Skip,
    Overwrite::Rename,
];

fn request(direction: Direction, local: PathBuf, remote: String, o: Overwrite) -> TransferRequest {
    TransferRequest {
        direction,
        local,
        remote,
        overwrite: o,
        preserve_mtime: true,
        preserve_perms: false,
    }
}

/// Um pedido por item selecionado. Upload: `local` = o item, `remote` = diretório de destino.
/// Download: `remote` = o item, `local` = diretório de destino (ver contrato). Itens que não são
/// arquivo/pasta/link (sockets, dispositivos) ficam de fora.
pub fn build_requests(
    direction: Direction,
    selected: &[&Entry],
    local_dir: &Path,
    remote_dir: &str,
    overwrite: Overwrite,
) -> Vec<TransferRequest> {
    selected
        .iter()
        .filter(|e| e.kind != Kind::Other)
        .map(|e| match direction {
            Direction::Upload => request(
                direction,
                PathBuf::from(&e.path),
                remote_dir.to_string(),
                overwrite,
            ),
            Direction::Download => request(
                direction,
                local_dir.to_path_buf(),
                e.path.clone(),
                overwrite,
            ),
        })
        .collect()
}

/// Arquivos soltos do SO sobre a coluna remota: upload de cada caminho.
pub fn requests_for_drop(
    paths: &[PathBuf],
    remote_dir: &str,
    overwrite: Overwrite,
) -> Vec<TransferRequest> {
    paths
        .iter()
        .map(|p| {
            request(
                Direction::Upload,
                p.clone(),
                remote_dir.to_string(),
                overwrite,
            )
        })
        .collect()
}

/// Escolha do usuário no diálogo de conflito.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictChoice {
    Overwrite,
    Skip,
    Rename,
    Cancel,
}

/// Traduz a escolha em decisão do contrato. "Aplicar a todos" existe no contrato só para
/// sobrescrever/pular; para renomear a UI lembra a escolha e responde `Rename` sozinha
/// (`sticky = true`).
pub fn resolve_conflict(choice: ConflictChoice, apply_all: bool) -> (ConflictDecision, bool) {
    match (choice, apply_all) {
        (ConflictChoice::Overwrite, true) => (ConflictDecision::OverwriteAll, false),
        (ConflictChoice::Overwrite, false) => (ConflictDecision::Overwrite, false),
        (ConflictChoice::Skip, true) => (ConflictDecision::SkipAll, false),
        (ConflictChoice::Skip, false) => (ConflictDecision::Skip, false),
        (ConflictChoice::Rename, all) => (ConflictDecision::Rename, all),
        (ConflictChoice::Cancel, _) => (ConflictDecision::Cancel, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(name: &str, kind: Kind, path: &str) -> Entry {
        Entry {
            name: name.into(),
            path: path.into(),
            kind,
            size: 1,
            mtime: None,
        }
    }

    #[test]
    fn upload_um_pedido_por_item_com_destino_pai() {
        let a = ent("a.txt", Kind::File, "/tmp/work/a.txt");
        let d = ent("dados", Kind::Dir, "/tmp/work/dados");
        let s = ent("sock", Kind::Other, "/tmp/work/sock");
        let r = build_requests(
            Direction::Upload,
            &[&a, &d, &s],
            Path::new("/tmp/work"),
            "/srv/app",
            Overwrite::Rename,
        );
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].direction, Direction::Upload);
        assert_eq!(r[0].local, PathBuf::from("/tmp/work/a.txt"));
        assert_eq!(r[0].remote, "/srv/app");
        assert_eq!(r[1].local, PathBuf::from("/tmp/work/dados"));
        assert!(r.iter().all(|x| x.overwrite == Overwrite::Rename));
        assert!(r.iter().all(|x| x.preserve_mtime && !x.preserve_perms));
    }

    #[test]
    fn download_usa_remoto_como_origem_e_local_como_destino_pai() {
        let a = ent("log", Kind::File, "/var/log/log");
        let r = build_requests(
            Direction::Download,
            &[&a],
            Path::new("/home/ana/Downloads"),
            "/var/log",
            Overwrite::Ask,
        );
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].remote, "/var/log/log");
        assert_eq!(r[0].local, PathBuf::from("/home/ana/Downloads"));
        assert_eq!(r[0].overwrite, Overwrite::Ask);
    }

    #[test]
    fn nomes_com_espaco_e_unicode_passam_intactos() {
        let a = ent("relatório final.pdf", Kind::File, "/r/relatório final.pdf");
        let r = build_requests(
            Direction::Download,
            &[&a],
            Path::new("/d"),
            "/r",
            Overwrite::Ask,
        );
        assert_eq!(r[0].remote, "/r/relatório final.pdf");
    }

    #[test]
    fn soltar_do_so_vira_upload() {
        let r = requests_for_drop(
            &[PathBuf::from("/x/a"), PathBuf::from("/x/b")],
            "/srv",
            Overwrite::Skip,
        );
        assert_eq!(r.len(), 2);
        assert!(
            r.iter()
                .all(|x| x.direction == Direction::Upload && x.remote == "/srv")
        );
    }

    #[test]
    fn politica_serializa_e_ignora_lixo() {
        for p in POLICIES {
            assert_eq!(parse_overwrite(overwrite_key(p)), Some(p));
        }
        assert_eq!(parse_overwrite("talvez"), None);
    }

    #[test]
    fn conflito_aplicar_a_todos() {
        use ConflictChoice as C;
        assert_eq!(
            resolve_conflict(C::Overwrite, false),
            (ConflictDecision::Overwrite, false)
        );
        assert_eq!(
            resolve_conflict(C::Overwrite, true),
            (ConflictDecision::OverwriteAll, false)
        );
        assert_eq!(
            resolve_conflict(C::Skip, true),
            (ConflictDecision::SkipAll, false)
        );
        assert_eq!(
            resolve_conflict(C::Skip, false),
            (ConflictDecision::Skip, false)
        );
        // renomear em todos: a UI memoriza
        assert_eq!(
            resolve_conflict(C::Rename, true),
            (ConflictDecision::Rename, true)
        );
        assert_eq!(
            resolve_conflict(C::Rename, false),
            (ConflictDecision::Rename, false)
        );
        // cancelar nunca é "para todos"
        assert_eq!(
            resolve_conflict(C::Cancel, true),
            (ConflictDecision::Cancel, false)
        );
    }
}
