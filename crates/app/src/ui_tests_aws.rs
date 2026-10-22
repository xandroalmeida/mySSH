//! UI do transporte AWS SSM: formulário e diálogo de login IAM Identity Center.
//!
//! Renders: `MYSSH_DEMO_SHOTS=dir cargo test -p myssh-app aws_screenshots -- --ignored`

use egui_kittest::Harness;
use myssh_core::{AwsCredentials, AwsSsm, Profile, Transport};

use crate::app::{App, Dialog};
use crate::form::{AwsCredsKind, TransportKind};
use crate::tabs::{Tab, TabKind};
use crate::terminal_tab::TerminalTab;
use crate::ui_tests::memory_app;

fn ssm_profile() -> Profile {
    let mut p = Profile::new("ec2-commandcenter", "i-0f934fb5955059e81", "ubuntu");
    p.transport = Transport::AwsSsm(AwsSsm {
        region: "us-east-1".into(),
        credentials: AwsCredentials::Sso {
            start_url: "https://d-9067abcdef.awsapps.com/start".into(),
            sso_region: "us-east-1".into(),
            account_id: "123456789012".into(),
            role_name: "AdministratorAccess".into(),
        },
    });
    p
}

fn form_mut(app: &mut App) -> &mut crate::form::ProfileForm {
    match &mut app.dialog {
        Dialog::Profile(d) => &mut d.form,
        _ => panic!("diálogo de perfil fechado"),
    }
}

/// Os textos de ajuda longos quebram linha: o diálogo cabe numa janela estreita.
#[test]
fn dialogo_ssm_nao_alarga_alem_da_janela() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(700.0, 900.0))
        .build_eframe(|_| memory_app());
    h.run_steps(2);
    h.state_mut().new_profile_dialog();
    {
        let f = form_mut(h.state_mut());
        f.transport = TransportKind::AwsSsm;
        f.auth = crate::form::AuthKind::InstanceConnect;
    }
    h.run_steps(4);
    let rect = h
        .ctx
        .memory(|m| m.area_rect(egui::Id::new("profile-dialog")))
        .expect("área do modal");
    assert!(rect.width() < 640.0, "modal largo demais: {rect:?}");
    assert!(rect.left() >= 0.0, "modal vaza à esquerda: {rect:?}");
}

#[test]
fn salvar_perfil_ssm_pelo_dialogo_persiste_transporte() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1100.0, 800.0))
        .build_eframe(|_| memory_app());
    h.run_steps(2);
    h.state_mut().new_profile_dialog();
    {
        let f = form_mut(h.state_mut());
        f.name = "ec2".into();
        f.host = "i-0123456789abcdef0".into();
        f.username = "ubuntu".into();
        f.transport = TransportKind::AwsSsm;
        f.aws_profile = "prod".into();
    }
    h.run_steps(2);
    let draft = form_mut(h.state_mut()).build(&[]).unwrap();
    h.state().store.create_profile(&draft.profile).unwrap();
    let back = h
        .state()
        .store
        .get_profile(draft.profile.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        back.transport,
        Transport::AwsSsm(AwsSsm {
            region: String::new(),
            credentials: AwsCredentials::Profile {
                name: Some("prod".into())
            },
        })
    );
}

fn shot(h: &mut Harness<'static, App>, dir: &std::path::Path, name: &str) {
    h.run_steps(8);
    let img = h.render().expect("render");
    let path = dir.join(format!("{name}.png"));
    img.save(&path).unwrap();
    eprintln!("screenshot: {}", path.display());
}

#[test]
#[ignore = "gera PNGs; requer MYSSH_DEMO_SHOTS=dir e GPU (wgpu)"]
fn aws_screenshots() {
    let Some(dir) = std::env::var_os("MYSSH_DEMO_SHOTS").map(std::path::PathBuf::from) else {
        eprintln!("MYSSH_DEMO_SHOTS não definido: nada a fazer");
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = Harness::builder()
        .with_size(egui::vec2(900.0, 900.0))
        .with_pixels_per_point(1.5)
        .wgpu()
        .build_eframe(|_| memory_app());
    h.state_mut().settings.theme = crate::settings::ThemePref::Dark;
    h.run_steps(3);

    // 1. perfil AWS (~/.aws)
    h.state_mut().new_profile_dialog();
    {
        let f = form_mut(h.state_mut());
        f.name = "ec2-commandcenter".into();
        f.host = "i-0f934fb5955059e81".into();
        f.username = "ubuntu".into();
        f.transport = TransportKind::AwsSsm;
        f.auth = crate::form::AuthKind::InstanceConnect;
        f.aws_profile = "prod".into();
    }
    shot(&mut h, &dir, "aws-01-perfil-aws");

    // 2. IAM Identity Center
    {
        let f = form_mut(h.state_mut());
        f.aws_creds = AwsCredsKind::Sso;
        f.aws_region = "us-east-1".into();
        f.sso_start_url = "https://d-9067abcdef.awsapps.com/start".into();
        f.sso_region = "us-east-1".into();
        f.sso_account = "123456789012".into();
        f.sso_role = "AdministratorAccess".into();
    }
    shot(&mut h, &dir, "aws-02-sso");
    h.state_mut().close_dialog();

    // 3. diálogo de login SSO na aba
    let p = ssm_profile();
    let mut tab = TerminalTab::new(p.clone(), true, None);
    tab.debug_aws_login(
        "https://device.sso.us-east-1.amazonaws.com/?user_code=WDKT-QZPM",
        "WDKT-QZPM",
    );
    h.state_mut().tabs.push(Tab {
        id: 990,
        kind: TabKind::Terminal {
            profile: Box::new(p),
            saved: true,
        },
        body: Box::new(tab),
    });
    h.state_mut().active = Some(990);
    shot(&mut h, &dir, "aws-03-login-sso");
}
