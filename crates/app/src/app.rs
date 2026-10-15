//! Estado da aplicação e operações (perfis, credenciais, abas). A UI fica em `view`/`dialogs`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use myssh_core::trust::{HostEndpoint, HostKey, HostTrustStore};
use myssh_core::{Profile, ProfileId};
use myssh_secrets::{Availability, MemoryStore, SecretError, SecretRef, Vault};
use myssh_store::{Folder, ProfileFilter, Store};

use crate::demo::Demo;
use crate::form::{Draft, ProfileForm};
use crate::i18n;
use crate::logic::{
    ViewFilter, credential_ids, duplicate_profile, parse_quick, quick_profile, release_credential,
};
use crate::settings::Settings;
use crate::shortcuts::{self, Action};
use crate::split::{Dir, PaneId};
use crate::split_body::{MAX_PANES, SplitBody, refresh_kind_theme};
use crate::ssh_import::{self, Preview, SaveReport};
use crate::suspend::SuspendDetector;
use crate::tabs::{Tab, TabBody, TabEnv, TabEvent, TabKind, make_body};

/// Mensagens de tarefas em segundo plano (keyring, diálogo de arquivo) para a thread de UI.
pub enum Msg {
    Availability(Availability),
    SecretPut {
        op: u64,
        result: Result<(), SecretError>,
    },
    Unlocked(Result<(), SecretError>),
    KeyPicked(PathBuf),
    /// Arquivo ssh config lido e interpretado (ou erro em pt-BR).
    SshConfigLoaded(Result<Preview, String>),
    SshConfigSaved(Result<SaveReport, String>),
    /// Resultado da exportação da configuração (texto do aviso; vazio = cancelado).
    ConfigExported(Result<String, String>),
    ConfigImported(Result<myssh_store::ImportReport, String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyringStatus {
    Checking,
    Available,
    Unavailable(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultMode {
    /// Cofre do sistema (Keychain / Credential Manager / Secret Service).
    Os,
    /// Somente memória, escolhido explicitamente (ou demo): some ao fechar o app.
    SessionOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultIssue {
    Unavailable,
    Locked,
}

pub struct ProfileDialog {
    pub form: ProfileForm,
    pub error: Option<String>,
    pub saving: bool,
    pub issue: Option<VaultIssue>,
}

impl ProfileDialog {
    pub fn new(form: ProfileForm) -> Self {
        Self {
            form,
            error: None,
            saving: false,
            issue: None,
        }
    }
}

pub struct QuickDialog {
    pub text: String,
    pub error: Option<String>,
}

pub enum Dialog {
    None,
    Profile(Box<ProfileDialog>),
    Settings,
    Quick(QuickDialog),
    ConfirmDelete(ProfileId),
    ConfirmCloseTab(u64),
    ConfirmQuit,
    /// Esquecer a chave confiável de um host (volta às Configurações depois).
    ConfirmForget(HostEndpoint),
    /// Importação de `~/.ssh/config` (escolha, prévia e gravação).
    ImportSsh(Box<ImportDialog>),
    /// Escolha do perfil da nova sessão ao dividir.
    Split(SplitDialog),
    ConfirmClosePane {
        tab: u64,
        pane: PaneId,
    },
    ConfirmEndAll,
    /// Janela "Sobre" (informações de versão, créditos, licenças).
    About(crate::about::AboutState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportStage {
    Intro,
    Loading,
    Preview,
    Saving,
}

pub struct ImportDialog {
    pub stage: ImportStage,
    pub preview: Preview,
    pub error: Option<String>,
}

impl ImportDialog {
    pub fn new() -> Self {
        Self {
            stage: ImportStage::Intro,
            preview: Preview::default(),
            error: None,
        }
    }
}

pub struct SplitDialog {
    pub dir: Dir,
    pub choice: Option<ProfileId>,
}

pub struct Pending {
    pub op: u64,
    pub draft: Draft,
    pub editing: bool,
}

pub struct App {
    pub(crate) rt: Arc<tokio::runtime::Runtime>,
    pub(crate) store: Arc<Store>,
    pub(crate) vault: Arc<Vault>,
    pub(crate) vault_mode: VaultMode,
    pub(crate) keyring: KeyringStatus,
    pub(crate) keyring_check_started: bool,
    /// Aviso persistente (ex.: banco em memória por falha ao abrir o arquivo).
    pub(crate) store_warning: Option<String>,
    pub(crate) tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub(crate) settings: Settings,
    saved_settings: Settings,
    applied_theme: Option<crate::settings::ThemePref>,
    pub(crate) profiles: Vec<Profile>,
    pub(crate) folders: Vec<Folder>,
    pub(crate) filter: ViewFilter,
    pub(crate) selected: Option<ProfileId>,
    pub(crate) focus_search: bool,
    pub(crate) tabs: Vec<Tab>,
    pub(crate) active: Option<u64>,
    next_tab: u64,
    pub(crate) dialog: Dialog,
    pub(crate) pending: Option<Pending>,
    next_op: u64,
    pub(crate) notice: Option<(String, Instant)>,
    quit_ok: bool,
    /// Abas de terminal conectam sozinhas ao abrir (desligado em demos/testes sem rede).
    pub(crate) auto_connect: bool,
    /// Diretório de dados (journal da sincronização); `None` em testes.
    pub data_dir: Option<std::path::PathBuf>,
    /// Hosts confiáveis (cache; atualizado ao abrir as Configurações).
    pub(crate) trusted: Vec<(HostEndpoint, HostKey)>,
    pub(crate) trusted_error: Option<String>,
    pub(crate) scroll_to_trusted: bool,
    pub(crate) demo: Option<Demo>,
    suspend: SuspendDetector,
    /// Importar/exportar configuração em andamento (evita cliques repetidos).
    pub(crate) config_busy: bool,
}

impl App {
    pub fn new(
        rt: Arc<tokio::runtime::Runtime>,
        store: Store,
        vault: Arc<Vault>,
        vault_mode: VaultMode,
        store_warning: Option<String>,
        demo: Option<Demo>,
    ) -> Self {
        let (tx, rx) = channel();
        let store = Arc::new(store);
        let settings = Settings::load(&store);
        vault.set_idle_timeout(settings.vault_timeout());
        let mut app = Self {
            rt,
            store,
            vault,
            vault_mode,
            keyring: if vault_mode == VaultMode::Os {
                KeyringStatus::Checking
            } else {
                KeyringStatus::Available
            },
            keyring_check_started: false,
            store_warning,
            tx,
            rx,
            saved_settings: settings.clone(),
            settings,
            applied_theme: None,
            profiles: Vec::new(),
            folders: Vec::new(),
            filter: ViewFilter::default(),
            selected: None,
            focus_search: false,
            tabs: Vec::new(),
            active: None,
            next_tab: 1,
            dialog: Dialog::None,
            pending: None,
            next_op: 1,
            notice: None,
            quit_ok: false,
            auto_connect: demo.as_ref().is_none_or(Demo::connects),
            data_dir: None,
            trusted: Vec::new(),
            trusted_error: None,
            scroll_to_trusted: false,
            demo,
            suspend: SuspendDetector::default(),
            config_busy: false,
        };
        app.reload();
        app.open_tab(TabKind::Welcome);
        app
    }

    // ------------------------------------------------------------------ dados

    pub fn reload(&mut self) {
        match self.store.list_profiles(&ProfileFilter::default()) {
            Ok(p) => self.profiles = p,
            Err(e) => self.set_notice(e.to_string()),
        }
        match self.store.list_folders() {
            Ok(f) => self.folders = f,
            Err(e) => self.set_notice(e.to_string()),
        }
        if self
            .selected
            .is_some_and(|s| !self.profiles.iter().any(|p| p.id == s))
        {
            self.selected = None;
        }
        // O tema editado vale também para as abas/painéis já abertos do perfil.
        for t in &mut self.tabs {
            refresh_kind_theme(&mut t.kind, &self.profiles);
            if let Some(sp) = t.body.as_split() {
                sp.refresh_themes(&self.profiles);
            }
        }
    }

    pub fn profile(&self, id: ProfileId) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn set_notice(&mut self, msg: impl Into<String>) {
        self.notice = Some((msg.into(), Instant::now()));
    }

    // -------------------------------------------------------- tarefas de fundo

    /// Verifica o cofre do SO fora da thread de UI.
    pub fn check_keyring(&mut self, ctx: &egui::Context) {
        self.keyring = KeyringStatus::Checking;
        let (vault, tx, ctx) = (self.vault.clone(), self.tx.clone(), ctx.clone());
        self.rt.spawn_blocking(move || {
            let _ = tx.send(Msg::Availability(vault.availability()));
            ctx.request_repaint();
        });
    }

    fn spawn_put(
        &self,
        ctx: &egui::Context,
        op: u64,
        key: SecretRef,
        secret: myssh_secrets::SecretString,
    ) {
        let (vault, tx, ctx) = (self.vault.clone(), self.tx.clone(), ctx.clone());
        self.rt.spawn_blocking(move || {
            let result = vault.put(&key, secret);
            let _ = tx.send(Msg::SecretPut { op, result });
            ctx.request_repaint();
        });
    }

    /// Apaga um segredo do cofre em segundo plano (erros ignorados: pode já não existir).
    pub fn purge_secret(&self, key: SecretRef) {
        let vault = self.vault.clone();
        self.rt.spawn_blocking(move || {
            let _ = vault.delete(&key);
        });
    }

    pub fn unlock_vault(&self, ctx: &egui::Context) {
        let (vault, tx, ctx) = (self.vault.clone(), self.tx.clone(), ctx.clone());
        self.rt.spawn_blocking(move || {
            let _ = tx.send(Msg::Unlocked(vault.unlock()));
            ctx.request_repaint();
        });
    }

    pub fn pick_key_file(&self, ctx: &egui::Context) {
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        let start = crate::paths::ssh_dir();
        self.rt.spawn(async move {
            let mut dlg = rfd::AsyncFileDialog::new().set_title(i18n::PICK_KEY_TITLE());
            if let Some(d) = start {
                dlg = dlg.set_directory(d);
            }
            if let Some(f) = dlg.pick_file().await {
                let _ = tx.send(Msg::KeyPicked(f.path().to_path_buf()));
                ctx.request_repaint();
            }
        });
    }

    /// Troca para cofre só em memória (ação explícita do usuário; nunca automática).
    pub fn use_session_vault(&mut self, ctx: &egui::Context) {
        let v = Arc::new(Vault::new(Arc::new(MemoryStore::new())));
        v.set_idle_timeout(self.settings.vault_timeout());
        self.vault = v;
        self.vault_mode = VaultMode::SessionOnly;
        self.keyring = KeyringStatus::Available;
        self.set_notice(i18n::SESSION_VAULT_ON());
        ctx.request_repaint();
    }

    pub fn drain_messages(&mut self, ctx: &egui::Context) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Availability(a) => {
                    self.keyring = match a {
                        Availability::Available => KeyringStatus::Available,
                        Availability::Locked => {
                            KeyringStatus::Unavailable(i18n::KEYRING_LOCKED().into())
                        }
                        Availability::Unavailable(r) => KeyringStatus::Unavailable(r),
                    }
                }
                Msg::Unlocked(Ok(())) => self.set_notice(i18n::VAULT_UNLOCKED()),
                Msg::Unlocked(Err(e)) => self.set_notice(secret_error_text(&e)),
                Msg::KeyPicked(path) => {
                    if let Dialog::Profile(d) = &mut self.dialog {
                        d.form.key_path = path.to_string_lossy().into_owned();
                    }
                }
                Msg::SecretPut { op, result } => self.on_secret_put(ctx, op, result),
                Msg::SshConfigLoaded(r) => self.on_import_loaded(r),
                Msg::SshConfigSaved(r) => self.on_import_saved(r),
                Msg::ConfigExported(r) => {
                    self.config_busy = false;
                    match r {
                        Ok(text) | Err(text) if !text.is_empty() => self.set_notice(text),
                        _ => {}
                    }
                }
                Msg::ConfigImported(r) => {
                    self.config_busy = false;
                    match r {
                        Ok(rep) => {
                            self.reload();
                            self.set_notice(i18n::cfg_imported(rep.profiles, rep.folders));
                        }
                        Err(e) if !e.is_empty() => self.set_notice(e),
                        Err(_) => {}
                    }
                }
            }
        }
    }

    fn on_secret_put(&mut self, _ctx: &egui::Context, op: u64, result: Result<(), SecretError>) {
        let Some(p) = self.pending.take_if(|p| p.op == op) else {
            return;
        };
        match result {
            Ok(()) => self.commit(p.draft, p.editing),
            Err(e) => {
                if let Dialog::Profile(d) = &mut self.dialog {
                    d.saving = false;
                    d.error = Some(secret_error_text(&e));
                    d.issue = match e {
                        SecretError::Unavailable(_) => Some(VaultIssue::Unavailable),
                        SecretError::Locked => Some(VaultIssue::Locked),
                        _ => None,
                    };
                }
            }
        }
    }

    // ---------------------------------------------------------------- perfis

    /// Valida o formulário do diálogo aberto e inicia o salvamento.
    pub fn submit_profile_dialog(&mut self, ctx: &egui::Context) {
        let Dialog::Profile(d) = &mut self.dialog else {
            return;
        };
        if d.saving {
            return;
        }
        d.error = None;
        d.issue = None;
        let editing = d.form.existing.is_some();
        let draft = match d.form.build(&self.folders) {
            Ok(x) => x,
            Err(e) => {
                d.error = Some(e.to_string());
                return;
            }
        };
        match draft.new_secret.clone() {
            None => self.commit(draft, editing),
            Some(ns) => {
                d.saving = true;
                let op = self.next_op;
                self.next_op += 1;
                self.spawn_put(ctx, op, ns.credential.secret.clone(), ns.secret.clone());
                self.pending = Some(Pending { op, draft, editing });
            }
        }
    }

    /// Grava no store (pasta, credencial, perfil), libera credenciais obsoletas e fecha o diálogo.
    fn commit(&mut self, draft: Draft, editing: bool) {
        let Draft {
            profile,
            new_secret,
            obsolete,
            new_folder,
        } = draft;
        let result = (|| -> Result<(), myssh_store::StoreError> {
            if let Some(f) = &new_folder {
                self.store.create_folder(f)?;
            }
            if let Some(ns) = &new_secret {
                self.store.create_credential(&ns.credential)?;
            }
            let r = if editing {
                self.store.update_profile(&profile)
            } else {
                self.store.create_profile(&profile)
            };
            if let (Err(_), Some(ns)) = (&r, &new_secret) {
                let _ = self.store.delete_credential(ns.credential.id);
            }
            r
        })();
        match result {
            Ok(()) => {
                for c in obsolete {
                    if let Some(r) = release_credential(&self.store, c) {
                        self.purge_secret(r);
                    }
                }
                self.selected = Some(profile.id);
                self.set_notice(i18n::profile_saved(&profile.name));
                self.close_dialog();
                self.reload();
            }
            Err(e) => {
                if let Some(ns) = new_secret {
                    self.purge_secret(ns.credential.secret);
                }
                if let Dialog::Profile(d) = &mut self.dialog {
                    d.saving = false;
                    d.error = Some(e.to_string());
                }
            }
        }
    }

    pub fn close_dialog(&mut self) {
        if let Dialog::Profile(d) = &mut self.dialog {
            d.form.wipe();
        }
        self.dialog = Dialog::None;
    }

    pub fn new_profile_dialog(&mut self) {
        self.dialog = Dialog::Profile(Box::new(ProfileDialog::new(ProfileForm::blank())));
    }

    pub fn edit_profile_dialog(&mut self, id: ProfileId) {
        if let Some(p) = self.profile(id) {
            let form = ProfileForm::from_profile(p);
            self.dialog = Dialog::Profile(Box::new(ProfileDialog::new(form)));
        }
    }

    pub fn duplicate(&mut self, id: ProfileId) {
        let Some(p) = self.profile(id) else { return };
        let d = duplicate_profile(p);
        match self.store.create_profile(&d) {
            Ok(()) => {
                self.selected = Some(d.id);
                self.set_notice(i18n::profile_duplicated(&d.name));
                self.reload();
            }
            Err(e) => self.set_notice(e.to_string()),
        }
    }

    pub fn toggle_favorite(&mut self, id: ProfileId) {
        let Some(mut p) = self.profile(id).cloned() else {
            return;
        };
        p.favorite = !p.favorite;
        match self.store.update_profile(&p) {
            Ok(()) => self.reload(),
            Err(e) => self.set_notice(e.to_string()),
        }
    }

    /// Exclui o perfil e as credenciais que ninguém mais usa.
    pub fn delete_profile(&mut self, id: ProfileId) {
        let Some(p) = self.profile(id).cloned() else {
            return;
        };
        match self.store.delete_profile(id) {
            Ok(()) => {
                for c in credential_ids(&p.auth) {
                    if let Some(r) = release_credential(&self.store, c) {
                        self.purge_secret(r);
                    }
                }
                self.set_notice(i18n::profile_deleted(&p.name));
                self.reload();
            }
            Err(e) => self.set_notice(e.to_string()),
        }
    }

    pub fn confirm_quick(&mut self, text: &str) -> Result<(), String> {
        let target = parse_quick(text).map_err(|e| e.to_string())?;
        let profile = quick_profile(&target);
        profile.validate().map_err(|e| e.to_string())?;
        self.open_tab(TabKind::Terminal {
            profile: Box::new(profile),
            saved: false,
        });
        Ok(())
    }

    // ------------------------------------------------------------------ abas

    fn new_body(&self, kind: &TabKind) -> Box<dyn TabBody> {
        let jump_name = match kind {
            TabKind::Terminal { profile, .. } => profile
                .jump
                .and_then(|j| self.profile(j))
                .map(|j| j.name.clone()),
            TabKind::Welcome => None,
        };
        make_body(kind, jump_name)
    }

    pub fn open_tab(&mut self, kind: TabKind) {
        let id = self.next_tab;
        self.next_tab += 1;
        let body = self.new_body(&kind);
        self.tabs.push(Tab { id, kind, body });
        self.active = Some(id);
    }

    // --------------------------------------------------------- painéis divididos

    fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        let id = self.active?;
        self.tabs.iter_mut().find(|t| t.id == id)
    }

    /// Tipo (perfil) do painel focado da aba ativa, se for uma aba de terminal.
    pub fn focused_kind(&self) -> Option<TabKind> {
        let t = self.tabs.iter().find(|t| Some(t.id) == self.active)?;
        let kind = match t.body.as_split_ref() {
            Some(sp) => sp.focused_kind().clone(),
            None => t.kind.clone(),
        };
        matches!(kind, TabKind::Terminal { .. }).then_some(kind)
    }

    /// Divide o painel focado da aba ativa. `profile` = `None` repete o perfil do painel focado.
    pub fn split_active(&mut self, dir: Dir, profile: Option<ProfileId>) {
        let Some(base) = self.focused_kind() else {
            return self.set_notice(i18n::SPLIT_ONLY_TERMINAL());
        };
        let kind = match profile.and_then(|id| self.profile(id).cloned()) {
            Some(p) => TabKind::Terminal {
                profile: Box::new(p),
                saved: true,
            },
            None => base,
        };
        let body = self.new_body(&kind);
        let Some(tab) = self.active_tab_mut() else {
            return;
        };
        if tab.body.as_split().is_none() {
            let old = std::mem::replace(&mut tab.body, Box::new(crate::tabs::WelcomeBody));
            tab.body = Box::new(SplitBody::new(tab.kind.clone(), old));
        }
        let ok = tab
            .body
            .as_split()
            .is_some_and(|sp| sp.split(dir, kind, body).is_some());
        if !ok {
            self.set_notice(i18n::SPLIT_LIMIT());
        }
    }

    pub fn can_split(&self) -> bool {
        self.focused_kind().is_some()
            && self
                .tabs
                .iter()
                .find(|t| Some(t.id) == self.active)
                .and_then(|t| t.body.as_split_ref())
                .is_none_or(|sp| sp.pane_count() < MAX_PANES)
    }

    pub fn open_split_picker(&mut self, dir: Dir) {
        if self.focused_kind().is_some() {
            self.dialog = Dialog::Split(SplitDialog { dir, choice: None });
        } else {
            self.set_notice(i18n::SPLIT_ONLY_TERMINAL());
        }
    }

    /// Pede para fechar um painel; sessão ativa pede confirmação. Último painel = fecha a aba.
    pub fn request_close_pane(&mut self, tab: u64, pane: PaneId) {
        let Some(t) = self.tabs.iter().find(|t| t.id == tab) else {
            return;
        };
        let Some(sp) = t.body.as_split_ref() else {
            return self.request_close_tab(tab);
        };
        if sp.pane_count() <= 1 {
            return self.request_close_tab(tab);
        }
        if sp.is_pane_busy(pane) {
            self.dialog = Dialog::ConfirmClosePane { tab, pane };
        } else {
            self.close_pane(tab, pane);
        }
    }

    pub fn close_pane(&mut self, tab: u64, pane: PaneId) {
        let Some(t) = self.tabs.iter_mut().find(|t| t.id == tab) else {
            return;
        };
        let empty = t.body.as_split().is_some_and(|sp| sp.close_pane(pane));
        if empty {
            self.close_tab(tab);
        }
    }

    pub fn cycle_pane(&mut self) {
        if let Some(sp) = self.active_tab_mut().and_then(|t| t.body.as_split()) {
            sp.cycle_focus();
        }
    }

    // ------------------------------------------------------------ sessões e cofre

    /// Sessões ativas (abas e painéis).
    pub fn active_sessions(&self) -> usize {
        self.tabs
            .iter()
            .map(|t| match t.body.as_split_ref() {
                Some(sp) => sp
                    .pane_ids()
                    .into_iter()
                    .filter(|p| sp.is_pane_busy(*p))
                    .count(),
                None => usize::from(t.body.is_busy()),
            })
            .sum()
    }

    pub fn request_end_all(&mut self) {
        if self.active_sessions() == 0 {
            self.set_notice(i18n::END_ALL_NONE());
        } else {
            self.dialog = Dialog::ConfirmEndAll;
        }
    }

    /// Desconecta todas as sessões (as abas continuam abertas). Não mexe no cofre.
    pub fn end_all_sessions(&mut self) {
        for t in &mut self.tabs {
            t.body.close();
        }
        self.set_notice(i18n::END_ALL_DONE());
    }

    pub fn lock_vault(&mut self) {
        self.vault.lock();
        self.set_notice(i18n::VAULT_LOCKED_NOTICE());
    }

    // -------------------------------------------------- importação do ssh config

    /// Abre o diálogo de importação (etapa de escolha do arquivo).
    pub fn open_import_ssh(&mut self) {
        self.dialog = Dialog::ImportSsh(Box::new(ImportDialog::new()));
    }

    /// Lê o arquivo em segundo plano (`None` = `~/.ssh/config`).
    pub fn start_import_load(&mut self, ctx: &egui::Context, path: Option<PathBuf>) {
        if let Dialog::ImportSsh(d) = &mut self.dialog {
            d.stage = ImportStage::Loading;
            d.error = None;
        }
        let existing = self.profiles.clone();
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        self.rt.spawn_blocking(move || {
            let r = path
                .or_else(ssh_import::default_path)
                .ok_or_else(|| i18n::IMPORT_ERR_NO_HOME().to_string())
                .and_then(|p| ssh_import::load_with_env(&p))
                .map(|raw| ssh_import::build_preview(raw, &existing));
            let _ = tx.send(Msg::SshConfigLoaded(r));
            ctx.request_repaint();
        });
    }

    /// Escolhe o arquivo com o seletor do sistema e o lê.
    pub fn pick_import_file(&self, ctx: &egui::Context) {
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        let start = crate::paths::ssh_dir();
        let existing = self.profiles.clone();
        self.rt.spawn(async move {
            let mut dlg = rfd::AsyncFileDialog::new().set_title(i18n::IMPORT_PICK_TITLE());
            if let Some(d) = start {
                dlg = dlg.set_directory(d);
            }
            let Some(f) = dlg.pick_file().await else {
                return;
            };
            let path = f.path().to_path_buf();
            let r = tokio::task::spawn_blocking(move || {
                ssh_import::load_with_env(&path)
                    .map(|raw| ssh_import::build_preview(raw, &existing))
            })
            .await
            .unwrap_or_else(|_| Err(i18n::ERR_INTERNAL().to_string()));
            let _ = tx.send(Msg::SshConfigLoaded(r));
            ctx.request_repaint();
        });
    }

    fn on_import_loaded(&mut self, r: Result<Preview, String>) {
        let Dialog::ImportSsh(d) = &mut self.dialog else {
            return;
        };
        match r {
            Ok(p) => {
                d.preview = p;
                d.stage = ImportStage::Preview;
            }
            Err(e) => {
                d.error = Some(e);
                d.stage = ImportStage::Intro;
            }
        }
    }

    /// Grava os candidatos marcados (em segundo plano).
    pub fn confirm_import(&mut self, ctx: &egui::Context) {
        let Dialog::ImportSsh(d) = &mut self.dialog else {
            return;
        };
        if d.stage != ImportStage::Preview {
            return;
        }
        let profiles = d.preview.prepare();
        if profiles.is_empty() {
            return;
        }
        d.stage = ImportStage::Saving;
        let (store, tx, ctx) = (self.store.clone(), self.tx.clone(), ctx.clone());
        self.rt.spawn_blocking(move || {
            let _ = tx.send(Msg::SshConfigSaved(ssh_import::save(&store, profiles)));
            ctx.request_repaint();
        });
    }

    fn on_import_saved(&mut self, r: Result<SaveReport, String>) {
        match r {
            Ok(rep) => {
                if matches!(self.dialog, Dialog::ImportSsh(_)) {
                    self.dialog = Dialog::None;
                }
                self.reload();
                self.set_notice(i18n::import_done(rep.imported));
            }
            Err(e) => {
                if let Dialog::ImportSsh(d) = &mut self.dialog {
                    d.stage = ImportStage::Preview;
                    d.error = Some(format!("{}{e}", i18n::IMPORT_ERR_SAVE()));
                }
            }
        }
    }

    // ------------------------------------------ exportar / importar configuração

    pub fn export_config(&mut self, ctx: &egui::Context) {
        if self.config_busy {
            return;
        }
        self.config_busy = true;
        let (store, tx, ctx) = (self.store.clone(), self.tx.clone(), ctx.clone());
        self.rt.spawn(async move {
            let dlg = rfd::AsyncFileDialog::new()
                .set_title(i18n::CFG_EXPORT_TITLE())
                .set_file_name(i18n::CFG_EXPORT_NAME())
                .add_filter("JSON", &["json"]);
            let msg = match dlg.save_file().await {
                None => Err(String::new()),
                Some(f) => {
                    let path = f.path().to_path_buf();
                    tokio::task::spawn_blocking(move || crate::config_io::export_to(&store, &path))
                        .await
                        .unwrap_or_else(|_| Err(i18n::ERR_INTERNAL().to_string()))
                }
            };
            let _ = tx.send(Msg::ConfigExported(msg));
            ctx.request_repaint();
        });
    }

    pub fn import_config(&mut self, ctx: &egui::Context) {
        if self.config_busy {
            return;
        }
        self.config_busy = true;
        let (store, tx, ctx) = (self.store.clone(), self.tx.clone(), ctx.clone());
        self.rt.spawn(async move {
            let dlg = rfd::AsyncFileDialog::new()
                .set_title(i18n::CFG_IMPORT_TITLE())
                .add_filter("JSON", &["json"]);
            let r = match dlg.pick_file().await {
                None => Err(String::new()),
                Some(f) => {
                    let path = f.path().to_path_buf();
                    tokio::task::spawn_blocking(move || {
                        crate::config_io::import_from(&store, &path)
                    })
                    .await
                    .unwrap_or_else(|_| Err(i18n::ERR_INTERNAL().to_string()))
                }
            };
            let _ = tx.send(Msg::ConfigImported(r));
            ctx.request_repaint();
        });
    }

    pub fn open_profile(&mut self, id: ProfileId) {
        if let Some(p) = self.profile(id).cloned() {
            self.open_tab(TabKind::Terminal {
                profile: Box::new(p),
                saved: true,
            });
        }
    }

    /// Fecha a aba; se a sessão estiver ativa, pede confirmação.
    pub fn request_close_tab(&mut self, id: u64) {
        let busy = self
            .tabs
            .iter()
            .find(|t| t.id == id)
            .is_some_and(|t| t.body.is_busy());
        if busy {
            self.dialog = Dialog::ConfirmCloseTab(id);
        } else {
            self.close_tab(id);
        }
    }

    pub fn close_tab(&mut self, id: u64) {
        if let Some(i) = self.tabs.iter().position(|t| t.id == id) {
            let mut t = self.tabs.remove(i);
            t.body.close();
            if self.active == Some(id) {
                self.active = self
                    .tabs
                    .get(i.min(self.tabs.len().saturating_sub(1)))
                    .map(|t| t.id);
            }
        }
    }

    /// Transferências em andamento em todas as abas.
    pub fn active_transfers(&self) -> usize {
        self.tabs.iter().map(|t| t.body.active_transfers()).sum()
    }

    /// Sincronizações em execução em todas as abas.
    pub fn active_syncs(&self) -> usize {
        self.tabs.iter().map(|t| t.body.active_syncs()).sum()
    }

    pub fn any_busy(&self) -> bool {
        self.tabs.iter().any(|t| t.body.is_busy())
    }

    pub fn terminal_focused(&self) -> bool {
        self.active
            .and_then(|id| self.tabs.iter().find(|t| t.id == id))
            .is_some_and(|t| t.body.has_terminal_focus())
    }

    // ---------------------------------------------------------- atalhos/estado

    fn run_action(&mut self, a: Action) {
        match a {
            Action::NewTab => {
                if let Some(id) = self.selected {
                    self.open_profile(id);
                }
            }
            Action::CloseTab => {
                if let Some(id) = self.active {
                    self.request_close_tab(id);
                }
            }
            Action::NewProfile => self.new_profile_dialog(),
            Action::FocusSearch => self.focus_search = true,
            Action::SplitSide => self.split_active(Dir::Vertical, None),
            Action::SplitStack => self.split_active(Dir::Horizontal, None),
            Action::ClosePane => {
                let target = self.active.and_then(|tab| {
                    let t = self.tabs.iter().find(|t| t.id == tab)?;
                    let pane = t.body.as_split_ref().map(SplitBody::focused_id);
                    Some((tab, pane))
                });
                match target {
                    Some((tab, Some(pane))) => self.request_close_pane(tab, pane),
                    Some((tab, None)) => self.request_close_tab(tab),
                    None => {}
                }
            }
            Action::NextPane => self.cycle_pane(),
            Action::ToggleFiles => {
                if let Some(t) = self.active_tab_mut() {
                    t.body.toggle_files();
                }
            }
        }
    }

    pub fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if !matches!(self.dialog, Dialog::None) {
            return;
        }
        for a in shortcuts::poll(ctx, self.terminal_focused()) {
            self.run_action(a);
        }
    }

    /// Reação à suspensão detectada: bloqueia o cofre (se configurado). Não encerra sessões.
    pub(crate) fn on_suspend(&mut self, suspended: bool) {
        if suspended && self.settings.vault_lock_on_suspend && !self.vault.is_locked() {
            self.vault.lock();
            self.set_notice(i18n::VAULT_LOCKED_SUSPEND());
        }
    }

    /// Cofre: atividade, bloqueio por inatividade. Barato (mutex), seguro na UI.
    pub fn vault_housekeeping(&mut self, ctx: &egui::Context) {
        // Primeiro a suspensão: ao acordar, o cofre fecha antes de qualquer uso neste quadro.
        let suspended = self.suspend.observe_now();
        self.on_suspend(suspended);
        if ctx.input(|i| !i.events.is_empty()) {
            self.vault.touch();
        }
        self.vault.tick(Instant::now());
        if self.settings.vault_lock_minutes > 0 || self.settings.vault_lock_on_suspend {
            ctx.request_repaint_after(Duration::from_secs(5));
        }
    }

    /// Aplica tema e persiste configurações quando mudam (sem gravar durante arrastes).
    pub fn settings_housekeeping(&mut self, ctx: &egui::Context) {
        if self.applied_theme != Some(self.settings.theme) {
            ctx.set_theme(self.settings.theme.to_egui());
            self.applied_theme = Some(self.settings.theme);
        }
        if self.settings != self.saved_settings && !ctx.input(|i| i.pointer.any_down()) {
            self.vault.set_idle_timeout(self.settings.vault_timeout());
            match self.settings.save(&self.store) {
                Ok(()) => self.saved_settings = self.settings.clone(),
                Err(e) => {
                    self.saved_settings = self.settings.clone();
                    self.set_notice(e.to_string());
                }
            }
        }
    }

    /// Janela fechando com sessão ativa: pede confirmação.
    pub fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.quit_ok && self.any_busy() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.dialog = Dialog::ConfirmQuit;
        }
    }

    pub fn confirm_quit(&mut self, ctx: &egui::Context) {
        self.quit_ok = true;
        self.close_dialog();
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Desconecta todas as sessões e espera (no máximo 3 s) o desligamento educado terminar.
    pub fn close_all_tabs(&mut self) {
        for t in &mut self.tabs {
            t.body.close();
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline && !self.tabs.iter().all(|t| t.body.is_closed()) {
            for t in &mut self.tabs {
                t.body.drain();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    // ----------------------------------------------------- abas: integração

    /// Separa as abas dos serviços que elas usam (empréstimos disjuntos).
    pub(crate) fn tabs_and_env<'a>(
        &'a mut self,
        ctx: &'a egui::Context,
    ) -> (&'a mut Vec<Tab>, TabEnv<'a>) {
        let input_blocked =
            !matches!(self.dialog, Dialog::None) || self.tabs.iter().any(|t| t.body.needs_modal());
        let env = TabEnv {
            rt: self.rt.handle(),
            store: &self.store,
            vault: &self.vault,
            ctx,
            font_size: self.settings.terminal_font_size,
            confirm_paste: self.settings.confirm_multiline_paste,
            input_blocked,
            auto_connect: self.auto_connect,
            dark: ctx.theme() == egui::Theme::Dark,
            prod_warning: self.settings.prod_warning,
            theme: None,
            default_overwrite: self.settings.files_overwrite,
            data_dir: self.data_dir.clone(),
        };
        (&mut self.tabs, env)
    }

    /// Todo frame, todas as abas: drena sessões (inclusive em segundo plano) e avança conexões.
    pub fn pump_tabs(&mut self, ctx: &egui::Context) {
        let (tabs, env) = self.tabs_and_env(ctx);
        for t in tabs.iter_mut() {
            t.body.pump(&env);
        }
    }

    /// Mostra o diálogo pendente de uma aba (a ativa tem prioridade). Um por vez.
    pub fn tab_modals(&mut self, ctx: &egui::Context) {
        if !matches!(self.dialog, Dialog::None) {
            return;
        }
        let active = self.active;
        let (tabs, env) = self.tabs_and_env(ctx);
        let idx = tabs
            .iter()
            .position(|t| Some(t.id) == active && t.body.needs_modal())
            .or_else(|| tabs.iter().position(|t| t.body.needs_modal()));
        if let Some(i) = idx {
            tabs[i].body.modal_ui(ctx, &env);
        }
    }

    /// Pedidos das abas (recarregar perfis, abrir hosts confiáveis).
    pub fn process_tab_events(&mut self) {
        let events: Vec<(u64, TabEvent)> = self
            .tabs
            .iter_mut()
            .flat_map(|t| {
                let id = t.id;
                t.body.take_events().into_iter().map(move |e| (id, e))
            })
            .collect();
        for (tab, e) in events {
            match e {
                TabEvent::ProfileChanged => self.reload(),
                TabEvent::OpenTrustedHosts => {
                    self.open_settings();
                    self.scroll_to_trusted = true;
                }
                TabEvent::ClosePane(pane) => self.request_close_pane(tab, pane),
            }
        }
    }

    // --------------------------------------------------------- hosts confiáveis

    pub fn open_about(&mut self) {
        self.dialog = Dialog::About(crate::about::AboutState::new());
    }

    pub fn open_settings(&mut self) {
        self.refresh_trusted();
        self.dialog = Dialog::Settings;
    }

    pub fn refresh_trusted(&mut self) {
        match self.store.list() {
            Ok(mut l) => {
                l.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.algorithm.cmp(&b.1.algorithm)));
                self.trusted = l;
                self.trusted_error = None;
            }
            Err(_) => self.trusted_error = Some(i18n::TRUSTED_ERR().into()),
        }
    }

    /// Remove a chave confiável (o próximo acesso cai no fluxo de host desconhecido).
    pub fn forget_host(&mut self, ep: &HostEndpoint) {
        match self.store.forget(ep) {
            Ok(()) => self.trusted_error = None,
            Err(_) => self.trusted_error = Some(i18n::TRUSTED_ERR().into()),
        }
        self.refresh_trusted();
    }

    /// Há operação em andamento (usado pela demo para esperar).
    #[cfg(any(test, debug_assertions, feature = "demo"))]
    pub fn is_idle(&self) -> bool {
        self.pending.is_none()
    }
}

pub fn secret_error_text(e: &SecretError) -> String {
    match e {
        SecretError::Unavailable(_) => i18n::ERR_VAULT_UNAVAILABLE().into(),
        SecretError::Locked => i18n::ERR_VAULT_LOCKED().into(),
        SecretError::NotFound => i18n::ERR_SECRET_NOT_FOUND().into(),
        SecretError::TooLong { .. } => i18n::ERR_SECRET_TOO_LONG().into(),
        SecretError::Backend(_) => i18n::ERR_VAULT_BACKEND().into(),
    }
}
