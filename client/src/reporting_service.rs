use log::info;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::interval;

/// Message updates from clients
#[derive(Debug)]
pub struct ReportUpdate {
    pub count: u64,
}

pub struct ReportingService {
    rx: mpsc::Receiver<ReportUpdate>,
    max_total: u64,
}

impl ReportingService {
    pub fn new(
        rx: mpsc::Receiver<ReportUpdate>,
        max_total: u64,
    ) -> Self {
        Self { rx, max_total }
    }

    /// entry point for our actor
    pub fn run(
        mut self,
        handle: tokio::runtime::Handle,
    ) -> tokio::task::JoinHandle<Result<u64, Box<dyn std::error::Error + Send + Sync>>>
    {
        handle.spawn(async move {
            let mut total: u64 = 0;
            let mut interval = interval(Duration::from_secs(5));
            let start = Instant::now();

            loop {
                tokio::select! {
                        Some(update) = self.rx.recv() => {
                            total += update.count;
                            if total >= self.max_total {
                                info!("[Report] total={} (reached max_total={}), reporter exiting",
                                      total, self.max_total);
                                break;
                            }
                        }
                        _ = interval.tick() => {
                            let elapsed = start.elapsed().as_secs_f64();
                            if elapsed > 0.0 {
                                let rate = total as f64 / elapsed;
                                let pct = (total as f64 / self.max_total as f64) * 100.0;
                                info!("[Report] total={} → {:.2} echoes/sec ({:.1}% complete)",
                                      total, rate, pct.min(100.0));
                            }
                        }
                        else => {
                            break;
                        }
                    }
            }

            Ok(total)
        })
    }
}