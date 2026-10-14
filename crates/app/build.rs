//! Embute ícone e metadados de versão no .exe (somente quando o build roda em Windows
//! para alvo Windows; em macOS/Linux é no-op).

fn main() {
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
