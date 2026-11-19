//! Console do sistema operacional.
use myssh_i18n::tr;

pub fn LOCAL_CONSOLE() -> &'static str {
    tr!("Console local", "Local console", "Consola local")
}
pub fn LOCAL_SHELL() -> &'static str {
    tr!("Terminal (shell)", "Terminal (shell)", "Terminal (shell)")
}
pub fn LOCAL_HELP() -> &'static str {
    tr!(
        "Abre um terminal neste computador. Escolha o shell instalado que deseja usar.",
        "Open a terminal on this computer. Choose the installed shell you want to use.",
        "Abre un terminal en este equipo. Elige el shell instalado que deseas usar."
    )
}
pub fn LOCAL_LOADING() -> &'static str {
    tr!(
        "Buscando terminais…",
        "Finding shells…",
        "Buscando terminales…"
    )
}
pub fn LOCAL_EMPTY() -> &'static str {
    tr!(
        "Nenhum terminal disponível.",
        "No shells available.",
        "No hay terminales disponibles."
    )
}
pub fn LOCAL_START_ERROR() -> &'static str {
    tr!(
        "Não foi possível iniciar o console local.",
        "Could not start the local console.",
        "No se pudo iniciar la consola local."
    )
}
pub fn LOCAL_IO_ERROR() -> &'static str {
    tr!(
        "Falha de comunicação com o console local.",
        "Local console communication failed.",
        "Error de comunicación con la consola local."
    )
}

pub fn paste_more_lines(n: usize) -> String {
    tr!(
        format!("+{n} linhas"),
        format!("+{n} lines"),
        format!("+{n} líneas")
    )
}

pub fn LOCAL_DIRECTORY() -> &'static str {
    tr!(
        "Diretório inicial",
        "Starting directory",
        "Directorio inicial"
    )
}
pub fn LOCAL_DIRECTORY_DEFAULT() -> &'static str {
    tr!(
        "Pasta pessoal (padrão)",
        "Home folder (default)",
        "Carpeta personal (predeterminada)"
    )
}
pub fn LOCAL_DIRECTORY_HELP() -> &'static str {
    tr!(
        "Vazio: pasta pessoal. Use um caminho absoluto ou selecione uma pasta.",
        "Leave empty for your home folder. Enter an absolute path or select a folder.",
        "Vacío: carpeta personal. Escribe una ruta absoluta o selecciona una carpeta."
    )
}
pub fn LOCAL_PICK_DIRECTORY() -> &'static str {
    tr!(
        "Selecionar diretório inicial",
        "Select starting directory",
        "Seleccionar directorio inicial"
    )
}
pub fn LOCAL_DIRECTORY_ERROR() -> &'static str {
    tr!(
        "O diretório inicial do console local não está disponível.",
        "The local console's starting directory is not available.",
        "El directorio inicial de la consola local no está disponible."
    )
}
