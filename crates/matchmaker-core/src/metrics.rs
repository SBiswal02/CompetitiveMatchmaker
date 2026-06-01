//! Low-overhead health metrics for the hot matching loop.

use std::sync::atomic::{AtomicU64, Ordering};

use matchmaker_types::HealthSnapshot;
use parking_lot::Mutex;

/// Atomics on the scan path; histogram / rolling averages updated in bulk.
#[derive(Debug, Default)]
pub struct MatchmakerMetrics {
    queue_depth: AtomicU64,
    matches_formed: AtomicU64,
    scans: AtomicU64,
    evictions: AtomicU64,
    scan_duration_nanos: AtomicU64,
    scan_count_for_avg: AtomicU64,
    /// Exponential moving average of match quality (updated outside hot path).
    quality_ema_bits: AtomicU64,
    /// P99 scan latency in microseconds (approximate, lock-protected histogram).
    p99_scan_micros: Mutex<u64>,
    recent_scan_micros: Mutex<Vec<u64>>,
}

const EMA_ALPHA: f64 = 0.1;

impl MatchmakerMetrics {
    pub fn set_queue_depth(&self, depth: u64) {
        self.queue_depth.store(depth, Ordering::Relaxed);
    }

    pub fn record_scan(&self, duration_nanos: u64) {
        self.scans.fetch_add(1, Ordering::Relaxed);
        self.scan_duration_nanos
            .fetch_add(duration_nanos, Ordering::Relaxed);
        self.scan_count_for_avg.fetch_add(1, Ordering::Relaxed);

        let micros = duration_nanos / 1000;
        let mut recent = self.recent_scan_micros.lock();
        recent.push(micros);
        if recent.len() > 512 {
            recent.drain(0..256);
        }
        if recent.len() >= 10 {
            let mut sorted = recent.clone();
            sorted.sort_unstable();
            let idx = (sorted.len() as f64 * 0.99) as usize;
            let p99 = sorted[idx.min(sorted.len() - 1)];
            *self.p99_scan_micros.lock() = p99;
        }
    }

    pub fn record_match(&self, quality: f64) {
        self.matches_formed.fetch_add(1, Ordering::Relaxed);
        self.update_quality_ema(quality);
    }

    pub fn record_evictions(&self, count: u64) {
        self.evictions.fetch_add(count, Ordering::Relaxed);
    }

    fn update_quality_ema(&self, quality: f64) {
        let current = f64::from_bits(self.quality_ema_bits.load(Ordering::Relaxed));
        let updated = if current == 0.0 {
            quality
        } else {
            EMA_ALPHA * quality + (1.0 - EMA_ALPHA) * current
        };
        self.quality_ema_bits
            .store(updated.to_bits(), Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> HealthSnapshot {
        let scans = self.scans.load(Ordering::Relaxed);
        let total_nanos = self.scan_duration_nanos.load(Ordering::Relaxed);
        let _ = if scans > 0 {
            total_nanos / scans
        } else {
            0
        };

        HealthSnapshot {
            queue_depth: self.queue_depth.load(Ordering::Relaxed),
            matches_formed_total: self.matches_formed.load(Ordering::Relaxed),
            scans_total: scans,
            evictions_total: self.evictions.load(Ordering::Relaxed),
            avg_match_quality: f64::from_bits(self.quality_ema_bits.load(Ordering::Relaxed)),
            p99_scan_micros: *self.p99_scan_micros.lock(),
        }
    }
}
