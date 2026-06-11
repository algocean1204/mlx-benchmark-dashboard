use std::path::PathBuf;
use std::process::Stdio;
use aidash_core::auth;
use aidash_core::bench::{
    allocate_bench_run, request_bench_abort, run_context_sweep, run_single_with_id, BenchKind,
    BenchRunConfig, RunResult,
};
use aidash_core::client;
use aidash_core::db::{CompareRow, DeleteSummary, RunListRow};
use aidash_core::env_detect::{self, DoctorItem, DoctorReport, DoctorStatus};
use aidash_core::events::CoreEvent;
use aidash_core::lifecycle::{Command, LifecycleHandle, LifecycleState, StartParams};
use aidash_core::hf_cache::{
    self, CacheDeleteResult, CacheRepoEntry, CacheScanResult, DiskUsage, DownloadProgress,
    HfSearchResult,
};
use aidash_core::monitor::{sample_system, total_system_memory_bytes, ResourceSample};
use aidash_core::sys_memory;
use aidash_core::profile::{self, ModelProfile, ProfileListRow};
use aidash_core::stats::{ContextPick, ModelStats, OverviewRow, DEFAULT_OVERVIEW_CONTEXT};
use aidash_core::tps_tier::{self, TpsTier};
use aidash_core::bootstrap::{self, BootstrapEvent};
use aidash_core::tools;
use aidash_core::{
    find_project_root, profiles_dir, python_adapters_available, python_dir, resolve_file_path,
};
use crate::frb_generated::StreamSink;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;
use tokio::sync::broadcast;
use tokio::time::{sleep, Duration};

use crate::state::{set_state, state_arc, with_state, AppState};

// ── FRB 타입 ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FrbTierInfo {
    pub badge: String,
    pub label: String,
    pub key: String,
}

#[derive(Debug, Clone)]
pub struct FrbDoctorItem {
    pub category: String,
    pub name: String,
    pub status: String,
    pub detail: String,
    pub fix_action: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FrbDoctorReport {
    pub items: Vec<FrbDoctorItem>,
}

#[derive(Debug, Clone)]
pub struct FrbContextPick {
    pub requested: i64,
    pub actual: i64,
    pub substituted: bool,
}

#[derive(Debug, Clone)]
pub struct FrbOverviewRow {
    pub profile_id: String,
    pub display_name: String,
    pub model_type: String,
    pub decode_tps: Option<f64>,
    pub tier: Option<FrbTierInfo>,
    pub ttft_ms: Option<f64>,
    pub context: FrbContextPick,
    pub hf_url: Option<String>,
    pub measured_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FrbContextStatsRow {
    pub context_size: i64,
    pub decode_tps_min: f64,
    pub decode_tps_avg: f64,
    pub decode_tps_max: f64,
    pub ttft_avg_ms: f64,
    pub run_count: i64,
    pub peak_phys_footprint_bytes: i64,
}

#[derive(Debug, Clone)]
pub struct FrbModelStats {
    pub profile_id: String,
    pub display_name: String,
    pub total_runs: i64,
    pub latest_measured_at: Option<String>,
    pub current_tier: Option<FrbTierInfo>,
    pub current_decode_tps: Option<f64>,
    pub peak_phys_footprint_bytes: i64,
    pub peak_mlx_active_bytes: i64,
    pub hf_url: Option<String>,
    pub by_context: Vec<FrbContextStatsRow>,
}

#[derive(Debug, Clone)]
pub struct FrbRunListRow {
    pub run_id: i64,
    pub profile_id: String,
    pub display_name: String,
    pub kind: String,
    pub context_size: Option<i64>,
    pub status: String,
    pub decode_tps: Option<f64>,
    pub peak_phys_footprint_bytes: Option<i64>,
    pub tier: Option<FrbTierInfo>,
}

#[derive(Debug, Clone)]
pub struct FrbCompareRow {
    pub profile_id: String,
    pub display_name: String,
    pub model_type: String,
    pub context_requested: i64,
    pub context_actual: i64,
    pub context_substituted: bool,
    pub decode_tps: Option<f64>,
    pub tier: Option<FrbTierInfo>,
    pub ttft_ms: Option<f64>,
    pub peak_phys_footprint_bytes: Option<i64>,
    pub peak_mlx_active_bytes: Option<i64>,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
    pub measured_at: Option<String>,
    pub hf_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FrbDeleteSummary {
    pub runs: i64,
    pub samples: i64,
    pub results: i64,
}

#[derive(Debug, Clone)]
pub struct FrbProfileRow {
    pub id: String,
    pub backend: String,
    pub model_type: String,
    pub context_default: u32,
    pub context_min: u32,
    pub context_max: u32,
    pub sweep_steps: Vec<u32>,
    pub filename: String,
    pub is_multimodal: bool,
}

#[derive(Debug, Clone)]
pub struct FrbAuthStatus {
    pub sources: Vec<FrbTokenSourceStatus>,
    pub active_source: Option<String>,
    pub masked_token: Option<String>,
    pub whoami_user: String,
    pub can_import: bool,
}

#[derive(Debug, Clone)]
pub struct FrbTokenSourceStatus {
    pub source: String,
    pub label: String,
    pub present: bool,
}

#[derive(Debug, Clone)]
pub struct FrbResourceSample {
    pub ts: u64,
    pub phys_footprint_bytes: u64,
    pub mlx_active_bytes: Option<u64>,
    pub cpu_pct: f64,
    pub sys_available_bytes: u64,
    pub total_memory_bytes: u64,
    pub power_w: Option<f64>,
    pub temp_c: Option<f64>,
    pub throttled: Option<bool>,
}

#[derive(Debug, Clone)]
pub enum FrbBenchMode {
    Single,
    Sweep,
}

#[derive(Debug, Clone)]
pub enum FrbBenchEvent {
    StateChanged { from: String, to: String },
    Sample(FrbResourceSample),
    Token { index: u32, text: String },
    WatchdogWarn,
    WatchdogKill,
    RunFinished {
        run_id: u64,
        status: String,
        result: Option<FrbBenchResult>,
    },
    Log { level: String, message: String },
    Progress { message: String },
}

#[derive(Debug, Clone)]
pub struct FrbBenchResult {
    pub run_id: i64,
    pub status: String,
    pub context_size: u32,
    pub decode_tps: Option<f64>,
    pub tier: Option<FrbTierInfo>,
    pub ttft_ms: Option<f64>,
    pub peak_phys_footprint_bytes: u64,
    pub peak_mlx_active_bytes: u64,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FrbChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct FrbFixProgress {
    pub line: String,
    pub done: bool,
    pub success: bool,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct FrbBootstrapEvent {
    pub step: String,
    pub kind: String,
    pub message: String,
    pub success: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct FrbCacheRepoEntry {
    pub repo_id: String,
    pub size_bytes: u64,
    pub last_modified: Option<String>,
    pub has_profile: bool,
}

#[derive(Debug, Clone)]
pub struct FrbCacheScanResult {
    pub cache_dir: String,
    pub total_size_bytes: u64,
    pub repo_count: usize,
    pub repos: Vec<FrbCacheRepoEntry>,
}

#[derive(Debug, Clone)]
pub struct FrbCacheDeleteResult {
    pub repo_id: String,
    pub deleted: bool,
    pub freed_bytes: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FrbDiskUsage {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub cache_dir: String,
    pub cache_total_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct FrbHfSearchResult {
    pub repo_id: String,
    pub downloads: i64,
    pub likes: i64,
    pub pipeline_tag: Option<String>,
    pub installed: bool,
}

#[derive(Debug, Clone)]
pub struct FrbDownloadProgress {
    pub line: String,
    pub percent: Option<f64>,
    pub done: bool,
    pub success: bool,
}

// ── 변환 헬퍼 ───────────────────────────────────────────────────────────────

fn tier_info(tier: TpsTier) -> FrbTierInfo {
    let info = tier.info();
    FrbTierInfo {
        badge: info.badge.to_string(),
        label: info.label.to_string(),
        key: info.key.to_string(),
    }
}

fn doctor_status_str(s: DoctorStatus) -> String {
    match s {
        DoctorStatus::Ok => "ok".into(),
        DoctorStatus::Warn => "warn".into(),
        DoctorStatus::Missing => "missing".into(),
        DoctorStatus::Info => "info".into(),
    }
}

fn lifecycle_state_str(s: LifecycleState) -> String {
    format!("{s:?}").to_lowercase()
}

fn ctx_pick(p: ContextPick) -> FrbContextPick {
    FrbContextPick {
        requested: p.requested,
        actual: p.actual,
        substituted: p.substituted,
    }
}

fn sample_to_frb(s: &ResourceSample) -> FrbResourceSample {
    FrbResourceSample {
        ts: s.ts,
        phys_footprint_bytes: s.phys_footprint_bytes,
        mlx_active_bytes: s.mlx_active_bytes,
        cpu_pct: s.cpu_pct,
        sys_available_bytes: s.sys_available_bytes,
        total_memory_bytes: total_system_memory_bytes(),
        power_w: s.power_w,
        temp_c: s.temp_c,
        throttled: s.throttled,
    }
}

fn bench_result_to_frb(r: &RunResult) -> FrbBenchResult {
    FrbBenchResult {
        run_id: r.run_id,
        status: r.status.clone(),
        context_size: r.context_size,
        decode_tps: r.stats.as_ref().and_then(|s| s.decode_tps),
        tier: r.stats.as_ref().and_then(|s| s.decode_tps.map(tps_tier::tps_tier).map(tier_info)),
        ttft_ms: r.stats.as_ref().map(|s| s.ttft_ms),
        peak_phys_footprint_bytes: r.peak_phys_footprint_bytes,
        peak_mlx_active_bytes: r.peak_mlx_active_bytes,
        error_message: r.error_message.clone(),
    }
}

fn core_event_to_frb(ev: CoreEvent, result: Option<&RunResult>) -> Option<FrbBenchEvent> {
    match ev {
        CoreEvent::StateChanged { from, to } => Some(FrbBenchEvent::StateChanged {
            from: lifecycle_state_str(from),
            to: lifecycle_state_str(to),
        }),
        CoreEvent::Sample(s) => Some(FrbBenchEvent::Sample(sample_to_frb(&s))),
        CoreEvent::Token { index, text } => Some(FrbBenchEvent::Token { index, text }),
        CoreEvent::WatchdogWarn => Some(FrbBenchEvent::WatchdogWarn),
        CoreEvent::WatchdogKill => Some(FrbBenchEvent::WatchdogKill),
        CoreEvent::RunFinished { run_id, status } => Some(FrbBenchEvent::RunFinished {
            run_id,
            status,
            result: result.map(bench_result_to_frb),
        }),
        CoreEvent::Log { level, message } => Some(FrbBenchEvent::Log { level, message }),
    }
}

fn profile_max_tokens(profile: &ModelProfile) -> u32 {
    profile
        .default_params
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .filter(|v| *v > 0)
        .unwrap_or(512)
}

// ── 공개 API ────────────────────────────────────────────────────────────────

#[flutter_rust_bridge::frb(sync)]
pub fn init(root_path: String) -> Result<(), String> {
    let root = resolve_effective_project_root(&root_path)?;
    if !python_adapters_available(&root) {
        let py = python_dir(&root);
        return Err(format!(
            "python adapters not found (expected {}): {}",
            py.display(),
            root.display()
        ));
    }
    let _ = std::fs::create_dir_all(profiles_dir(&root));
    let state = AppState::new(root)?;
    set_state(state)
}

fn resolve_effective_project_root(root_path: &str) -> Result<PathBuf, String> {
    if !root_path.is_empty() {
        let candidate = PathBuf::from(root_path);
        if candidate.join("profiles").is_dir() && candidate.join("python").join("adapters").is_dir()
        {
            return Ok(candidate);
        }
    }
    find_project_root().ok_or_else(|| {
        format!(
            "project root not found (need profiles/ and python adapters/): {}",
            if root_path.is_empty() {
                "(empty hint)".into()
            } else {
                root_path.to_string()
            }
        )
    })
}

#[flutter_rust_bridge::frb(sync)]
pub fn is_bundle_deploy_mode() -> bool {
    tools::is_bundle_deploy_mode()
}

fn map_bootstrap_event(ev: BootstrapEvent) -> FrbBootstrapEvent {
    match ev {
        BootstrapEvent::StepStart { step, message } => FrbBootstrapEvent {
            step,
            kind: "step_start".into(),
            message,
            success: None,
        },
        BootstrapEvent::StepDone {
            step,
            success,
            message,
        } => FrbBootstrapEvent {
            step,
            kind: "step_done".into(),
            message,
            success: Some(success),
        },
        BootstrapEvent::Log { line } => FrbBootstrapEvent {
            step: String::new(),
            kind: "log".into(),
            message: line,
            success: None,
        },
    }
}

#[flutter_rust_bridge::frb]
pub async fn env_bootstrap(sink: StreamSink<FrbBootstrapEvent>) -> Result<(), String> {
    let root = with_state(|s| Ok(s.project_root.clone()))?;
    bootstrap::env_bootstrap(&root, |ev| {
        sink.add(map_bootstrap_event(ev));
    })
    .await
}

#[flutter_rust_bridge::frb]
pub async fn doctor_report() -> Result<FrbDoctorReport, String> {
    let root = with_state(|s| Ok(s.project_root.clone()))?;
    let report = env_detect::run_doctor(root).await;
    Ok(map_doctor_report(report))
}

fn map_doctor_report(report: DoctorReport) -> FrbDoctorReport {
    FrbDoctorReport {
        items: report
            .items
            .into_iter()
            .map(|i| map_doctor_item(i))
            .collect(),
    }
}

fn map_doctor_item(i: DoctorItem) -> FrbDoctorItem {
    FrbDoctorItem {
        category: i.category,
        name: i.name,
        status: doctor_status_str(i.status),
        detail: i.detail,
        fix_action: i.fix_action,
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn stats_overview(ctx: Option<i64>) -> Result<Vec<FrbOverviewRow>, String> {
    with_state(|s| {
        let target = ctx.unwrap_or(DEFAULT_OVERVIEW_CONTEXT);
        let rows = s.db.stats_overview(target).map_err(|e| e.to_string())?;
        Ok(rows.into_iter().map(map_overview_row).collect())
    })
}

fn map_overview_row(r: OverviewRow) -> FrbOverviewRow {
    FrbOverviewRow {
        profile_id: r.profile_id,
        display_name: r.display_name,
        model_type: r.model_type,
        decode_tps: r.decode_tps,
        tier: r.tier.map(tier_info),
        ttft_ms: r.ttft_ms,
        context: ctx_pick(r.context),
        hf_url: r.hf_url,
        measured_at: r.measured_at,
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn stats_model(id: String) -> Result<FrbModelStats, String> {
    with_state(|s| {
        let stats = s.db.stats_model(&id).map_err(|e| e.to_string())?;
        Ok(map_model_stats(stats))
    })
}

fn map_model_stats(s: ModelStats) -> FrbModelStats {
    FrbModelStats {
        profile_id: s.profile_id,
        display_name: s.display_name,
        total_runs: s.total_runs,
        latest_measured_at: s.latest_measured_at,
        current_tier: s.current_tier.map(tier_info),
        current_decode_tps: s.current_decode_tps,
        peak_phys_footprint_bytes: s.peak_phys_footprint_bytes,
        peak_mlx_active_bytes: s.peak_mlx_active_bytes,
        hf_url: s.hf_url,
        by_context: s
            .by_context
            .into_iter()
            .map(|c| FrbContextStatsRow {
                context_size: c.context_size,
                decode_tps_min: c.decode_tps_min,
                decode_tps_avg: c.decode_tps_avg,
                decode_tps_max: c.decode_tps_max,
                ttft_avg_ms: c.ttft_avg_ms,
                run_count: c.run_count,
                peak_phys_footprint_bytes: c.peak_phys_footprint_bytes,
            })
            .collect(),
    }
}

fn profile_ids_for_root(root: &std::path::Path) -> Vec<String> {
    let profiles = profiles_dir(root);
    profile::list_profiles(&profiles)
        .map(|rows| rows.into_iter().map(|r| r.id).collect())
        .unwrap_or_default()
}

fn repo_in_active_use(state: &AppState, repo_id: &str) -> Option<String> {
    if let Some(ref serve_id) = state.serve_profile_id {
        if serve_id == repo_id {
            return Some("현재 서버(serve) 실행 중인 모델입니다".into());
        }
    }
    if let Some(ref bench_id) = state.bench_profile_id {
        if bench_id == repo_id {
            return Some("벤치마크가 진행 중인 모델입니다".into());
        }
    }
    None
}

fn map_cache_repo(entry: CacheRepoEntry, profile_ids: &[String]) -> FrbCacheRepoEntry {
    FrbCacheRepoEntry {
        repo_id: entry.repo_id.clone(),
        size_bytes: entry.size_bytes,
        last_modified: entry.last_modified,
        has_profile: profile_ids.iter().any(|id| id == &entry.repo_id),
    }
}

fn map_cache_scan(scan: CacheScanResult, profile_ids: &[String]) -> FrbCacheScanResult {
    FrbCacheScanResult {
        cache_dir: scan.cache_dir,
        total_size_bytes: scan.total_size_bytes,
        repo_count: scan.repo_count,
        repos: scan
            .repos
            .into_iter()
            .map(|r| map_cache_repo(r, profile_ids))
            .collect(),
    }
}

fn map_cache_delete(r: CacheDeleteResult) -> FrbCacheDeleteResult {
    FrbCacheDeleteResult {
        repo_id: r.repo_id,
        deleted: r.deleted,
        freed_bytes: r.freed_bytes,
        error: r.error,
    }
}

fn map_disk_usage(d: DiskUsage) -> FrbDiskUsage {
    FrbDiskUsage {
        total_bytes: d.total_bytes,
        available_bytes: d.available_bytes,
        cache_dir: d.cache_dir,
        cache_total_bytes: d.cache_total_bytes,
    }
}

fn map_hf_search(r: HfSearchResult) -> FrbHfSearchResult {
    FrbHfSearchResult {
        repo_id: r.repo_id,
        downloads: r.downloads,
        likes: r.likes,
        pipeline_tag: r.pipeline_tag,
        installed: r.installed,
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn list_runs(model: Option<String>) -> Result<Vec<FrbRunListRow>, String> {
    with_state(|s| {
        let rows = s
            .db
            .list_runs(model.as_deref())
            .map_err(|e| e.to_string())?;
        Ok(rows.into_iter().map(map_run_row).collect())
    })
}

fn map_run_row(r: RunListRow) -> FrbRunListRow {
    FrbRunListRow {
        run_id: r.run_id,
        profile_id: r.profile_id,
        display_name: r.display_name,
        kind: r.kind,
        context_size: r.context_size,
        status: r.status,
        decode_tps: r.decode_tps,
        peak_phys_footprint_bytes: r.peak_phys_footprint_bytes,
        tier: r.decode_tps.map(tps_tier::tps_tier).map(tier_info),
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn delete_run(id: i64) -> Result<FrbDeleteSummary, String> {
    with_state(|s| {
        let summary = s.db.delete_run_summary(id).map_err(|e| e.to_string())?;
        s.db.delete_run(id).map_err(|e| e.to_string())?;
        Ok(map_delete_summary(summary))
    })
}

#[flutter_rust_bridge::frb(sync)]
pub fn delete_model(id: String) -> Result<FrbDeleteSummary, String> {
    with_state(|s| {
        let summary = s
            .db
            .delete_model_summary(&id)
            .map_err(|e| e.to_string())?;
        s.db.delete_model(&id).map_err(|e| e.to_string())?;
        Ok(map_delete_summary(summary))
    })
}

fn map_delete_summary(s: DeleteSummary) -> FrbDeleteSummary {
    FrbDeleteSummary {
        runs: s.runs,
        samples: s.samples,
        results: s.results,
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn compare(models: Vec<String>, ctx: Option<i64>) -> Result<Vec<FrbCompareRow>, String> {
    if models.len() < 2 {
        return Err("at least two models required".into());
    }
    with_state(|s| {
        let target = ctx.unwrap_or(DEFAULT_OVERVIEW_CONTEXT);
        let rows = s
            .db
            .compare_models(&models, target)
            .map_err(|e| e.to_string())?;
        Ok(rows.into_iter().map(map_compare_row).collect())
    })
}

fn map_compare_row(r: CompareRow) -> FrbCompareRow {
    FrbCompareRow {
        profile_id: r.profile_id,
        display_name: r.display_name,
        model_type: r.model_type,
        context_requested: r.context_requested,
        context_actual: r.context_actual,
        context_substituted: r.context_substituted,
        decode_tps: r.decode_tps,
        tier: r.decode_tps.map(tps_tier::tps_tier).map(tier_info),
        ttft_ms: r.ttft_ms,
        peak_phys_footprint_bytes: r.peak_phys_footprint_bytes,
        peak_mlx_active_bytes: r.peak_mlx_active_bytes,
        tokens_in: r.tokens_in,
        tokens_out: r.tokens_out,
        measured_at: r.measured_at,
        hf_url: r.hf_url,
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn list_profiles() -> Result<Vec<FrbProfileRow>, String> {
    with_state(|s| {
        let rows = profile::list_profiles(&profiles_dir(&s.project_root))
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            let full = profile::load_profile_by_id(&profiles_dir(&s.project_root), &row.id)
                .map_err(|e| e.to_string())?;
            out.push(map_profile_row(row, &full));
        }
        Ok(out)
    })
}

fn map_profile_row(row: ProfileListRow, full: &ModelProfile) -> FrbProfileRow {
    FrbProfileRow {
        id: row.id,
        backend: row.backend,
        model_type: row.model_type,
        context_default: row.context_default,
        context_min: full.context.min,
        context_max: full.context.max,
        sweep_steps: full.context.sweep_steps.clone(),
        filename: row.filename,
        is_multimodal: full.model_type == "multimodal" || full.io.input.contains(&"image".to_string()),
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn profile_set_task(
    profile_id: String,
    task: String,
    adjust_backend: bool,
) -> Result<(), String> {
    with_state(|s| {
        profile::set_profile_task(
            &profiles_dir(&s.project_root),
            &profile_id,
            &task,
            adjust_backend,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

#[flutter_rust_bridge::frb(sync)]
pub fn profile_task_label(task: String) -> String {
    profile::task_label_ko(&task).to_string()
}

#[flutter_rust_bridge::frb]
pub async fn bench_start(
    profile_id: String,
    ctx: u32,
    mode: FrbBenchMode,
    prompt: Option<String>,
    image_path: Option<String>,
    audio_path: Option<String>,
    bench_task: Option<String>,
) -> Result<i64, String> {
    let arc = state_arc()?;
    let (root, progress_tx, run_id, model_profile, config, db) = {
        let mut s = arc.lock();
        if s.bench_task.is_some() {
            return Err("bench already running".into());
        }
        let (tx, _) = broadcast::channel(256);
        s.bench_events_tx = Some(tx.clone());
        let root = s.project_root.clone();
        let db = s.db.clone();
        let mut model_profile = profile::load_profile_by_id(&profiles_dir(&root), &profile_id)
            .map_err(|e| e.to_string())?;
        if let Some(ref task) = bench_task {
            if !profile::is_valid_task(task) {
                return Err(format!("unsupported bench task: {task}"));
            }
            if task == profile::TASK_VIDEO_GEN {
                return Err("지원 예정 — 현재 측정 불가 (video_gen)".into());
            }
            model_profile.model_type = task.clone();
        }
        if model_profile.model_type == profile::TASK_VIDEO_GEN {
            return Err("지원 예정 — 현재 측정 불가 (video_gen)".into());
        }
        let resolved_image = image_path
            .as_deref()
            .map(|p| resolve_file_path(p, &root))
            .transpose()?;
        let resolved_audio = audio_path
            .as_deref()
            .map(|p| resolve_file_path(p, &root))
            .transpose()?;
        let config = BenchRunConfig {
            profile: model_profile.clone(),
            context: ctx,
            prompt,
            image_path: resolved_image,
            audio_path: resolved_audio,
            mem_limit_gb: None,
            kind: BenchKind::Single,
            sweep_id: None,
            python_dir: python_dir(&root),
            child_spec: None,
            port: None,
            progress_tx: Some(tx.clone()),
        };
        let run_id = allocate_bench_run(&db, &config, None)?;
        (root, tx, run_id, model_profile, config, db)
    };

    let arc_task = arc.clone();
    let task = tokio::spawn(async move {
        let result = match mode {
            FrbBenchMode::Single => run_single_with_id(&db, run_id, config, None)
                .await
                .map(Some),
            FrbBenchMode::Sweep => {
                let steps = if model_profile.context.sweep_steps.is_empty() {
                    vec![ctx]
                } else {
                    model_profile.context.sweep_steps.clone()
                };
                run_context_sweep(
                    &db,
                    model_profile,
                    steps,
                    None,
                    python_dir(&root),
                    None,
                    None,
                    None,
                    Some(progress_tx.clone()),
                )
                .await
                .map(|_| None)
            }
        };

        match result {
            Ok(Some(r)) => {
                let _ = progress_tx.send(CoreEvent::RunFinished {
                    run_id: r.run_id as u64,
                    status: r.status.clone(),
                });
            }
            Ok(None) => {}
            Err(e) => {
                let _ = progress_tx.send(CoreEvent::Log {
                    level: "error".into(),
                    message: e,
                });
            }
        }
        let mut s = arc_task.lock();
        s.bench_task = None;
        s.bench_profile_id = None;
        s.bench_events_tx = None;
    });

    with_state(|s| {
        s.bench_task = Some(task);
        s.bench_profile_id = Some(profile_id);
        Ok(run_id)
    })
}

#[flutter_rust_bridge::frb]
pub async fn bench_events(sink: StreamSink<FrbBenchEvent>) -> Result<(), String> {
    let rx = with_state(|s| {
        s.bench_events_tx
            .as_ref()
            .map(|tx| tx.subscribe())
            .ok_or_else(|| "no active bench session".into())
    })?;

    let mut rx = rx;
    loop {
        match rx.recv().await {
            Ok(ev) => {
                if let Some(frb) = core_event_to_frb(ev, None) {
                    sink.add(frb);
                }
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
    Ok(())
}

#[flutter_rust_bridge::frb(sync)]
pub fn bench_abort() -> Result<bool, String> {
    let aborted = request_bench_abort();
    with_state(|s| {
        if let Some(task) = s.bench_task.take() {
            task.abort();
        }
        s.bench_profile_id = None;
        s.bench_events_tx = None;
        Ok(aborted)
    })
}

#[flutter_rust_bridge::frb]
pub async fn serve_start(profile_id: String, ctx: u32) -> Result<(), String> {
    let (cmd_tx, start) = with_state(|s| {
        if s.serve_handle.is_some() {
            return Err("server already running".into());
        }
        let model_profile =
            profile::load_profile_by_id(&profiles_dir(&s.project_root), &profile_id)
                .map_err(|e| e.to_string())?;
        let handle = LifecycleHandle::spawn();
        let events_tx = handle.event_tx.clone();
        let start = StartParams {
            profile: model_profile,
            context: ctx,
            mem_limit_gb: None,
            port: None,
            python_dir: python_dir(&s.project_root),
            child_spec: None,
        };
        let cmd_tx = handle.command_tx.clone();
        s.serve_handle = Some(handle);
        s.serve_events_tx = Some(events_tx);
        s.serve_profile_id = Some(profile_id);
        Ok((cmd_tx, start))
    })?;
    let _ = cmd_tx.send(Command::Start(start)).await;
    Ok(())
}

#[flutter_rust_bridge::frb]
pub async fn serve_stop() -> Result<(), String> {
    with_state(|s| {
        if let Some(handle) = s.serve_handle.take() {
            let cmd_tx = handle.command_tx.clone();
            tokio::spawn(async move {
                let _ = cmd_tx.send(Command::Stop).await;
                handle.wait_for_state(LifecycleState::Idle).await;
            });
        }
        s.serve_events_tx = None;
        Ok(())
    })
}

#[flutter_rust_bridge::frb]
pub async fn chat_send(
    messages: Vec<FrbChatMessage>,
    image_path: Option<String>,
    sink: StreamSink<String>,
) -> Result<(), String> {
    let (root, cmd_tx, events_tx, port, profile_id, max_tokens) = with_state(|s| {
        let handle = s
            .serve_handle
            .as_ref()
            .ok_or_else(|| "server not running — call serve_start first".to_string())?;
        let port = (*handle.port_rx.borrow())
            .ok_or_else(|| "server port not ready".to_string())?;
        let profile_id = s
            .serve_profile_id
            .clone()
            .ok_or_else(|| "serve profile not set".to_string())?;
        let profile =
            profile::load_profile_by_id(&profiles_dir(&s.project_root), &profile_id)
                .map_err(|e| e.to_string())?;
        Ok((
            s.project_root.clone(),
            handle.command_tx.clone(),
            s.serve_events_tx.clone(),
            port,
            profile_id,
            profile_max_tokens(&profile),
        ))
    })?;

    let last_user = messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.clone())
        .ok_or_else(|| "no user message".to_string())?;

    let image = if let Some(path) = image_path {
        Some(resolve_file_path(&path, &root)?)
    } else {
        None
    };

    if let Some(tx) = events_tx {
        let mut rx = tx.subscribe();
        let token_sink = sink;
        tokio::spawn(async move {
            while let Ok(ev) = rx.recv().await {
                if let CoreEvent::Token { text, .. } = ev {
                    token_sink.add(text);
                }
            }
        });
    }

    let _ = cmd_tx.send(Command::BeginWork).await;

    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| e.to_string())?;

    let event_tx = with_state(|s| {
        Ok(s.serve_events_tx.clone())
    })?;

    let result = if let Some(img) = image {
        client::stream_chat_completion_with_image(
            &http,
            port,
            &profile_id,
            &last_user,
            Some(img.as_path()),
            max_tokens,
            event_tx,
        )
        .await
    } else {
        client::stream_chat_completion(
            &http,
            port,
            &profile_id,
            &last_user,
            max_tokens,
            event_tx,
        )
        .await
    };

    let _ = cmd_tx.send(Command::EndWork).await;
    result.map(|_| ()).map_err(|e| e.to_string())
}

#[flutter_rust_bridge::frb]
pub async fn auth_status() -> Result<FrbAuthStatus, String> {
    let status = auth::build_auth_status().await;
    Ok(map_auth_status(status))
}

fn map_auth_status(s: auth::AuthStatus) -> FrbAuthStatus {
    let can_import = s.sources.iter().any(|src| {
        src.present
            && !matches!(
                src.source,
                aidash_core::auth::TokenSource::Keychain
            )
    });
    FrbAuthStatus {
        sources: s
            .sources
            .into_iter()
            .map(|src| FrbTokenSourceStatus {
                source: format!("{:?}", src.source).to_lowercase(),
                label: src.source.label().to_string(),
                present: src.present,
            })
            .collect(),
        active_source: s.active_source.map(|src| format!("{:?}", src).to_lowercase()),
        masked_token: s.masked_token,
        whoami_user: s.whoami_user,
        can_import,
    }
}

#[flutter_rust_bridge::frb]
pub async fn auth_set(token: String) -> Result<String, String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err("empty token".into());
    }
    let username = auth::verify_whoami(trimmed)
        .await
        .map_err(|e| e.to_string())?;
    auth::keychain_set(trimmed).map_err(|e| e.to_string())?;
    Ok(username)
}

#[flutter_rust_bridge::frb]
pub async fn auth_import() -> Result<String, String> {
    auth::import_from_hf_cli()
        .await
        .map_err(|e| e.to_string())
}

#[flutter_rust_bridge::frb(sync)]
pub fn auth_clear() -> Result<(), String> {
    auth::keychain_clear().map_err(|e| e.to_string())
}

#[flutter_rust_bridge::frb]
pub async fn auth_verify_token(token: String) -> Result<String, String> {
    auth::verify_whoami(token.trim())
        .await
        .map_err(|e| e.to_string())
}

#[flutter_rust_bridge::frb]
pub async fn system_resources(sink: StreamSink<FrbResourceSample>) -> Result<(), String> {
    loop {
        let sample = sample_system();
        sink.add(sample_to_frb(&sample));
        sleep(Duration::from_secs(1)).await;
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn tps_tier(decode_tps: f64) -> FrbTierInfo {
    tier_info(tps_tier::tps_tier(decode_tps))
}

#[flutter_rust_bridge::frb(sync)]
pub fn get_project_root() -> Result<String, String> {
    with_state(|s| Ok(s.project_root.display().to_string()))
}

#[flutter_rust_bridge::frb(sync)]
pub fn set_project_root(path: String) -> Result<(), String> {
    let root = PathBuf::from(&path);
    if !root.join("profiles").is_dir() || !root.join("python").is_dir() {
        return Err("invalid project root".into());
    }
    with_state(|s| {
        s.project_root = root;
        Ok(())
    })
}

#[flutter_rust_bridge::frb]
pub async fn run_fix_action(command: String, sink: StreamSink<FrbFixProgress>) -> Result<(), String> {
    let root = with_state(|s| Ok(s.project_root.clone()))?;
    let mut cmd = TokioCommand::new("sh");
    cmd.arg("-c")
        .arg(&command)
        .current_dir(&root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    if let Some(out) = stdout {
        let mut reader = BufReader::new(out).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            sink.add(FrbFixProgress {
                line,
                done: false,
                success: true,
                exit_code: None,
            });
        }
    }

    if let Some(err) = stderr {
        let mut reader = BufReader::new(err).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            sink.add(FrbFixProgress {
                line,
                done: false,
                success: false,
                exit_code: None,
            });
        }
    }

    let status = child.wait().await.map_err(|e| e.to_string())?;
    sink.add(FrbFixProgress {
        line: format!("exit code: {}", status.code().unwrap_or(-1)),
        done: true,
        success: status.success(),
        exit_code: status.code(),
    });
    Ok(())
}

#[flutter_rust_bridge::frb(sync)]
pub fn device_label() -> String {
    aidash_core::system_device_label()
}

#[flutter_rust_bridge::frb(sync)]
pub fn system_memory_info() -> (u64, u64, u64, f64) {
    let mem = sys_memory::system_memory();
    (
        mem.total_bytes,
        mem.used_bytes,
        mem.available_bytes,
        sys_memory::system_free_percent(),
    )
}

#[flutter_rust_bridge::frb]
pub async fn cache_scan() -> Result<FrbCacheScanResult, String> {
    let root = with_state(|s| Ok(s.project_root.clone()))?;
    let profile_ids = profile_ids_for_root(&root);
    let scan = hf_cache::cache_scan(&root).await.map_err(|e| e.to_string())?;
    Ok(map_cache_scan(scan, &profile_ids))
}

#[flutter_rust_bridge::frb]
pub async fn cache_delete(repo_id: String) -> Result<FrbCacheDeleteResult, String> {
    with_state(|s| {
        if let Some(reason) = repo_in_active_use(s, &repo_id) {
            return Err(reason);
        }
        Ok(s.project_root.clone())
    })?;
    let root = with_state(|s| Ok(s.project_root.clone()))?;
    let result = hf_cache::cache_delete(&root, &repo_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(map_cache_delete(result))
}

#[flutter_rust_bridge::frb]
pub async fn disk_usage() -> Result<FrbDiskUsage, String> {
    let root = with_state(|s| Ok(s.project_root.clone()))?;
    let usage = hf_cache::disk_usage(&root).await.map_err(|e| e.to_string())?;
    Ok(map_disk_usage(usage))
}

#[flutter_rust_bridge::frb]
pub async fn hf_search(query: String) -> Result<Vec<FrbHfSearchResult>, String> {
    let root = with_state(|s| Ok(s.project_root.clone()))?;
    let scan = hf_cache::cache_scan(&root).await.map_err(|e| e.to_string())?;
    let installed: Vec<String> = scan.repos.into_iter().map(|r| r.repo_id).collect();
    let results = hf_cache::hf_search(&query, &installed)
        .await
        .map_err(|e| e.to_string())?;
    Ok(results.into_iter().map(map_hf_search).collect())
}

#[flutter_rust_bridge::frb]
pub async fn hf_model_size(repo_id: String) -> Result<u64, String> {
    hf_cache::hf_model_size(&repo_id)
        .await
        .map_err(|e| e.to_string())
}

#[flutter_rust_bridge::frb]
pub async fn hf_download_start(
    repo_id: String,
    sink: StreamSink<FrbDownloadProgress>,
) -> Result<(), String> {
    let arc = state_arc()?;
    let (root, cancel_rx) = {
        let mut s = arc.lock();
        if s.download_task.is_some() {
            return Err("download already in progress".into());
        }
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        s.download_cancel_tx = Some(cancel_tx);
        s.download_repo_id = Some(repo_id.clone());
        (s.project_root.clone(), cancel_rx)
    };

    let size = hf_cache::hf_model_size(&repo_id)
        .await
        .map_err(|e| e.to_string())?;
    let usage = hf_cache::disk_usage(&root).await.map_err(|e| e.to_string())?;
    hf_cache::check_disk_for_download(usage.available_bytes, size).map_err(|e| e.to_string())?;

    let arc_task = arc.clone();
    let task = tokio::spawn(async move {
        let result = hf_cache::hf_download(
            &root,
            &repo_id,
            |p: DownloadProgress| {
                sink.add(FrbDownloadProgress {
                    line: p.line,
                    percent: p.percent,
                    done: p.done,
                    success: p.success,
                });
            },
            cancel_rx,
        )
        .await;

        if let Err(e) = result {
            sink.add(FrbDownloadProgress {
                line: e.to_string(),
                percent: None,
                done: true,
                success: false,
            });
        }

        let mut s = arc_task.lock();
        s.download_task = None;
        s.download_cancel_tx = None;
        s.download_repo_id = None;
    });

    with_state(|s| {
        s.download_task = Some(task);
        Ok(())
    })
}

#[flutter_rust_bridge::frb(sync)]
pub fn hf_download_cancel() -> Result<bool, String> {
    with_state(|s| {
        if let Some(tx) = s.download_cancel_tx.take() {
            let _ = tx.send(true);
            if let Some(task) = s.download_task.take() {
                task.abort();
            }
            s.download_repo_id = None;
            Ok(true)
        } else {
            Ok(false)
        }
    })
}

#[flutter_rust_bridge::frb(sync)]
pub fn profile_generate(repo_id: String) -> Result<String, String> {
    with_state(|s| {
        let path = profile::generate_profile_hf(&profiles_dir(&s.project_root), &repo_id)
            .map_err(|e| e.to_string())?;
        Ok(path.display().to_string())
    })
}