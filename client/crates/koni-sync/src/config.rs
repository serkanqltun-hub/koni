//! Senkron eşikleri. Değerler docs/design/sync.md §4.4 ve §4.7'de (v0.3.1) kilitlidir.

/// Motorun tüm eşikleri. `Default` plandaki film modu değerleridir.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncConfig {
    /// Bu sapmanın altında düzeltme yok (s). Plan: 50 ms.
    pub deadband_sec: f64,
    /// Düzeltme başladıktan sonra hız bu sapmanın altına inince 1.0'a döner (s).
    /// Ölü banttan küçük tutulur, eşikte gidip gelmeyi önler.
    pub deadband_exit_sec: f64,
    /// En düşük oynatma hızı. Plan: 0.96.
    pub min_speed: f64,
    /// En yüksek oynatma hızı. Plan: 1.04.
    pub max_speed: f64,
    /// Orantısal kazanç: `speed = 1 − gain × sapma` (sapma saniye, + = önde).
    pub gain_per_sec: f64,
    /// Hızın saniyede en fazla ne kadar değişebileceği (yumuşak rampa).
    pub max_speed_slew_per_sec: f64,
    /// Ciddi desync eşiği (s). Plan: 3.0 s (sabit).
    pub hard_desync_sec: f64,
    /// Bu kadar geride kalan istemci ileri seek ile atlar (s). Plan: 30 s.
    pub skip_forward_sec: f64,
    /// Stall sonrası devam için gereken rezerv (s). Plan: 12 s (sabit).
    pub reserve_sec: f64,
    /// Bu tamponun altında ve konum ilerlemiyorsa stall sayılır (s). Plan: ~0.5 s.
    pub stall_low_water_sec: f64,
    /// Kronik stall: `chronic_window_sec` içinde `chronic_stall_count` stall. Plan: 4 dk'da 3.
    pub chronic_stall_count: usize,
    pub chronic_window_sec: f64,
    /// Değerlendirme aralığı (ms). Plan: 250–500 ms.
    pub tick_ms: f64,
    /// Rapor aralığı (ms). Protokol: ~1 sn.
    pub report_interval_ms: f64,
    /// Kararlı durumda ping aralığı (ms). Plan: 2–5 sn.
    pub ping_interval_ms: f64,
    /// Bağlantının başında hızlı ping sayısı ve aralığı (ofset çabuk otursun diye).
    pub warmup_pings: usize,
    pub warmup_ping_interval_ms: f64,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            deadband_sec: 0.050,
            deadband_exit_sec: 0.020,
            min_speed: 0.96,
            max_speed: 1.04,
            gain_per_sec: 0.10,
            max_speed_slew_per_sec: 0.04,
            hard_desync_sec: 3.0,
            skip_forward_sec: 30.0,
            reserve_sec: 12.0,
            stall_low_water_sec: 0.5,
            chronic_stall_count: 3,
            chronic_window_sec: 240.0,
            tick_ms: 250.0,
            report_interval_ms: 1000.0,
            ping_interval_ms: 3000.0,
            warmup_pings: 5,
            warmup_ping_interval_ms: 300.0,
        }
    }
}

impl SyncConfig {
    /// Lounge (müzik) modu: daha geniş ölü bant ve daha dar hız aralığı (docs/design/media.md §6.3).
    pub fn lounge() -> Self {
        Self {
            deadband_sec: 0.150,
            deadband_exit_sec: 0.060,
            min_speed: 0.98,
            max_speed: 1.02,
            ..Self::default()
        }
    }
}
