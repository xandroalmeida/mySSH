//! Idioma da interface do mySSH: inglês, português (Brasil) e espanhol.
//!
//! O idioma ativo é global ao processo ([`set_lang`]); o app o define na inicialização a
//! partir da preferência salva ou, em "Automático", do idioma do sistema ([`detect_system`]),
//! que cai em inglês quando não é um dos suportados. Textos são escolhidos com [`tr!`], que só
//! avalia o ramo do idioma ativo (pode envolver `format!`).
//!
//! Antes de [`set_lang`] o idioma é pt-BR, o idioma-fonte dos textos: assim testes que
//! conferem mensagens literais não dependem do locale da máquina. Testes que precisam de outro
//! idioma usam [`with_lang`], que vale só para a thread atual.

use std::cell::Cell;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    En,
    PtBr,
    Es,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::En, Lang::PtBr, Lang::Es];

    /// Chave estável para persistência.
    pub fn key(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::PtBr => "pt-BR",
            Lang::Es => "es",
        }
    }

    pub fn parse(s: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.key() == s)
    }

    /// Nome do idioma escrito nele mesmo (para o seletor).
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::PtBr => "Português (Brasil)",
            Lang::Es => "Español",
        }
    }

    /// Idioma correspondente a uma tag BCP 47 / POSIX (`pt-BR`, `pt_PT.UTF-8`, `es-419`…).
    /// Qualquer variante de português usa pt-BR; de espanhol, es.
    pub fn from_locale(tag: &str) -> Option<Lang> {
        let primary = tag
            .split(['-', '_', '.', '@'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match primary.as_str() {
            "en" => Some(Lang::En),
            "pt" => Some(Lang::PtBr),
            "es" => Some(Lang::Es),
            _ => None,
        }
    }

    fn to_u8(self) -> u8 {
        match self {
            Lang::En => 0,
            Lang::PtBr => 1,
            Lang::Es => 2,
        }
    }

    fn from_u8(v: u8) -> Lang {
        match v {
            0 => Lang::En,
            2 => Lang::Es,
            _ => Lang::PtBr,
        }
    }
}

/// Primeiro idioma suportado na lista de preferências do sistema; inglês se nenhum for.
pub fn detect_system() -> Lang {
    resolve(sys_locale::get_locales())
}

fn resolve(tags: impl IntoIterator<Item = String>) -> Lang {
    tags.into_iter()
        .find_map(|t| Lang::from_locale(&t))
        .unwrap_or(Lang::En)
}

static CURRENT: AtomicU8 = AtomicU8::new(1); // pt-BR até `set_lang`

thread_local! {
    static OVERRIDE: Cell<Option<Lang>> = const { Cell::new(None) };
}

pub fn set_lang(lang: Lang) {
    CURRENT.store(lang.to_u8(), Ordering::Relaxed);
}

/// Idioma ativo nesta thread.
pub fn lang() -> Lang {
    OVERRIDE
        .with(Cell::get)
        .unwrap_or_else(|| Lang::from_u8(CURRENT.load(Ordering::Relaxed)))
}

/// Executa `f` com `lang` ativo só na thread atual (testes).
pub fn with_lang<R>(lang: Lang, f: impl FnOnce() -> R) -> R {
    let prev = OVERRIDE.with(|o| o.replace(Some(lang)));
    struct Restore(Option<Lang>);
    impl Drop for Restore {
        fn drop(&mut self) {
            OVERRIDE.with(|o| o.set(self.0));
        }
    }
    let _restore = Restore(prev);
    f()
}

/// Escolhe o texto do idioma ativo: `tr!(pt_br, en, es)`. Só o ramo escolhido é avaliado.
#[macro_export]
macro_rules! tr {
    ($pt:expr, $en:expr, $es:expr $(,)?) => {
        match $crate::lang() {
            $crate::Lang::PtBr => $pt,
            $crate::Lang::En => $en,
            $crate::Lang::Es => $es,
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_mapeia_variantes_e_cai_em_ingles() {
        assert_eq!(Lang::from_locale("pt-BR"), Some(Lang::PtBr));
        assert_eq!(Lang::from_locale("pt_PT.UTF-8"), Some(Lang::PtBr));
        assert_eq!(Lang::from_locale("es-419"), Some(Lang::Es));
        assert_eq!(Lang::from_locale("en_US"), Some(Lang::En));
        assert_eq!(Lang::from_locale("de-DE"), None);
        assert_eq!(resolve(["de-DE".into(), "es-MX".into()]), Lang::Es);
        assert_eq!(resolve(["fr-FR".into()]), Lang::En);
        assert_eq!(resolve(Vec::new()), Lang::En);
    }

    #[test]
    fn chaves_ida_e_volta() {
        for l in Lang::ALL {
            assert_eq!(Lang::parse(l.key()), Some(l));
            assert_eq!(Lang::from_u8(l.to_u8()), l);
        }
        assert_eq!(Lang::parse("fr"), None);
    }

    #[test]
    fn with_lang_vale_so_na_thread_e_restaura() {
        let antes = lang();
        let dentro = with_lang(Lang::Es, || {
            let outra = std::thread::spawn(lang).join().unwrap();
            (tr!("Salvar", "Save", "Guardar"), outra)
        });
        assert_eq!(dentro.0, "Guardar");
        assert_eq!(dentro.1, Lang::from_u8(CURRENT.load(Ordering::Relaxed)));
        assert_eq!(lang(), antes);
    }
}
