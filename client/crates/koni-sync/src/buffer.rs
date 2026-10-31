//! Stall algılama, 12 s histerezis rezervi ve kronik stall sayacı (docs/design/sync.md §4.7).

use std::collections::VecDeque;

use crate::config::SyncConfig;

/// Stall algılayıcı. Konum ilerlemiyorken `paused_for_cache` veya
/// `cache_ahead < stall_low_water_sec` ise stall sayılır.
/// Oynatıcı bilinçli olarak duraklatılmışsa (paused) stall sayılmaz.
#[derive(Debug, Clone)]
pub struct StallDetector {
    low_water: f64,
    last_position: Option<f64>,
}

impl StallDetector {
    pub fn new(cfg: &SyncConfig) -> Self {
        Self {
            low_water: cfg.stall_low_water_sec,
            last_position: None,
        }
    }

    /// Her tick'te çağrılır. `expect_progress`: motor oynatıcının oynamasını bekliyor mu
    /// (pause/hold/countdown'da false). Stall varsa `true`.
    ///
    /// `paused_for_cache == true` ise anında stall. Değilse: `expect_progress` iken konum
    /// son çağrıdan beri ilerlememiş (< 1 ms) ve `cache_ahead < low_water` ise stall.
    /// Dosya sonundaysa (`eof`) asla stall değildir.
    pub fn update(
        &mut self,
        position: f64,
        cache_ahead: f64,
        paused_for_cache: bool,
        eof: bool,
        expect_progress: bool,
    ) -> bool {
        let prev = self.last_position.replace(position);
        if eof {
            return false;
        }
        if paused_for_cache {
            return true;
        }
        if !expect_progress {
            return false;
        }
        match prev {
            Some(p) => (position - p) < 0.001 && cache_ahead < self.low_water,
            None => false,
        }
    }

    /// Seek sonrası konum sıçramasının "ilerleme" sanılmaması için sıfırla.
    pub fn reset(&mut self) {
        self.last_position = None;
    }
}

/// Histerezis kapısı: stall sonrası devam için rezerv koşulu.
#[derive(Debug, Clone)]
pub struct ReserveGate {
    reserve_sec: f64,
}

impl ReserveGate {
    pub fn new(cfg: &SyncConfig) -> Self {
        Self {
            reserve_sec: cfg.reserve_sec,
        }
    }

    fn eof_buffered(position: f64, cache_ahead: f64, duration: Option<f64>) -> bool {
        matches!(duration, Some(d) if position + cache_ahead >= d - 0.05)
    }

    /// Rezerv doldu mu: `cache_ahead ≥ reserve_sec`, ya da dosya sonu tampondaysa
    /// (`duration` biliniyor ve `position + cache_ahead ≥ duration − 0.05`).
    pub fn is_satisfied(&self, position: f64, cache_ahead: f64, duration: Option<f64>) -> bool {
        cache_ahead >= self.reserve_sec || Self::eof_buffered(position, cache_ahead, duration)
    }

    /// Rapor için yüzde: `min(100, cache_ahead / reserve_sec × 100)`; dosya sonu
    /// tampondaysa 100.
    pub fn percent(&self, position: f64, cache_ahead: f64, duration: Option<f64>) -> u8 {
        if self.is_satisfied(position, cache_ahead, duration) {
            return 100;
        }
        let p = (cache_ahead / self.reserve_sec * 100.0).floor();
        if !p.is_finite() {
            return 0;
        }
        p.clamp(0.0, 100.0) as u8
    }

    pub fn target_sec(&self) -> f64 {
        self.reserve_sec
    }
}

/// "4 dk'da 3 stall" sayacı (kayan pencere). Sunucu otoritedir; bu yerel kopya UI ve
/// telemetri içindir.
#[derive(Debug, Clone)]
pub struct StallHistory {
    window_sec: f64,
    threshold: usize,
    stalls: VecDeque<f64>,
}

impl StallHistory {
    pub fn new(cfg: &SyncConfig) -> Self {
        Self {
            window_sec: cfg.chronic_window_sec,
            threshold: cfg.chronic_stall_count,
            stalls: VecDeque::new(),
        }
    }

    /// Yeni stall başlangıcını kaydet (`now_sec` monoton saniye). Pencere içindeki
    /// stall sayısı eşiğe ulaştıysa `true` (kronik).
    pub fn record(&mut self, now_sec: f64) -> bool {
        self.stalls.push_back(now_sec);
        let window = self.window_sec;
        self.stalls.retain(|&t| now_sec - t <= window);
        self.stalls.len() >= self.threshold
    }

    /// `now_sec` anında pencere içindeki stall sayısı.
    pub fn count_in_window(&self, now_sec: f64) -> usize {
        self.stalls
            .iter()
            .filter(|&&t| now_sec - t <= self.window_sec)
            .count()
    }

    pub fn clear(&mut self) {
        self.stalls.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> SyncConfig {
        SyncConfig::default()
    }

    #[test]
    fn stall_via_paused_for_cache() {
        let mut d = StallDetector::new(&cfg());
        assert!(d.update(10.0, 5.0, true, false, true));
    }

    #[test]
    fn stall_via_no_progress_and_low_cache() {
        let mut d = StallDetector::new(&cfg());
        assert!(!d.update(10.0, 0.1, false, false, true)); // ilk çağrı
        assert!(d.update(10.0, 0.1, false, false, true));
    }

    #[test]
    fn no_stall_when_not_expecting_progress() {
        let mut d = StallDetector::new(&cfg());
        assert!(!d.update(10.0, 0.0, false, false, false));
        assert!(!d.update(10.0, 0.0, false, false, false));
    }

    #[test]
    fn no_stall_when_progressing_even_with_low_cache() {
        let mut d = StallDetector::new(&cfg());
        d.update(10.0, 0.1, false, false, true);
        assert!(!d.update(10.1, 0.1, false, false, true));
        assert!(!d.update(10.2, 0.0, false, false, true));
    }

    #[test]
    fn no_stall_with_no_progress_but_enough_cache() {
        let mut d = StallDetector::new(&cfg());
        d.update(10.0, 5.0, false, false, true);
        assert!(!d.update(10.0, 5.0, false, false, true));
    }

    #[test]
    fn eof_never_stalls() {
        let mut d = StallDetector::new(&cfg());
        d.update(10.0, 0.0, false, true, true);
        assert!(!d.update(10.0, 0.0, false, true, true));
        assert!(!d.update(10.0, 0.0, true, true, true));
    }

    #[test]
    fn reset_prevents_seek_confusion() {
        let mut d = StallDetector::new(&cfg());
        d.update(10.0, 0.1, false, false, true);
        d.reset();
        // seek sonrası aynı konum, düşük cache: ilk çağrı stall sayılmaz
        assert!(!d.update(10.0, 0.1, false, false, true));
        assert!(d.update(10.0, 0.1, false, false, true));
    }

    #[test]
    fn paused_resume_records_position() {
        let mut d = StallDetector::new(&cfg());
        d.update(10.0, 0.0, false, false, false);
        // expect_progress açılınca önceki konum biliniyor ve değişmedi
        assert!(d.update(10.0, 0.0, false, false, true));
    }

    #[test]
    fn reserve_gate_threshold() {
        let g = ReserveGate::new(&cfg());
        assert!(!g.is_satisfied(0.0, 11.9, Some(600.0)));
        assert!(g.is_satisfied(0.0, 12.0, Some(600.0)));
        assert!(!g.is_satisfied(0.0, 11.9, None));
        assert_eq!(g.target_sec(), 12.0);
    }

    #[test]
    fn reserve_gate_eof_satisfies() {
        let g = ReserveGate::new(&cfg());
        assert!(g.is_satisfied(98.0, 1.96, Some(100.0)));
        assert!(!g.is_satisfied(98.0, 1.9, Some(100.0)));
        assert_eq!(g.percent(98.0, 1.96, Some(100.0)), 100);
    }

    #[test]
    fn reserve_gate_percent() {
        let g = ReserveGate::new(&cfg());
        assert_eq!(g.percent(0.0, 7.8, Some(600.0)), 65);
        assert_eq!(g.percent(0.0, 0.0, None), 0);
        assert_eq!(g.percent(0.0, 11.9, None), 99);
        assert_eq!(g.percent(0.0, 12.0, None), 100);
        assert_eq!(g.percent(0.0, 50.0, None), 100);
        assert_eq!(g.percent(0.0, f64::NAN, None), 0);
        assert_eq!(g.percent(0.0, -3.0, None), 0);
    }

    #[test]
    fn chronic_three_in_window() {
        let mut h = StallHistory::new(&cfg());
        assert!(!h.record(0.0));
        assert!(!h.record(100.0));
        assert!(h.record(239.0));
        assert_eq!(h.count_in_window(239.0), 3);
    }

    #[test]
    fn chronic_spread_over_300s() {
        let mut h = StallHistory::new(&cfg());
        assert!(!h.record(0.0));
        assert!(!h.record(150.0));
        assert!(!h.record(300.0));
        assert_eq!(h.count_in_window(300.0), 2);
    }

    #[test]
    fn chronic_window_boundary_inclusive() {
        let mut h = StallHistory::new(&cfg());
        h.record(0.0);
        h.record(100.0);
        assert!(h.record(240.0));
        assert_eq!(h.count_in_window(240.0), 3);
        assert_eq!(h.count_in_window(240.5), 2); // t=0 pencere dışına çıktı
    }

    #[test]
    fn history_clear() {
        let mut h = StallHistory::new(&cfg());
        h.record(1.0);
        h.record(2.0);
        h.clear();
        assert_eq!(h.count_in_window(3.0), 0);
        assert!(!h.record(4.0));
    }
}
