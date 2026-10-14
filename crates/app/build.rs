//! Embute ícone e metadados de versão no .exe (somente quando o build roda em Windows
//! para alvo Windows) e informações de build para a janela "Sobre": `MYSSH_GIT_COMMIT`,
//! `MYSSH_GIT_DIRTY` e `MYSSH_BUILD_DATE`.

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Data civil (UTC) a partir de dias desde 1970-01-01 (algoritmo de Howard Hinnant).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

fn build_date() -> String {
    let secs = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs() as i64)
        })
        .unwrap_or(0);
    let (y, m, d) = civil(secs.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

fn git_info() {
    let commit = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_default();
    let dirty =
        git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty());
    println!("cargo:rustc-env=MYSSH_GIT_COMMIT={commit}");
    println!("cargo:rustc-env=MYSSH_GIT_DIRTY={}", u8::from(dirty));
    println!("cargo:rustc-env=MYSSH_BUILD_DATE={}", build_date());
    // Refaz quando o HEAD ou a branch atual mudam (funciona também em worktrees).
    for what in ["HEAD", "refs/heads"] {
        if let Some(p) = git(&["rev-parse", "--git-path", what]) {
            println!("cargo:rerun-if-changed={p}");
        }
    }
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
}

fn main() {
    git_info();
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("ProductName", "mySSH")
            .set("FileDescription", "mySSH - gerenciador de sessões SSH")
            .set("InternalName", "myssh")
            .set("OriginalFilename", "myssh.exe");
        if let Err(e) = res.compile() {
            println!("cargo:warning=falha ao embutir recursos do Windows: {e}");
        }
    }
}
