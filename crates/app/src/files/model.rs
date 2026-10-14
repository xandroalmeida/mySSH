//! Modelo puro (sem egui): entradas, ordenação, seleção, caminhos e nomes.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::time::SystemTime;

use super::contract::{Kind, RemoteEntry};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Local,
    Remote,
}

impl Side {
    pub fn other(self) -> Self {
        match self {
            Self::Local => Self::Remote,
            Self::Remote => Self::Local,
        }
    }
}

/// Item de uma listagem (local ou remota).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    /// Caminho completo (no remoto, separador `/`).
    pub path: String,
    pub kind: Kind,
    pub size: u64,
    pub mtime: Option<SystemTime>,
}

impl Entry {
    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Dir
    }

    pub fn hidden(&self) -> bool {
        self.name.starts_with('.')
    }
}

impl From<RemoteEntry> for Entry {
    fn from(e: RemoteEntry) -> Self {
        Self {
            name: e.name,
            path: e.path,
            kind: e.kind,
            size: e.size,
            mtime: e.mtime,
        }
    }
}

// ------------------------------------------------------------------------------ ordenação

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Size,
    Modified,
    Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    pub key: SortKey,
    pub asc: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Self {
            key: SortKey::Name,
            asc: true,
        }
    }
}

impl Sort {
    /// Clique no cabeçalho: mesma coluna inverte; outra coluna começa ascendente.
    pub fn toggle(&mut self, key: SortKey) {
        if self.key == key {
            self.asc = !self.asc;
        } else {
            *self = Self { key, asc: true };
        }
    }
}

fn kind_rank(k: Kind) -> u8 {
    match k {
        Kind::Dir => 0,
        Kind::Symlink => 1,
        Kind::File => 2,
        Kind::Other => 3,
    }
}

fn name_cmp(a: &str, b: &str) -> Ordering {
    a.to_lowercase().cmp(&b.to_lowercase()).then(a.cmp(b))
}

/// Diretórios sempre primeiro; dentro de cada grupo, pela coluna escolhida (desempate pelo nome).
pub fn sort_entries(v: &mut [Entry], s: Sort) {
    v.sort_by(|a, b| {
        let dirs = b.is_dir().cmp(&a.is_dir());
        if dirs != Ordering::Equal {
            return dirs;
        }
        let o = match s.key {
            SortKey::Name => name_cmp(&a.name, &b.name),
            SortKey::Size => a.size.cmp(&b.size).then_with(|| name_cmp(&a.name, &b.name)),
            SortKey::Modified => a
                .mtime
                .cmp(&b.mtime)
                .then_with(|| name_cmp(&a.name, &b.name)),
            SortKey::Kind => kind_rank(a.kind)
                .cmp(&kind_rank(b.kind))
                .then_with(|| name_cmp(&a.name, &b.name)),
        };
        if s.asc { o } else { o.reverse() }
    });
}

/// Índices visíveis (oculta os que começam com `.` quando `show_hidden` é falso).
pub fn visible_indices(v: &[Entry], show_hidden: bool) -> Vec<usize> {
    v.iter()
        .enumerate()
        .filter(|(_, e)| show_hidden || !e.hidden())
        .map(|(i, _)| i)
        .collect()
}

// ------------------------------------------------------------------------------- seleção

/// Seleção por nome (sobrevive a reordenar/atualizar). `cursor`/`anchor` são índices na lista
/// visível.
#[derive(Debug, Default, Clone)]
pub struct Selection {
    pub names: BTreeSet<String>,
    pub anchor: Option<usize>,
    pub cursor: Option<usize>,
}

impl Selection {
    pub fn clear(&mut self) {
        self.names.clear();
        self.anchor = None;
        self.cursor = None;
    }

    pub fn contains(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    /// Clique: simples seleciona só o item; `toggle` (Cmd/Ctrl) alterna; `range` (Shift)
    /// seleciona do âncora até o item.
    pub fn click(&mut self, idx: usize, names: &[&str], toggle: bool, range: bool) {
        let Some(name) = names.get(idx) else { return };
        if range {
            let a = self.anchor.unwrap_or(idx).min(names.len() - 1);
            let (lo, hi) = (a.min(idx), a.max(idx));
            if !toggle {
                self.names.clear();
            }
            for n in &names[lo..=hi] {
                self.names.insert((*n).to_string());
            }
        } else if toggle {
            if !self.names.remove(*name) {
                self.names.insert((*name).to_string());
            }
            self.anchor = Some(idx);
        } else {
            self.names.clear();
            self.names.insert((*name).to_string());
            self.anchor = Some(idx);
        }
        self.cursor = Some(idx);
    }

    /// Setas: move o cursor; com `extend` (Shift) amplia a seleção desde o âncora.
    pub fn move_cursor(&mut self, delta: isize, names: &[&str], extend: bool) {
        if names.is_empty() {
            return;
        }
        let cur = self
            .cursor
            .unwrap_or(if delta >= 0 { usize::MAX } else { 0 });
        let next = if cur == usize::MAX {
            0
        } else {
            cur.saturating_add_signed(delta).min(names.len() - 1)
        };
        self.go_to(next, names, extend);
    }

    pub fn go_to(&mut self, idx: usize, names: &[&str], extend: bool) {
        let idx = idx.min(names.len().saturating_sub(1));
        if extend {
            let a = self.anchor.unwrap_or(idx);
            self.names.clear();
            let (lo, hi) = (a.min(idx), a.max(idx));
            for n in &names[lo..=hi] {
                self.names.insert((*n).to_string());
            }
            self.anchor.get_or_insert(a);
        } else {
            self.names.clear();
            self.names.insert(names[idx].to_string());
            self.anchor = Some(idx);
        }
        self.cursor = Some(idx);
    }

    pub fn select_all(&mut self, names: &[&str]) {
        self.names = names.iter().map(|n| (*n).to_string()).collect();
    }

    /// Remove nomes que não existem mais.
    pub fn retain_existing(&mut self, names: &[&str]) {
        self.names.retain(|n| names.contains(&n.as_str()));
        self.cursor = self.cursor.filter(|c| *c < names.len());
        self.anchor = self.anchor.filter(|c| *c < names.len());
    }
}

// --------------------------------------------------------------------------------- caminhos

pub fn remote_parent(path: &str) -> String {
    let t = path.trim_end_matches('/');
    match t.rsplit_once('/') {
        Some(("", _)) | None => "/".into(),
        Some((p, _)) => p.into(),
    }
}

pub fn remote_join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// Valida o nome de um item novo/renomeado. `Err` = mensagem para o usuário.
pub fn validate_name(name: &str) -> Result<(), &'static str> {
    use crate::i18n;
    let n = name.trim();
    if n.is_empty() {
        return Err(i18n::FILES_NAME_EMPTY);
    }
    if n == "." || n == ".." || n.contains(['/', '\0']) || (cfg!(windows) && n.contains('\\')) {
        return Err(i18n::FILES_NAME_INVALID);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, kind: Kind, size: u64, secs: u64) -> Entry {
        Entry {
            name: name.into(),
            path: format!("/x/{name}"),
            kind,
            size,
            mtime: Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs)),
        }
    }

    fn sample() -> Vec<Entry> {
        vec![
            e("b.txt", Kind::File, 300, 20),
            e("Zeta", Kind::Dir, 0, 5),
            e("a.log", Kind::File, 100, 30),
            e("alfa", Kind::Dir, 0, 50),
            e("link", Kind::Symlink, 1, 10),
        ]
    }

    fn names(v: &[Entry]) -> Vec<&str> {
        v.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn ordena_por_nome_com_pastas_primeiro() {
        let mut v = sample();
        sort_entries(&mut v, Sort::default());
        assert_eq!(names(&v), ["alfa", "Zeta", "a.log", "b.txt", "link"]);
        sort_entries(
            &mut v,
            Sort {
                key: SortKey::Name,
                asc: false,
            },
        );
        // pastas continuam primeiro mesmo em ordem descendente
        assert_eq!(names(&v), ["Zeta", "alfa", "link", "b.txt", "a.log"]);
    }

    #[test]
    fn ordena_por_tamanho_data_e_tipo() {
        let mut v = sample();
        sort_entries(
            &mut v,
            Sort {
                key: SortKey::Size,
                asc: false,
            },
        );
        assert_eq!(names(&v)[2..], ["b.txt", "a.log", "link"]);
        sort_entries(
            &mut v,
            Sort {
                key: SortKey::Modified,
                asc: true,
            },
        );
        assert_eq!(names(&v), ["Zeta", "alfa", "link", "b.txt", "a.log"]);
        sort_entries(
            &mut v,
            Sort {
                key: SortKey::Kind,
                asc: true,
            },
        );
        assert_eq!(names(&v), ["alfa", "Zeta", "link", "a.log", "b.txt"]);
    }

    #[test]
    fn cabecalho_alterna_e_reinicia() {
        let mut s = Sort::default();
        s.toggle(SortKey::Name);
        assert!(!s.asc);
        s.toggle(SortKey::Size);
        assert_eq!(
            s,
            Sort {
                key: SortKey::Size,
                asc: true
            }
        );
    }

    #[test]
    fn oculta_arquivos_com_ponto() {
        let v = vec![
            e(".git", Kind::Dir, 0, 1),
            e("a", Kind::File, 1, 1),
            e(".env", Kind::File, 1, 1),
        ];
        assert_eq!(visible_indices(&v, false), vec![1]);
        assert_eq!(visible_indices(&v, true), vec![0, 1, 2]);
    }

    #[test]
    fn selecao_simples_toggle_e_intervalo() {
        let names = ["a", "b", "c", "d", "e"];
        let mut s = Selection::default();
        s.click(1, &names, false, false);
        assert_eq!(s.names.iter().collect::<Vec<_>>(), ["b"]);
        s.click(3, &names, true, false);
        assert_eq!(s.names.iter().collect::<Vec<_>>(), ["b", "d"]);
        s.click(1, &names, true, false);
        assert_eq!(s.names.iter().collect::<Vec<_>>(), ["d"]);
        // Shift: do âncora (1, último clique) até 3
        s.click(3, &names, false, true);
        assert_eq!(s.names.iter().collect::<Vec<_>>(), ["b", "c", "d"]);
        // Shift+Cmd soma ao que já estava selecionado
        s.click(4, &names, true, true);
        assert_eq!(s.names.len(), 4);
        assert!(s.contains("e") && s.contains("b"));
        // clique simples recolhe tudo
        s.click(4, &names, false, false);
        assert_eq!(s.names.len(), 1);
    }

    #[test]
    fn setas_movem_e_shift_estende() {
        let names = ["a", "b", "c", "d"];
        let mut s = Selection::default();
        s.move_cursor(1, &names, false);
        assert_eq!(s.cursor, Some(0));
        s.move_cursor(1, &names, false);
        assert_eq!(s.cursor, Some(1));
        s.move_cursor(1, &names, true);
        s.move_cursor(1, &names, true);
        assert_eq!(s.names.iter().collect::<Vec<_>>(), ["b", "c", "d"]);
        // limite
        s.move_cursor(5, &names, false);
        assert_eq!(s.cursor, Some(3));
        s.move_cursor(-9, &names, false);
        assert_eq!(s.cursor, Some(0));
        assert_eq!(s.names.len(), 1);
    }

    #[test]
    fn retem_somente_nomes_existentes() {
        let mut s = Selection::default();
        s.select_all(&["a", "b", "c"]);
        s.cursor = Some(2);
        s.retain_existing(&["a", "c"]);
        assert_eq!(s.names.len(), 2);
        s.retain_existing(&["a"]);
        assert_eq!(s.cursor, None);
    }

    #[test]
    fn caminhos_remotos() {
        assert_eq!(remote_parent("/home/ana/x"), "/home/ana");
        assert_eq!(remote_parent("/home"), "/");
        assert_eq!(remote_parent("/"), "/");
        assert_eq!(remote_join("/", "a"), "/a");
        assert_eq!(remote_join("/home/ana", "a b"), "/home/ana/a b");
    }

    #[test]
    fn valida_nomes() {
        assert!(validate_name("pasta nova").is_ok());
        assert!(validate_name("ação-ü.txt").is_ok());
        for bad in ["", "  ", ".", "..", "a/b", "a\0b"] {
            assert!(validate_name(bad).is_err(), "{bad:?}");
        }
    }
}
