//! Hız tabanlı sapma düzeltmesi (docs/design/sync.md §4.4): ölü bant + orantısal kontrol + yumuşak rampa.
//! Yalnızca |sapma| ≤ `hard_desync_sec` aralığında kullanılır; ötesini motor ele alır.

use crate::config::SyncConfig;

/// Sapmayı oynatma hızıyla kapatan kontrolcü.
///
/// Kurallar:
/// - `drift_sec = yerelPos − odaPos` (+ = önde → yavaşla, − = geride → hızlan).
/// - Düzeltme yokken |drift| < `deadband_sec` ise hız 1.0 kalır.
/// - |drift| ≥ `deadband_sec` olunca düzeltme başlar; |drift| < `deadband_exit_sec`
///   olunca biter ve hedef hız 1.0'a döner (histerezis).
/// - Hedef hız `1 − gain_per_sec × drift`, `[min_speed, max_speed]` ile sınırlı.
/// - Uygulanan hız hedefe en fazla `max_speed_slew_per_sec × dt` adımla yaklaşır.
/// - Uygulanan hız hiçbir zaman `[min_speed, max_speed]` dışına çıkmaz.
#[derive(Debug, Clone)]
pub struct SpeedController {
    cfg: SyncConfig,
    speed: f64,
    correcting: bool,
}

impl SpeedController {
    pub fn new(cfg: &SyncConfig) -> Self {
        Self {
            cfg: cfg.clone(),
            speed: 1.0,
            correcting: false,
        }
    }

    fn clamp_speed(&self, v: f64) -> f64 {
        v.min(self.cfg.max_speed).max(self.cfg.min_speed)
    }

    /// Yeni sapma ölçümüyle güncelle ve uygulanacak hızı döndür. `dt_sec` son
    /// güncellemeden bu yana geçen süre (rampa için).
    pub fn update(&mut self, drift_sec: f64, dt_sec: f64) -> f64 {
        if !drift_sec.is_finite() {
            return self.speed;
        }
        let abs = drift_sec.abs();
        if self.correcting {
            if abs < self.cfg.deadband_exit_sec {
                self.correcting = false;
            }
        } else if abs >= self.cfg.deadband_sec {
            self.correcting = true;
        }
        let target = if self.correcting {
            self.clamp_speed(1.0 - self.cfg.gain_per_sec * drift_sec)
        } else {
            1.0
        };
        let dt = if dt_sec.is_finite() && dt_sec > 0.0 {
            dt_sec
        } else {
            0.0
        };
        let max_step = (self.cfg.max_speed_slew_per_sec * dt).max(0.0);
        let diff = target - self.speed;
        let next = if diff.abs() <= max_step {
            target
        } else {
            self.speed + max_step * diff.signum()
        };
        let mut next = self.clamp_speed(next);
        if (next - 1.0).abs() < 1e-9 {
            next = 1.0;
        }
        self.speed = next;
        self.speed
    }

    /// Hızı anında 1.0'a çek, düzeltmeyi bitir (seek, pause, hold sonrası).
    pub fn reset(&mut self) {
        self.speed = 1.0;
        self.correcting = false;
    }

    /// Son uygulanan hız.
    pub fn speed(&self) -> f64 {
        self.speed
    }

    /// Düzeltme sürüyor mu (hız ≠ 1.0 veya histerezis bandında).
    pub fn is_correcting(&self) -> bool {
        self.correcting || self.speed != 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns (convergence_time, toggles, max_overshoot, min_speed, max_speed, final_speed, final_drift)
    fn simulate(cfg: &SyncConfig, start: f64) -> (f64, usize, f64, f64, f64, f64, f64) {
        let mut c = SpeedController::new(cfg);
        let dt = 0.25;
        let mut drift = start;
        let mut t = 0.0;
        let mut conv: Option<f64> = None;
        let mut toggles = 0;
        let mut prev = false;
        let mut overshoot: f64 = 0.0;
        let (mut smin, mut smax) = (f64::MAX, f64::MIN);
        while t < 300.0 {
            let sp = c.update(drift, dt);
            smin = smin.min(sp);
            smax = smax.max(sp);
            let cor = c.is_correcting();
            if cor != prev {
                toggles += 1;
                prev = cor;
            }
            drift += (sp - 1.0) * dt;
            t += dt;
            if drift.signum() != start.signum() {
                overshoot = overshoot.max(drift.abs());
            }
            if conv.is_none() && drift.abs() < cfg.deadband_sec {
                conv = Some(t);
            }
        }
        (
            conv.unwrap_or(f64::INFINITY),
            toggles,
            overshoot,
            smin,
            smax,
            c.speed(),
            drift,
        )
    }

    fn check(cfg: &SyncConfig, name: &str) {
        for start in [0.5, -1.0, 3.0, -3.0] {
            let (conv, toggles, over, smin, smax, fin, fd) = simulate(cfg, start);
            eprintln!("{name}: start {start:+.1}s -> converged (<{} ms) in {conv:.2}s, toggles {toggles}, overshoot {:.1} ms", cfg.deadband_sec * 1000.0, over * 1000.0);
            assert!(conv.is_finite(), "no convergence from {start}");
            assert!(fd.abs() < cfg.deadband_sec, "final drift {fd}");
            assert!(smin >= cfg.min_speed - 1e-12 && smax <= cfg.max_speed + 1e-12);
            assert!(over <= 0.05, "overshoot {over}");
            assert_eq!(fin, 1.0);
            assert!(toggles <= 2, "chattering: {toggles}");
        }
    }

    #[test]
    fn closed_loop_default() {
        check(&SyncConfig::default(), "default");
    }

    #[test]
    fn closed_loop_lounge() {
        let cfg = SyncConfig::lounge();
        assert_eq!((cfg.min_speed, cfg.max_speed), (0.98, 1.02));
        check(&cfg, "lounge");
    }

    #[test]
    fn deadband_no_correction() {
        let mut c = SpeedController::new(&SyncConfig::default());
        for d in [0.0, 0.049, -0.049] {
            assert_eq!(c.update(d, 0.25), 1.0);
            assert!(!c.is_correcting());
        }
    }

    #[test]
    fn hysteresis() {
        let mut c = SpeedController::new(&SyncConfig::default());
        c.update(0.06, 0.25);
        assert!(c.is_correcting());
        c.update(0.03, 0.25); // between exit and enter: stay correcting
        assert!(c.speed() < 1.0);
        for _ in 0..20 {
            c.update(0.01, 0.25);
        }
        assert_eq!(c.speed(), 1.0);
        assert!(!c.is_correcting());
    }

    #[test]
    fn slew_limited_and_signs() {
        let mut c = SpeedController::new(&SyncConfig::default());
        let s = c.update(2.0, 0.25);
        assert!((s - 0.99).abs() < 1e-12);
        let mut c = SpeedController::new(&SyncConfig::default());
        assert!(c.update(-2.0, 0.25) > 1.0);
    }

    #[test]
    fn reset_and_bad_input() {
        let mut c = SpeedController::new(&SyncConfig::default());
        for _ in 0..10 {
            c.update(1.0, 0.25);
        }
        let held = c.speed();
        assert!(held < 1.0);
        assert_eq!(c.update(f64::NAN, 0.25), held);
        assert_eq!(c.update(f64::INFINITY, 0.25), held);
        assert!(c.update(1.0, f64::NAN).is_finite());
        assert!(c.update(1.0, -1.0).is_finite());
        assert!(c.update(1.0, 0.0).is_finite());
        c.reset();
        assert_eq!(c.speed(), 1.0);
        assert!(!c.is_correcting());
    }
}
