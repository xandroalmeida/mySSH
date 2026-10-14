//! Formatação legível (pt-BR): tamanho, taxa, ETA e data.

use std::time::SystemTime;

use chrono::{DateTime, Local, TimeZone};

fn decimal(v: f64) -> String {
    format!("{v:.1}").replace('.', ",")
}

/// `0 B`, `512 B`, `1,5 KB`, `12,3 MB`... (base 1024).
pub fn fmt_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut v = bytes as f64 / 1024.0;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    // evita "1024,0 KB" por arredondamento
    if decimal(v) == "1024,0" && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    format!("{} {}", decimal(v), UNITS[u])
}

/// Bytes por segundo; `0` vira `—` (ainda sem medida).
pub fn fmt_rate(bps: u64) -> String {
    if bps == 0 {
        "—".into()
    } else {
        format!("{}/s", fmt_size(bps))
    }
}

/// Tempo restante estimado; `None` quando a taxa é zero.
pub fn eta_secs(done: u64, total: u64, rate_bps: u64) -> Option<u64> {
    if rate_bps == 0 || total == 0 {
        return None;
    }
    Some(total.saturating_sub(done).div_ceil(rate_bps))
}

pub fn fmt_duration(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 {
        format!("{h} h {m:02} min")
    } else if m > 0 {
        format!("{m} min {s:02} s")
    } else {
        format!("{s} s")
    }
}

pub fn fmt_eta(done: u64, total: u64, rate_bps: u64) -> String {
    match eta_secs(done, total, rate_bps) {
        Some(s) => fmt_duration(s),
        None => "—".into(),
    }
}

/// Percentual 0..=100 (total zero = 0).
pub fn percent(done: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (done.min(total) as f64 / total as f64 * 100.0) as f32
    }
}

pub fn fmt_datetime<Tz: TimeZone>(t: SystemTime, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    DateTime::<chrono::Utc>::from(t)
        .with_timezone(tz)
        .format("%d/%m/%Y %H:%M")
        .to_string()
}

/// Data no fuso local; `—` quando desconhecida.
pub fn fmt_mtime(t: Option<SystemTime>) -> String {
    t.map_or_else(|| "—".into(), |t| fmt_datetime(t, &Local))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn tamanhos() {
        assert_eq!(fmt_size(0), "0 B");
        assert_eq!(fmt_size(1023), "1023 B");
        assert_eq!(fmt_size(1024), "1,0 KB");
        assert_eq!(fmt_size(1536), "1,5 KB");
        assert_eq!(fmt_size(5 * 1024 * 1024), "5,0 MB");
        assert_eq!(
            fmt_size(3 * 1024 * 1024 * 1024 + 512 * 1024 * 1024),
            "3,5 GB"
        );
        assert_eq!(fmt_size(1024 * 1024 - 1), "1,0 MB"); // sem "1024,0 KB"
        assert!(fmt_size(u64::MAX).ends_with("PB"));
    }

    #[test]
    fn taxa_e_eta() {
        assert_eq!(fmt_rate(0), "—");
        assert_eq!(fmt_rate(2 * 1024 * 1024), "2,0 MB/s");
        assert_eq!(eta_secs(0, 100, 0), None);
        assert_eq!(eta_secs(0, 0, 10), None);
        assert_eq!(eta_secs(50, 100, 10), Some(5));
        assert_eq!(eta_secs(0, 101, 10), Some(11)); // arredonda para cima
        assert_eq!(eta_secs(200, 100, 10), Some(0));
        assert_eq!(fmt_duration(7), "7 s");
        assert_eq!(fmt_duration(65), "1 min 05 s");
        assert_eq!(fmt_duration(3725), "1 h 02 min");
        assert_eq!(fmt_eta(0, 100, 0), "—");
    }

    #[test]
    fn percentual() {
        assert_eq!(percent(0, 0), 0.0);
        assert_eq!(percent(50, 200), 25.0);
        assert_eq!(percent(300, 200), 100.0);
    }

    #[test]
    fn data_em_fuso_dado() {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(fmt_datetime(t, &chrono::Utc), "14/11/2023 22:13");
        assert_eq!(fmt_mtime(None), "—");
    }
}
