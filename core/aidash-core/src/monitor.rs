//! 250ms 주기 리소스 샘플링: 프로세스 트리 phys_footprint(libproc `proc_pid_rusage`),
//! CPU%(sysinfo), MLX 메모리(Python `/metrics` 폴링), 옵션 powermetrics(전력·온도·스로틀링)
//!
//! 틱 주기는 `sysinfo::MINIMUM_CPU_UPDATE_INTERVAL`(macOS 200ms)보다 커야 한다 — 그보다
//! 짧으면 sysinfo가 CPU 시간 델타(분자)는 매번 새로 재면서 경과시간(분모)은 캐시된 값을
//! 재사용해, 매 틱마다 cpu_pct가 실제와 무관하게 톱니처럼 튀는 계측 아티팩트가 생긴다.
//!
//! IN: 대상 pid 트리, 폴링 주기
//! OUT: `ResourceSample` 스트림

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use libproc::libproc::pid_rusage::{pidrusage, RUsageInfoV4};
use reqwest::Client;
use serde::Serialize;
use sysinfo::{Pid, ProcessesToUpdate, System, MINIMUM_CPU_UPDATE_INTERVAL};
use tokio::sync::broadcast;
use tokio::time::{interval, Duration, MissedTickBehavior};

/// sysinfo가 매 틱마다 신선한 CPU 시간 델타를 계산하도록, macOS 최소 갱신 간격보다
/// 여유 있게 큰 틱 주기를 쓴다 (짧으면 분모가 캐시된 값으로 튀는 계측 아티팩트 발생).
const SAMPLE_TICK: Duration = Duration::from_millis(
    MINIMUM_CPU_UPDATE_INTERVAL.as_millis() as u64 + 50,
);

use crate::client::fetch_metrics;
use crate::events::CoreEvent;
use crate::sys_memory;

static MONOTONIC_ORIGIN: OnceLock<Instant> = OnceLock::new();

pub fn monotonic_ms() -> u64 {
    MONOTONIC_ORIGIN
        .get_or_init(Instant::now)
        .elapsed()
        .as_millis() as u64
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourceSample {
    pub ts: u64,
    pub phys_footprint_bytes: u64,
    pub mlx_active_bytes: Option<u64>,
    pub cpu_pct: f64,
    pub sys_available_bytes: u64,
    pub power_w: Option<f64>,
    pub temp_c: Option<f64>,
    pub throttled: Option<bool>,
}

pub fn sample_system() -> ResourceSample {
    let mut system = System::new();
    system.refresh_memory();
    system.refresh_cpu_usage();

    ResourceSample {
        ts: monotonic_ms(),
        phys_footprint_bytes: 0,
        mlx_active_bytes: None,
        cpu_pct: system.global_cpu_usage() as f64,
        sys_available_bytes: sys_memory::system_available_bytes(),
        power_w: None,
        temp_c: None,
        throttled: None,
    }
}

fn sample_process_tree_from_system(system: &mut System, tree: &[u32]) -> ResourceSample {
    let phys_footprint_bytes = tree
        .iter()
        .map(|pid| phys_footprint_for_pid(*pid as i32))
        .sum();
    let cpu_pct = tree
        .iter()
        .filter_map(|pid| system.process(Pid::from_u32(*pid)))
        .map(|p| p.cpu_usage() as f64)
        .sum();

    ResourceSample {
        ts: monotonic_ms(),
        phys_footprint_bytes,
        mlx_active_bytes: None,
        cpu_pct,
        sys_available_bytes: sys_memory::system_available_bytes(),
        power_w: None,
        temp_c: None,
        throttled: None,
    }
}

pub fn total_system_memory_bytes() -> u64 {
    sys_memory::total_system_memory_bytes()
}

fn phys_footprint_for_pid(pid: i32) -> u64 {
    pidrusage::<RUsageInfoV4>(pid)
        .map(|info| info.ri_phys_footprint)
        .unwrap_or(0)
}

fn collect_process_tree(root_pid: u32, system: &System) -> Vec<u32> {
    let mut children_by_parent: HashMap<u32, Vec<u32>> = HashMap::new();
    for (pid, process) in system.processes() {
        if let Some(parent) = process.parent() {
            children_by_parent
                .entry(parent.as_u32())
                .or_default()
                .push(pid.as_u32());
        }
    }

    let mut tree = vec![root_pid];
    let mut queue = vec![root_pid];
    while let Some(pid) = queue.pop() {
        if let Some(children) = children_by_parent.get(&pid) {
            for child in children {
                if !tree.contains(child) {
                    tree.push(*child);
                    queue.push(*child);
                }
            }
        }
    }
    tree
}

/// `/metrics` HTTP 폴링을 메인 샘플링 루프와 별도 태스크로 분리해서 돌린다.
/// 예전엔 메인 루프 안에서 이 호출을 직접 `.await`해서, python adapter가
/// 바빠 응답이 늦어지면(최대 800ms 타임아웃) 리소스 샘플링 전체가 그만큼
/// 통째로 멈췄다 — 실측으로 1초 넘게 샘플이 끊기는 구간을 확인함. `alive_weak`가
/// 부모(메인 루프) 생존 여부를 감지해, 부모가 abort되면 이 태스크도 뒤따라 종료한다.
fn spawn_metrics_poller(
    port: u16,
    last_mlx: Arc<std::sync::Mutex<Option<u64>>>,
    alive_weak: std::sync::Weak<()>,
) {
    tokio::spawn(async move {
        let http = Client::builder()
            .timeout(Duration::from_millis(800))
            .build()
            .unwrap_or_else(|_| Client::new());
        let mut ticker = interval(Duration::from_millis(500));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if alive_weak.upgrade().is_none() {
                break;
            }
            if let Ok(metrics) = fetch_metrics(&http, port).await {
                *last_mlx.lock().unwrap() = Some(metrics.mlx_active_bytes);
            }
        }
    });
}

pub fn spawn_process_monitor(
    root_pid: u32,
    metrics_port: Option<u16>,
    event_tx: broadcast::Sender<CoreEvent>,
) -> tokio::task::JoinHandle<()> {
    let alive = Arc::new(());
    let last_mlx: Arc<std::sync::Mutex<Option<u64>>> = Arc::new(std::sync::Mutex::new(None));
    if let Some(port) = metrics_port {
        spawn_metrics_poller(port, last_mlx.clone(), Arc::downgrade(&alive));
    }

    tokio::spawn(async move {
        let _alive = alive;
        let mut ticker = interval(SAMPLE_TICK);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

        let mut system = System::new();
        system.refresh_memory();
        system.refresh_cpu_usage();
        system.refresh_processes(ProcessesToUpdate::All, true);
        let mut tree = collect_process_tree(root_pid, &system);
        let mut last_tree_refresh = Instant::now();

        loop {
            ticker.tick().await;

            if last_tree_refresh.elapsed() >= Duration::from_secs(1) {
                system.refresh_processes(ProcessesToUpdate::All, true);
                tree = collect_process_tree(root_pid, &system);
                last_tree_refresh = Instant::now();
            } else {
                let pids: Vec<Pid> = tree.iter().copied().map(Pid::from_u32).collect();
                system.refresh_processes(ProcessesToUpdate::Some(&pids), true);
            }
            system.refresh_cpu_usage();
            system.refresh_memory();

            let mut sample = sample_process_tree_from_system(&mut system, &tree);

            if metrics_port.is_some() {
                sample.mlx_active_bytes = *last_mlx.lock().unwrap();
            }

            let _ = event_tx.send(CoreEvent::Sample(sample));
        }
    })
}

pub async fn run_system_monitor_loop<F>(mut on_sample: F) -> Result<(), std::io::Error>
where
    F: FnMut(ResourceSample) -> bool,
{
    let mut ticker = interval(SAMPLE_TICK);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        let sample = sample_system();
        if !on_sample(sample) {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_tick_exceeds_sysinfo_minimum_update_interval() {
        // 이 여유가 없으면 sysinfo가 매 틱마다 캐시된 time_interval을 재사용해
        // cpu_pct가 톱니처럼 튀는 계측 아티팩트가 재발한다.
        assert!(SAMPLE_TICK > MINIMUM_CPU_UPDATE_INTERVAL);
    }
}