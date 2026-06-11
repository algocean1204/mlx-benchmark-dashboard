//! 100ms 주기 리소스 샘플링: 프로세스 트리 phys_footprint(libproc `proc_pid_rusage`),
//! CPU%(sysinfo), MLX 메모리(Python `/metrics` 폴링), 옵션 powermetrics(전력·온도·스로틀링)
//!
//! IN: 대상 pid 트리, 폴링 주기
//! OUT: `ResourceSample` 스트림

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Instant;

use libproc::libproc::pid_rusage::{pidrusage, RUsageInfoV4};
use reqwest::Client;
use serde::Serialize;
use sysinfo::{Pid, ProcessesToUpdate, System};
use tokio::sync::broadcast;
use tokio::time::{interval, Duration, MissedTickBehavior};

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

pub fn spawn_process_monitor(
    root_pid: u32,
    metrics_port: Option<u16>,
    event_tx: broadcast::Sender<CoreEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let http = Client::builder()
            .timeout(Duration::from_millis(800))
            .build()
            .unwrap_or_else(|_| Client::new());
        let mut ticker = interval(Duration::from_millis(100));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut last_mlx: Option<u64> = None;
        let mut last_metrics_poll = Instant::now()
            .checked_sub(Duration::from_millis(500))
            .unwrap_or_else(Instant::now);

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

            if let Some(port) = metrics_port {
                let due = last_metrics_poll.elapsed() >= Duration::from_millis(500);
                if due {
                    last_metrics_poll = Instant::now();
                    if let Ok(metrics) = fetch_metrics(&http, port).await {
                        last_mlx = Some(metrics.mlx_active_bytes);
                    }
                }
                sample.mlx_active_bytes = last_mlx;
            }

            let _ = event_tx.send(CoreEvent::Sample(sample));
        }
    })
}

pub async fn run_system_monitor_loop<F>(mut on_sample: F) -> Result<(), std::io::Error>
where
    F: FnMut(ResourceSample) -> bool,
{
    let mut ticker = interval(Duration::from_millis(100));
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