//! tokio broadcast 기반 통합 이벤트 버스 타입 정의
//!
//! IN: —
//! OUT: `CoreEvent` enum (StateChanged, Sample, Token, WatchdogWarn, WatchdogKill, RunFinished, Log)

use serde::Serialize;

use crate::lifecycle::LifecycleState;
use crate::monitor::ResourceSample;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CoreEvent {
    StateChanged {
        from: LifecycleState,
        to: LifecycleState,
    },
    Sample(ResourceSample),
    Token {
        index: u32,
        text: String,
    },
    WatchdogWarn,
    WatchdogKill,
    RunFinished {
        run_id: u64,
        status: String,
    },
    Log {
        level: String,
        message: String,
    },
}