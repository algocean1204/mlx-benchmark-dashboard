//! 메모리 한계 감시·즉시 강제 종료
//!
//! IN: `ResourceSample` 스트림, 한계 설정(soft/hard)
//! OUT: watchdog 이벤트(Warn/Kill), lifecycle에 Abort 명령

use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use crate::events::CoreEvent;
use crate::lifecycle::Command;
use crate::monitor::ResourceSample;
use crate::sys_memory;

#[derive(Debug, Clone)]
pub struct WatchdogLimits {
    pub soft_bytes: u64,
    pub hard_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogEvent {
    Warn,
    Kill,
}

pub fn compute_limits(mem_limit_gb: Option<f64>) -> WatchdogLimits {
    let hard_bytes = match mem_limit_gb {
        Some(gb) => (gb * 1024.0 * 1024.0 * 1024.0) as u64,
        None => sys_memory::watchdog_default_hard_bytes(),
    };
    let soft_bytes = (hard_bytes as f64 * 0.90) as u64;
    WatchdogLimits {
        soft_bytes,
        hard_bytes,
    }
}

pub fn spawn_watchdog(
    mut sample_rx: broadcast::Receiver<CoreEvent>,
    event_tx: broadcast::Sender<CoreEvent>,
    command_tx: mpsc::Sender<Command>,
    limits: WatchdogLimits,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut soft_warned = false;

        loop {
            let event = match sample_rx.recv().await {
                Ok(event) => event,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            };

            let CoreEvent::Sample(sample) = event else {
                continue;
            };

            if sample.phys_footprint_bytes >= limits.hard_bytes {
                let _ = event_tx.send(CoreEvent::WatchdogKill);
                let _ = command_tx
                    .send(Command::Abort {
                        reason: "watchdog hard limit exceeded".into(),
                    })
                    .await;
                break;
            }

            if sample.phys_footprint_bytes >= limits.soft_bytes {
                if !soft_warned {
                    let _ = event_tx.send(CoreEvent::WatchdogWarn);
                    soft_warned = true;
                }
            } else {
                soft_warned = false;
            }
        }
    })
}

pub fn evaluate_sample(sample: &ResourceSample, limits: &WatchdogLimits) -> Option<WatchdogEvent> {
    if sample.phys_footprint_bytes >= limits.hard_bytes {
        Some(WatchdogEvent::Kill)
    } else if sample.phys_footprint_bytes >= limits.soft_bytes {
        Some(WatchdogEvent::Warn)
    } else {
        None
    }
}