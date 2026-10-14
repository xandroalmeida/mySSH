//! Filtro de bytes antes do parser VTE (defesa contra saída remota hostil).
//!
//! O `vte` 0.15 (feature `std`) acumula o payload de OSC (`osc_raw`) **sem limite**: um servidor
//! que envia `ESC ] 0;` + centenas de MB sem terminador faria o RSS crescer sem parar. Além disso,
//! `CSI 22 t` (push de título) clona o título até 4096 vezes no `alacritty_terminal`.
//!
//! Este filtro é uma pequena máquina de estados (persistente entre chamadas, então sequências
//! partidas entre lotes funcionam) que espelha as transições do parser do `vte` 0.15:
//! - descarta o payload de OSC/DCS/SOS/PM/APC além de [`MAX_STRING_PAYLOAD`] bytes, mantendo o
//!   terminador (BEL, ST, CAN/SUB, ESC) para não dessincronizar o parser;
//! - remove `CSI 22 … t` e `CSI 23 … t` (push/pop da pilha de títulos), segurando no máximo
//!   [`MAX_CSI_HOLD`] bytes de uma CSI até o byte final.
//!
//! As formas C1 de 8 bits (0x9D, 0x90, 0x9F, 0x9E, 0x98) **não** iniciam strings no `vte` 0.15
//! (no estado Ground um byte C1 isolado é só `execute`; nos demais estados é ignorado), então não
//! há o que limitar nelas; o filtro espelha esse comportamento. Em texto UTF-8 comum o filtro só
//! procura ESC: nada é reinterpretado.

/// Máximo de bytes de payload de uma string de controle (OSC/DCS/APC/PM/SOS) repassados ao parser.
pub const MAX_STRING_PAYLOAD: usize = 4096;
/// Máximo de bytes de uma CSI retidos até o byte final (decidir se é `22 t`/`23 t`).
pub const MAX_CSI_HOLD: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum StrKind {
    Osc,
    Dcs,
    /// SOS, PM e APC.
    Other,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    Ground,
    /// Após ESC.
    Esc,
    /// Após ESC + intermediário (0x20..=0x2F).
    EscInt,
    /// Dentro de CSI; os bytes ficam retidos em `csi`.
    Csi,
    /// CSI longa demais que começava com 22/23: descarta até o byte final.
    CsiDrop,
    /// CSI longa demais, liberada: repassa até o byte final.
    CsiPass,
    Str(StrKind, usize),
}

pub struct StreamFilter {
    state: State,
    csi: Vec<u8>,
}

impl Default for StreamFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// A CSI retida (`ESC [` + corpo) é `22 …`/`23 …` sem marcador privado nem intermediário?
fn is_title_stack_csi(held: &[u8]) -> bool {
    let body = held.get(2..).unwrap_or(&[]);
    if !body.first().is_some_and(u8::is_ascii_digit) {
        return false;
    }
    if body.iter().any(|b| (0x20..=0x2F).contains(b)) {
        return false;
    }
    let mut v: u32 = 0;
    for b in body.iter().take_while(|b| b.is_ascii_digit()) {
        v = (v * 10 + u32::from(b - b'0')).min(u32::from(u16::MAX));
    }
    v == 22 || v == 23
}

impl StreamFilter {
    pub fn new() -> Self {
        Self {
            state: State::Ground,
            csi: Vec::with_capacity(MAX_CSI_HOLD),
        }
    }

    /// Bytes retidos pelo filtro (para testes de limite de memória).
    pub fn retained_bytes(&self) -> usize {
        self.csi.len()
    }

    /// Filtra `input` e acrescenta o resultado em `out`.
    pub fn filter(&mut self, input: &[u8], out: &mut Vec<u8>) {
        let mut i = 0;
        while i < input.len() {
            if self.state == State::Ground {
                // Caminho rápido: tudo até o próximo ESC passa intacto.
                match input[i..].iter().position(|&b| b == 0x1B) {
                    None => {
                        out.extend_from_slice(&input[i..]);
                        return;
                    }
                    Some(p) => {
                        out.extend_from_slice(&input[i..i + p]);
                        i += p;
                    }
                }
            }
            self.step(input[i], out);
            i += 1;
        }
    }

    fn flush_csi(&mut self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.csi);
        self.csi.clear();
    }

    fn step(&mut self, b: u8, out: &mut Vec<u8>) {
        match self.state {
            State::Ground => {
                // Só chega aqui com ESC; ele é emitido quando o byte seguinte decide o destino
                // (uma CSI fica retida inteira, inclusive o `ESC [`).
                self.state = State::Esc;
            }
            State::Esc | State::EscInt => {
                let inter = self.state == State::EscInt;
                if b == 0x1B {
                    return; // ESC repetido: o parser continua em Escape
                }
                if !inter && b != b'[' {
                    out.push(0x1B); // ESC pendente
                }
                match b {
                    0x18 | 0x1A => {
                        out.push(b);
                        self.state = State::Ground;
                    }
                    0x20..=0x2F => {
                        out.push(b);
                        self.state = State::EscInt;
                    }
                    0x30..=0x7E if inter => {
                        out.push(b);
                        self.state = State::Ground;
                    }
                    b'[' => {
                        self.csi.clear();
                        self.csi.extend_from_slice(b"\x1b[");
                        self.state = State::Csi;
                    }
                    b']' => {
                        out.push(b);
                        self.state = State::Str(StrKind::Osc, 0);
                    }
                    b'P' => {
                        out.push(b);
                        self.state = State::Str(StrKind::Dcs, 0);
                    }
                    b'X' | b'^' | b'_' => {
                        out.push(b);
                        self.state = State::Str(StrKind::Other, 0);
                    }
                    0x30..=0x7E => {
                        out.push(b);
                        self.state = State::Ground;
                    }
                    _ => out.push(b), // C0 executa; 0x7F e >= 0x80 ignorados: continua em Esc/EscInt
                }
            }
            State::Csi => match b {
                0x1B => {
                    self.flush_csi(out);
                    self.state = State::Esc;
                }
                0x18 | 0x1A => {
                    self.flush_csi(out);
                    out.push(b);
                    self.state = State::Ground;
                }
                0x40..=0x7E => {
                    if b == b't' && is_title_stack_csi(&self.csi) {
                        self.csi.clear(); // push/pop de título: descartado
                    } else {
                        self.flush_csi(out);
                        out.push(b);
                    }
                    self.state = State::Ground;
                }
                _ => {
                    if self.csi.len() < MAX_CSI_HOLD {
                        self.csi.push(b);
                    } else if is_title_stack_csi(&self.csi) {
                        self.csi.clear();
                        self.state = State::CsiDrop;
                    } else {
                        self.flush_csi(out);
                        out.push(b);
                        self.state = State::CsiPass;
                    }
                }
            },
            State::CsiDrop | State::CsiPass => {
                let drop = self.state == State::CsiDrop;
                match b {
                    0x1B => self.state = State::Esc,
                    0x18 | 0x1A => {
                        out.push(b);
                        self.state = State::Ground;
                    }
                    0x40..=0x7E => {
                        if !drop {
                            out.push(b);
                        }
                        self.state = State::Ground;
                    }
                    _ => {
                        if !drop {
                            out.push(b);
                        }
                    }
                }
            }
            State::Str(kind, n) => match b {
                0x1B => self.state = State::Esc,
                0x18 | 0x1A => {
                    out.push(b);
                    self.state = State::Ground;
                }
                0x07 if kind == StrKind::Osc => {
                    out.push(b);
                    self.state = State::Ground;
                }
                0x9C if kind == StrKind::Dcs => {
                    out.push(b);
                    self.state = State::Ground;
                }
                _ => {
                    if n < MAX_STRING_PAYLOAD {
                        out.push(b);
                        self.state = State::Str(kind, n + 1);
                    }
                    // além do limite: descarta o byte (o terminador continua sendo reconhecido).
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(f: &mut StreamFilter, data: &[u8]) -> Vec<u8> {
        let mut out = vec![];
        f.filter(data, &mut out);
        out
    }

    #[test]
    fn texto_e_sequencias_normais_passam_intactos() {
        let mut f = StreamFilter::new();
        let data = "olá \x1b[1;31mvermelho\x1b[0m \x1b]0;título\x07 fim \x1b[2J\x1b(B é".as_bytes();
        assert_eq!(run(&mut f, data), data);
        assert_eq!(f.retained_bytes(), 0);
    }

    #[test]
    fn osc_sem_terminador_e_limitado_mas_terminador_passa() {
        let mut f = StreamFilter::new();
        let mut data = b"\x1b]0;".to_vec();
        data.extend(std::iter::repeat_n(b'A', 1_000_000));
        let out = run(&mut f, &data);
        assert!(out.len() <= 4 + MAX_STRING_PAYLOAD, "{}", out.len());
        // O terminador (em outro lote) continua reconhecido; o texto seguinte passa.
        assert_eq!(run(&mut f, b"\x07depois"), b"\x07depois");
        // ST (ESC \) também.
        let mut data = b"\x1b]2;".to_vec();
        data.extend(std::iter::repeat_n(b'B', 10_000));
        data.extend_from_slice(b"\x1b\\ok");
        let out = run(&mut f, &data);
        assert!(out.ends_with(b"\x1b\\ok"));
        assert!(out.len() <= 4 + MAX_STRING_PAYLOAD + 4);
    }

    #[test]
    fn dcs_apc_pm_sos_sao_limitados() {
        for intro in [&b"\x1bP"[..], b"\x1b_", b"\x1b^", b"\x1bX"] {
            let mut f = StreamFilter::new();
            let mut data = intro.to_vec();
            data.extend(std::iter::repeat_n(b'x', 100_000));
            let out = run(&mut f, &data);
            assert!(out.len() <= 2 + MAX_STRING_PAYLOAD);
            assert_eq!(run(&mut f, b"\x1b\\z"), b"\x1b\\z");
        }
    }

    #[test]
    fn sequencia_partida_entre_lotes() {
        let mut f = StreamFilter::new();
        let mut out = vec![];
        for b in b"a\x1b[22;0tb\x1b[1mc\x1b]0;x\x07d" {
            f.filter(&[*b], &mut out);
        }
        assert_eq!(out, b"ab\x1b[1mc\x1b]0;x\x07d");
    }

    #[test]
    fn push_e_pop_de_titulo_sao_removidos() {
        let mut f = StreamFilter::new();
        assert_eq!(
            run(&mut f, b"\x1b[22t\x1b[23t\x1b[22;0t\x1b[023;1;2tok"),
            b"ok"
        );
        // Outros usos de 22/23 e de 't' ficam.
        assert_eq!(
            run(&mut f, b"\x1b[22m\x1b[14t\x1b[18t"),
            b"\x1b[22m\x1b[14t\x1b[18t"
        );
        assert_eq!(run(&mut f, b"\x1b[222t\x1b[?22t"), b"\x1b[222t\x1b[?22t");
        // CSI enorme começando com 22 é descartada por inteiro.
        let mut data = b"\x1b[22".to_vec();
        data.extend(std::iter::repeat_n(b';', 500));
        data.extend_from_slice(b"tok");
        assert_eq!(run(&mut f, &data), b"ok");
        assert_eq!(f.retained_bytes(), 0);
    }

    #[test]
    fn csi_interrompida_por_esc_ou_can_nao_perde_bytes() {
        let mut f = StreamFilter::new();
        assert_eq!(run(&mut f, b"\x1b[22\x1b[1m"), b"\x1b[22\x1b[1m");
        assert_eq!(run(&mut f, b"\x1b[22\x18x"), b"\x1b[22\x18x");
    }

    #[test]
    fn c1_de_8_bits_e_utf8_nao_quebram() {
        let mut f = StreamFilter::new();
        let data = "ĝ ý \u{9d} é".as_bytes();
        assert_eq!(run(&mut f, data), data);
        let raw = [0x9D, b'a', 0x90, b'b'];
        assert_eq!(run(&mut f, &raw), raw);
    }
}
