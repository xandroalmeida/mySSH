//! Taxa de transferência por média móvel (janela deslizante).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Média móvel de bytes/s sobre uma janela de tempo.
#[derive(Debug)]
pub struct RateMeter {
    window: Duration,
    samples: VecDeque<(Instant, u64)>,
}

impl RateMeter {
    pub fn new(window: Duration) -> Self {
        RateMeter {
            window,
            samples: VecDeque::new(),
        }
    }

    /// Registra o total acumulado de bytes em `now`.
    pub fn record(&mut self, now: Instant, total: u64) {
        self.samples.push_back((now, total));
        while self.samples.len() > 2 {
            let second = self.samples[1].0;
            if now.duration_since(second) >= self.window {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// Bytes/s na janela (0 sem pelo menos duas amostras ou sem avanço de tempo).
    pub fn rate_bps(&self) -> u64 {
        let (Some(first), Some(last)) = (self.samples.front(), self.samples.back()) else {
            return 0;
        };
        let dt = last.0.duration_since(first.0).as_secs_f64();
        if dt <= 0.0 {
            return 0;
        }
        (last.1.saturating_sub(first.1) as f64 / dt) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_movel() {
        let t0 = Instant::now();
        let mut m = RateMeter::new(Duration::from_secs(3));
        assert_eq!(m.rate_bps(), 0);
        m.record(t0, 0);
        assert_eq!(m.rate_bps(), 0);
        m.record(t0 + Duration::from_secs(1), 1_000);
        assert_eq!(m.rate_bps(), 1_000);
        m.record(t0 + Duration::from_secs(2), 3_000);
        assert_eq!(m.rate_bps(), 1_500);
    }

    #[test]
    fn janela_descarta_o_antigo_e_decai_parado() {
        let t0 = Instant::now();
        let mut m = RateMeter::new(Duration::from_secs(2));
        m.record(t0, 0);
        m.record(t0 + Duration::from_secs(1), 10_000_000);
        for s in 2..8 {
            m.record(t0 + Duration::from_secs(s), 10_000_000);
        }
        // Parado há vários segundos: taxa 0, não a média histórica.
        assert_eq!(m.rate_bps(), 0);
    }
}
