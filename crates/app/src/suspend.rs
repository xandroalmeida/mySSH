//! Detecção de suspensão do computador por salto de relógio (multiplataforma).
//!
//! # Como funciona
//! A cada quadro compara-se o tempo de parede (`SystemTime`) com o monotônico (`Instant`).
//! Em condições normais os dois avançam juntos. Quando o computador suspende, o relógio de
//! parede continua contando, mas o monotônico para (macOS: `mach_absolute_time`; Linux:
//! `CLOCK_MONOTONIC`). Se a diferença entre os dois avanços passa de [`SUSPEND_GAP`], houve
//! suspensão (ou hibernação) e o cofre deve ser bloqueado.
//!
//! # Limitações
//! * Não há notificação nativa (`NSWorkspaceWillSleepNotification`, bloqueio de tela): só se
//!   percebe a suspensão no primeiro quadro **depois** de acordar. O app roda esta checagem
//!   antes de qualquer outra coisa do quadro, então o cofre já estará bloqueado quando a
//!   interface voltar a funcionar; tarefas em segundo plano que leiam o cofre exatamente na
//!   janela entre acordar e o primeiro quadro não são cobertas.
//! * Bloquear a tela sem suspender não é detectado (a inatividade continua valendo).
//! * No Windows o `Instant` conta o tempo em suspensão em algumas versões; nesse caso a
//!   detecção não dispara e vale apenas o bloqueio por inatividade.
//! * Um ajuste manual do relógio para frente também bloqueia (falha segura).

use std::time::{Duration, Instant, SystemTime};

/// Diferença mínima entre o avanço do relógio de parede e do monotônico para contar como suspensão.
pub const SUSPEND_GAP: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct SuspendDetector {
    gap: Duration,
    last: Option<(SystemTime, Instant)>,
}

impl Default for SuspendDetector {
    fn default() -> Self {
        Self::new(SUSPEND_GAP)
    }
}

impl SuspendDetector {
    pub fn new(gap: Duration) -> Self {
        Self { gap, last: None }
    }

    /// Registra uma amostra; `true` se, desde a anterior, o computador parece ter suspendido.
    pub fn observe(&mut self, wall: SystemTime, mono: Instant) -> bool {
        let prev = self.last.replace((wall, mono));
        let Some((pw, pm)) = prev else {
            return false;
        };
        // Relógio de parede andando para trás (ajuste manual) não indica suspensão.
        let wall_dt = wall.duration_since(pw).unwrap_or_default();
        let mono_dt = mono.saturating_duration_since(pm);
        wall_dt.saturating_sub(mono_dt) > self.gap
    }

    /// Amostra com os relógios reais.
    pub fn observe_now(&mut self) -> bool {
        self.observe(SystemTime::now(), Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> (SystemTime, Instant) {
        (
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000),
            Instant::now(),
        )
    }

    #[test]
    fn primeira_amostra_nunca_dispara() {
        let (w, m) = t0();
        assert!(!SuspendDetector::default().observe(w, m));
    }

    #[test]
    fn avanco_normal_nao_dispara() {
        let (w, m) = t0();
        let mut d = SuspendDetector::default();
        d.observe(w, m);
        assert!(!d.observe(w + Duration::from_secs(5), m + Duration::from_secs(5)));
        // app parado (janela oculta, sem repaint): os dois relógios avançam igual
        assert!(!d.observe(w + Duration::from_secs(3605), m + Duration::from_secs(3600)));
    }

    #[test]
    fn salto_de_parede_dispara() {
        let (w, m) = t0();
        let mut d = SuspendDetector::default();
        d.observe(w, m);
        // dormiu 30 min: parede +30 min, monotônico +5 s
        assert!(d.observe(w + Duration::from_secs(1800), m + Duration::from_secs(5)));
        // a amostra seguinte volta ao normal
        assert!(!d.observe(w + Duration::from_secs(1805), m + Duration::from_secs(10)));
    }

    #[test]
    fn limiar_e_relogio_para_tras() {
        let (w, m) = t0();
        let mut d = SuspendDetector::new(Duration::from_secs(10));
        d.observe(w, m);
        assert!(!d.observe(w + Duration::from_secs(15), m + Duration::from_secs(6)));
        assert!(d.observe(w + Duration::from_secs(40), m + Duration::from_secs(8)));
        // relógio de parede voltou no tempo: sem disparo
        assert!(!d.observe(w, m + Duration::from_secs(9)));
    }
}
