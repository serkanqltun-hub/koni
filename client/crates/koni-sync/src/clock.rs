//! NTP benzeri saat ofseti tahmini (docs/design/sync.md §4.2, PROTOCOL §4.6).
//!
//! `rtt = t1 − t0`, `offset = serverTs − (t0 + t1)/2`.
//! Son `window` örnekten en düşük RTT'li `best` tanesinin ofset medyanı kullanılır.

use std::collections::VecDeque;

/// Ofset tahmincisi ayarları.
#[derive(Debug, Clone, PartialEq)]
pub struct ClockConfig {
    /// Tutulan son örnek sayısı.
    pub window: usize,
    /// Medyanı alınacak en düşük RTT'li örnek sayısı.
    pub best: usize,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self { window: 8, best: 3 }
    }
}

/// Tek bir ping/pong ölçümü.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockSample {
    pub rtt_ms: f64,
    pub offset_ms: f64,
}

/// Saat ofseti tahmincisi. Tüm zamanlar ms, `f64`.
/// `server = local + offset`.
#[derive(Debug, Clone)]
pub struct ClockSync {
    cfg: ClockConfig,
    samples: VecDeque<ClockSample>,
}

/// Sıralı dilimin medyanı (boş değil varsayılır).
fn median_sorted(v: &[f64]) -> f64 {
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

impl ClockSync {
    pub fn new(cfg: ClockConfig) -> Self {
        let window = cfg.window.max(1);
        let best = cfg.best.clamp(1, window);
        Self {
            cfg: ClockConfig { window, best },
            samples: VecDeque::with_capacity(window),
        }
    }

    /// Pong geldiğinde çağrılır. `t1 < t0` veya sonlu olmayan değerler yok sayılır.
    pub fn add_sample(&mut self, t0_local_ms: f64, server_ts_ms: f64, t1_local_ms: f64) {
        if !(t0_local_ms.is_finite() && server_ts_ms.is_finite() && t1_local_ms.is_finite()) {
            return;
        }
        if t1_local_ms < t0_local_ms {
            return;
        }
        let rtt_ms = t1_local_ms - t0_local_ms;
        let offset_ms = server_ts_ms - (t0_local_ms + t1_local_ms) / 2.0;
        if !(rtt_ms.is_finite() && offset_ms.is_finite()) {
            return;
        }
        while self.samples.len() >= self.cfg.window {
            self.samples.pop_front();
        }
        self.samples.push_back(ClockSample { rtt_ms, offset_ms });
    }

    /// Toplanan geçerli örnek sayısı (pencere ile sınırlı).
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// Penceredeki ham örnekler (eskiden yeniye).
    pub fn samples(&self) -> impl Iterator<Item = &ClockSample> {
        self.samples.iter()
    }

    /// Tüm örnekleri siler.
    pub fn reset(&mut self) {
        self.samples.clear();
    }

    /// En düşük RTT'li `best` örnek (RTT'ye göre artan).
    fn best_samples(&self) -> Vec<ClockSample> {
        let mut v: Vec<ClockSample> = self.samples.iter().copied().collect();
        v.sort_by(|a, b| a.rtt_ms.total_cmp(&b.rtt_ms));
        v.truncate(self.cfg.best);
        v
    }

    /// Tahmini ofset (ms). Hiç örnek yoksa `None`.
    pub fn offset_ms(&self) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }
        let mut o: Vec<f64> = self.best_samples().iter().map(|s| s.offset_ms).collect();
        o.sort_by(|a, b| a.total_cmp(b));
        Some(median_sorted(&o))
    }

    /// Seçilen en iyi örneklerin RTT medyanı (ms).
    pub fn rtt_ms(&self) -> Option<f64> {
        if self.samples.is_empty() {
            return None;
        }
        // best_samples zaten RTT'ye göre sıralı
        let r: Vec<f64> = self.best_samples().iter().map(|s| s.rtt_ms).collect();
        Some(median_sorted(&r))
    }

    /// Penceredeki en büyük RTT (ms); planlı başlangıç payı için.
    pub fn rtt_max_ms(&self) -> Option<f64> {
        self.samples.iter().map(|s| s.rtt_ms).reduce(f64::max)
    }

    /// Yerel saatten sunucu saatine.
    pub fn to_server(&self, local_ms: f64) -> Option<f64> {
        self.offset_ms().map(|o| local_ms + o)
    }

    /// Sunucu saatinden yerel saate.
    pub fn to_local(&self, server_ms: f64) -> Option<f64> {
        self.offset_ms().map(|o| server_ms - o)
    }
}

impl Default for ClockSync {
    fn default() -> Self {
        Self::new(ClockConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Küçük deterministik LCG; [0,1) üretir.
    struct Lcg(u64);
    impl Lcg {
        fn next_f64(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
        }
    }

    #[test]
    fn empty_returns_none() {
        let c = ClockSync::default();
        assert_eq!(c.sample_count(), 0);
        assert!(c.offset_ms().is_none());
        assert!(c.rtt_ms().is_none());
        assert!(c.rtt_max_ms().is_none());
        assert!(c.to_server(1.0).is_none());
        assert!(c.to_local(1.0).is_none());
    }

    #[test]
    fn symmetric_path_exact_offset() {
        let mut c = ClockSync::default();
        // gerçek ofset 500, tek yön 30 ms
        c.add_sample(1000.0, 1000.0 + 30.0 + 500.0, 1060.0);
        assert_eq!(c.offset_ms(), Some(500.0));
        assert_eq!(c.rtt_ms(), Some(60.0));
        assert_eq!(c.rtt_max_ms(), Some(60.0));
    }

    #[test]
    fn asymmetric_delay_error_bounded_by_half_rtt() {
        let mut c = ClockSync::default();
        let truth = -250.0;
        let (up, down) = (10.0, 90.0);
        let t0 = 5000.0;
        let server = t0 + up + truth;
        let t1 = t0 + up + down;
        c.add_sample(t0, server, t1);
        let err = (c.offset_ms().unwrap() - truth).abs();
        assert!(err <= c.rtt_ms().unwrap() / 2.0 + 1e-9);
        assert!((err - 40.0).abs() < 1e-9);
    }

    #[test]
    fn high_rtt_outliers_ignored() {
        let mut c = ClockSync::new(ClockConfig { window: 8, best: 3 });
        let truth = 100.0;
        // 3 temiz örnek
        for i in 0..3 {
            let t0 = 1000.0 * (i as f64);
            c.add_sample(t0, t0 + 10.0 + truth, t0 + 20.0);
        }
        // 5 asimetrik, yüksek RTT'li aykırı
        for i in 3..8 {
            let t0 = 1000.0 * (i as f64);
            c.add_sample(t0, t0 + 400.0 + truth, t0 + 500.0);
        }
        assert_eq!(c.offset_ms(), Some(truth));
        assert_eq!(c.rtt_ms(), Some(20.0));
        assert_eq!(c.rtt_max_ms(), Some(500.0));
    }

    #[test]
    fn median_even_count_averages_middle() {
        let mut c = ClockSync::new(ClockConfig { window: 4, best: 4 });
        for (i, off) in [10.0, 20.0, 30.0, 100.0].iter().enumerate() {
            let t0 = (i as f64) * 100.0;
            c.add_sample(t0, t0 + 5.0 + off, t0 + 10.0);
        }
        assert_eq!(c.offset_ms(), Some(25.0));
    }

    #[test]
    fn window_eviction() {
        let mut c = ClockSync::new(ClockConfig { window: 3, best: 3 });
        // ilk örnek çok büyük RTT, sonra çıkarılmalı
        c.add_sample(0.0, 1000.0, 1000.0);
        assert_eq!(c.rtt_max_ms(), Some(1000.0));
        for i in 1..=3 {
            let t0 = 2000.0 * (i as f64);
            c.add_sample(t0, t0 + 5.0, t0 + 10.0);
        }
        assert_eq!(c.sample_count(), 3);
        assert_eq!(c.rtt_max_ms(), Some(10.0));
        assert_eq!(c.samples().count(), 3);
    }

    #[test]
    fn rejects_invalid_samples() {
        let mut c = ClockSync::default();
        c.add_sample(100.0, 150.0, 99.0);
        c.add_sample(f64::NAN, 150.0, 200.0);
        c.add_sample(100.0, f64::INFINITY, 200.0);
        c.add_sample(100.0, 150.0, f64::NAN);
        c.add_sample(f64::NEG_INFINITY, 150.0, 200.0);
        assert_eq!(c.sample_count(), 0);
        assert!(c.offset_ms().is_none());
        // t1 == t0 geçerli (rtt 0)
        c.add_sample(100.0, 150.0, 100.0);
        assert_eq!(c.sample_count(), 1);
        assert_eq!(c.offset_ms(), Some(50.0));
    }

    #[test]
    fn roundtrip_to_server_to_local() {
        let mut c = ClockSync::default();
        c.add_sample(1000.0, 13345.0 + 1010.0, 1020.0);
        let off = c.offset_ms().unwrap();
        assert_eq!(off, 13345.0);
        let s = c.to_server(5000.0).unwrap();
        assert_eq!(s, 5000.0 + off);
        assert!((c.to_local(s).unwrap() - 5000.0).abs() < 1e-9);
    }

    #[test]
    fn config_is_clamped() {
        let c = ClockSync::new(ClockConfig { window: 0, best: 0 });
        assert_eq!(c.cfg, ClockConfig { window: 1, best: 1 });
        let c = ClockSync::new(ClockConfig {
            window: 4,
            best: 10,
        });
        assert_eq!(c.cfg, ClockConfig { window: 4, best: 4 });
    }

    #[test]
    fn reset_clears() {
        let mut c = ClockSync::default();
        c.add_sample(0.0, 10.0, 10.0);
        c.reset();
        assert_eq!(c.sample_count(), 0);
        assert!(c.offset_ms().is_none());
    }

    fn simulate(seed: u64, truth: f64) -> ClockSync {
        let base = 20.0;
        let mut rng = Lcg(seed.wrapping_mul(0x9E3779B97F4A7C15));
        let mut c = ClockSync::default();
        for i in 0..20 {
            let t0 = 10_000.0 + 1000.0 * (i as f64);
            let up = base + rng.next_f64() * 80.0;
            let down = base + rng.next_f64() * 80.0;
            c.add_sample(t0, t0 + up + truth, t0 + up + down);
        }
        c
    }

    #[test]
    fn simulated_jitter_scenario() {
        let truth = 12345.0;
        // sabit tohum için 10 ms içinde
        let est = simulate(1, truth).offset_ms().unwrap();
        assert!((est - truth).abs() < 10.0, "est {est}");
        // tüm tohumlar için teorik sınır: hata <= rtt/2
        for seed in 1..=50u64 {
            let c = simulate(seed, truth);
            let err = (c.offset_ms().unwrap() - truth).abs();
            assert!(
                err <= c.rtt_max_ms().unwrap() / 2.0,
                "seed {seed}: err {err}"
            );
        }
        // ortalama hata küçük olmalı
        let mean: f64 = (1..=50u64)
            .map(|s| (simulate(s, truth).offset_ms().unwrap() - truth).abs())
            .sum::<f64>()
            / 50.0;
        assert!(mean < 10.0, "mean err {mean}");
    }
}
