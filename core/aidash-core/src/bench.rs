//! 측정 오케스트레이션: 단일 런, 컨텍스트 스윕, 한계 찾기(이진탐색), A/B 대결,
//! 양자화 비교, 품질 미니평가
//!
//! IN: 벤치 설정, 프로파일
//! OUT: `RunResult`, 진행 이벤트

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex};
use tokio::time::timeout;

use crate::client::{self, StreamStats};
use crate::db::{Database, SampleRecorder};
use crate::events::CoreEvent;
use crate::lifecycle::{Command, LifecycleHandle, LifecycleState, StartParams};
use crate::profile::ModelProfile;
use crate::pyproc::ChildSpec;

static ACTIVE_BENCH_ABORT_TX: OnceLock<StdMutex<Option<tokio::sync::mpsc::Sender<Command>>>> =
    OnceLock::new();

/// 진행 중인 벤치마크에 Abort 명령을 보낸다. 활성 세션이 없으면 false.
pub fn request_bench_abort() -> bool {
    let Some(slot) = ACTIVE_BENCH_ABORT_TX.get() else {
        return false;
    };
    let Ok(guard) = slot.lock() else {
        return false;
    };
    let Some(tx) = guard.as_ref() else {
        return false;
    };
    tx.try_send(Command::Abort {
        reason: "user abort".into(),
    })
    .is_ok()
}

fn register_bench_abort_tx(tx: tokio::sync::mpsc::Sender<Command>) {
    let slot = ACTIVE_BENCH_ABORT_TX.get_or_init(|| StdMutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(tx);
    }
}

fn clear_bench_abort_tx() {
    if let Some(slot) = ACTIVE_BENCH_ABORT_TX.get() {
        if let Ok(mut guard) = slot.lock() {
            *guard = None;
        }
    }
}

pub const TTS_BENCH_PROMPT: &str = "Hello, benchmark test.";
pub const IMAGE_GEN_BENCH_PROMPT: &str = "A simple red circle on white background.";
pub const MULTIMODAL_BENCH_PROMPT: &str = "Describe this image briefly.";

const FILLER_TEXT: &str = "\
인공지능 모델의 성능을 측정하기 위해 충분한 길이의 컨텍스트를 채워야 한다. \
Large language models process text in tokens, and context window size directly \
affects memory usage and throughput. 벤치마크 실행 시 프롬프트 길이는 목표 \
컨텍스트의 약 80퍼센트에 해당하는 토큰 수를 목표로 한다. This paragraph \
mixes Korean and English so tokenizer behavior stays realistic across scripts. \
반복 연결로 목표 문자 수에 도달하면 잘라서 측정 입력으로 사용한다. \
Performance metrics include TTFT, prefill TPS, decode TPS, and peak memory. ";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchKind {
    Single,
    SweepStep,
    LimitSearch,
    AbBattle,
    QuantCompare,
    QualityEval,
    Chat,
}

impl BenchKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BenchKind::Single => "single",
            BenchKind::SweepStep => "sweep_step",
            BenchKind::LimitSearch => "limit_search",
            BenchKind::AbBattle => "ab_battle",
            BenchKind::QuantCompare => "quant_compare",
            BenchKind::QualityEval => "quality_eval",
            BenchKind::Chat => "chat",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    Completed,
    AbortedWatchdog,
    Failed,
    Cancelled,
}

impl RunOutcome {
    pub fn status_str(self) -> &'static str {
        match self {
            RunOutcome::Completed => "completed",
            RunOutcome::AbortedWatchdog => "aborted_watchdog",
            RunOutcome::Failed => "failed",
            RunOutcome::Cancelled => "cancelled",
        }
    }

    pub fn is_failure(self) -> bool {
        matches!(self, RunOutcome::Failed | RunOutcome::AbortedWatchdog)
    }
}

#[derive(Debug, Clone)]
pub struct RunResult {
    pub run_id: i64,
    pub kind: BenchKind,
    pub status: String,
    pub context_size: u32,
    pub stats: Option<StreamStats>,
    pub peak_phys_footprint_bytes: u64,
    pub peak_mlx_active_bytes: u64,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BenchRunConfig {
    pub profile: ModelProfile,
    pub context: u32,
    pub prompt: Option<String>,
    pub image_path: Option<PathBuf>,
    pub audio_path: Option<PathBuf>,
    pub mem_limit_gb: Option<f64>,
    pub kind: BenchKind,
    pub sweep_id: Option<i64>,
    pub python_dir: PathBuf,
    pub child_spec: Option<ChildSpec>,
    pub port: Option<u16>,
    pub progress_tx: Option<broadcast::Sender<CoreEvent>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SweepSummaryRow {
    pub context_size: u32,
    pub tokens_in: u32,
    pub ttft_ms: f64,
    pub decode_tps: Option<f64>,
    pub peak_phys_footprint_bytes: u64,
    pub peak_mlx_active_bytes: u64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SweepSummary {
    pub sweep_id: i64,
    pub rows: Vec<SweepSummaryRow>,
    pub skipped_remaining: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LimitSearchSummary {
    pub sweep_id: i64,
    pub limit_context: u64,
    pub attempts: Vec<LimitAttemptRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LimitAttemptRow {
    pub context_size: u64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbBattleRow {
    pub profile_id: String,
    pub run_id: i64,
    pub decode_tps: Option<f64>,
    pub ttft_ms: Option<f64>,
    pub peak_phys_footprint_bytes: u64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AbBattleSummary {
    pub sweep_id: i64,
    pub context_size: u32,
    pub rows: Vec<AbBattleRow>,
    pub winner: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuantCompareRow {
    pub profile_id: String,
    pub run_id: i64,
    pub decode_tps: Option<f64>,
    pub peak_phys_footprint_bytes: u64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuantCompareSummary {
    pub sweep_id: i64,
    pub context_size: u32,
    pub rows: Vec<QuantCompareRow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeOutcome {
    Success,
    Failure,
}

struct CycleOutcome {
    outcome: RunOutcome,
    stats: Option<StreamStats>,
    recorder: SampleRecorder,
    error_message: Option<String>,
}

pub fn estimate_chars_for_context(context_size: u32, fill_ratio: f64) -> usize {
    let target_tokens = (context_size as f64 * fill_ratio).round() as u32;
    (target_tokens as f64 * 3.5).ceil() as usize
}

pub fn build_context_fill_prompt(context_size: u32) -> String {
    let target_chars = estimate_chars_for_context(context_size, 0.8);
    if target_chars == 0 {
        return String::new();
    }
    let mut out = String::with_capacity(target_chars + FILLER_TEXT.len());
    while out.len() < target_chars {
        out.push_str(FILLER_TEXT);
    }
    let mut end = target_chars;
    while end > 0 && !out.is_char_boundary(end) {
        end -= 1;
    }
    out.truncate(end);
    out
}

pub fn parse_step_token(s: &str) -> Result<u32, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty step".into());
    }
    if let Some(num) = s.strip_suffix('k').or_else(|| s.strip_suffix('K')) {
        let value: f64 = num
            .trim()
            .parse()
            .map_err(|_| format!("invalid step token: {s}"))?;
        Ok((value * 1024.0).round() as u32)
    } else {
        s.parse::<u32>()
            .map_err(|_| format!("invalid step token: {s}"))
    }
}

pub fn parse_steps_list(s: &str) -> Result<Vec<u32>, String> {
    s.split(',')
        .map(parse_step_token)
        .collect::<Result<Vec<_>, _>>()
}

pub fn next_probe(lo: u64, hi: u64, granularity: u64) -> Option<u64> {
    if hi.saturating_sub(lo) <= granularity {
        return None;
    }
    let mid = lo + (hi - lo) / 2;
    if mid <= lo {
        return None;
    }
    Some(mid)
}

pub fn apply_probe_result(lo: u64, hi: u64, mid: u64, outcome: ProbeOutcome) -> (u64, u64) {
    match outcome {
        ProbeOutcome::Success => (mid, hi),
        ProbeOutcome::Failure => (lo, mid),
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

fn resolve_prompt(context: u32, prompt_file: Option<&Path>) -> Result<String, String> {
    if let Some(path) = prompt_file {
        return std::fs::read_to_string(path).map_err(|e| e.to_string());
    }
    Ok(build_context_fill_prompt(context))
}

fn run_params_json(
    profile: &ModelProfile,
    context: u32,
    prompt_file: Option<&Path>,
) -> Result<String, String> {
    let payload = serde_json::json!({
        "context_size": context,
        "max_tokens": profile_max_tokens(profile),
        "prompt_file": prompt_file.map(|p| p.display().to_string()),
        "fill_ratio": if prompt_file.is_some() { 0.0 } else { 0.8 },
    });
    serde_json::to_string(&payload).map_err(|e| e.to_string())
}

async fn emit_progress(event_tx: &broadcast::Sender<CoreEvent>, level: &str, message: &str) {
    let _ = event_tx.send(CoreEvent::Log {
        level: level.into(),
        message: message.into(),
    });
}

fn relay_event(progress_tx: &Option<broadcast::Sender<CoreEvent>>, event: &CoreEvent) {
    if let Some(tx) = progress_tx {
        let _ = tx.send(event.clone());
    }
}

pub(crate) fn spawn_sample_collector(
    event_tx: &broadcast::Sender<CoreEvent>,
) -> (Arc<Mutex<SampleRecorder>>, tokio::task::JoinHandle<()>) {
    let recorder = Arc::new(Mutex::new(SampleRecorder::new()));
    let rec = recorder.clone();
    let mut sample_rx = event_tx.subscribe();
    let task = tokio::spawn(async move {
        loop {
            match sample_rx.recv().await {
                Ok(CoreEvent::Sample(sample)) => {
                    rec.lock().await.on_sample(&sample);
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    (recorder, task)
}

pub(crate) async fn take_recorder(
    recorder: Arc<Mutex<SampleRecorder>>,
    collector_task: tokio::task::JoinHandle<()>,
) -> SampleRecorder {
    collector_task.abort();
    let _ = collector_task.await;
    let mutex = Arc::try_unwrap(recorder).unwrap_or_else(|_| {
        panic!("sample collector still holds recorder arc")
    });
    mutex.into_inner()
}

/// 벤치 런 레코드를 먼저 생성하고 run_id를 반환한다 (UI 즉시 표시용).
pub fn allocate_bench_run(
    db: &Database,
    config: &BenchRunConfig,
    prompt_file: Option<&Path>,
) -> Result<i64, String> {
    let model_id = db
        .upsert_model(&config.profile)
        .map_err(|e| e.to_string())?;
    let params_json = run_params_json(&config.profile, config.context, prompt_file)?;
    db.insert_run(
        model_id,
        config.kind.as_str(),
        config.sweep_id,
        Some(config.context),
        &params_json,
    )
    .map_err(|e| e.to_string())
}

pub async fn run_single(
    db: &Database,
    config: BenchRunConfig,
    prompt_file: Option<&Path>,
) -> Result<RunResult, String> {
    let run_id = allocate_bench_run(db, &config, prompt_file)?;
    run_single_with_id(db, run_id, config, prompt_file).await
}

pub async fn run_single_with_id(
    db: &Database,
    run_id: i64,
    config: BenchRunConfig,
    prompt_file: Option<&Path>,
) -> Result<RunResult, String> {
    let prompt = match &config.prompt {
        Some(p) => p.clone(),
        None => resolve_prompt(config.context, prompt_file)?,
    };

    let cycle = execute_cycle(&config, &prompt, run_id).await?;

    let status = cycle.outcome.status_str().to_string();
    if let Some(ref msg) = cycle.error_message {
        eprintln!("{msg}");
    }

    db.insert_samples(run_id, cycle.recorder.samples())
        .map_err(|e| e.to_string())?;
    if let Some(ref s) = cycle.stats {
        if client::is_token_benchmark(s) {
            db.insert_results(
                run_id,
                s,
                cycle.recorder.peak_phys_footprint_bytes(),
                cycle.recorder.peak_mlx_active_bytes(),
                cycle.recorder.avg_cpu_pct(),
            )
            .map_err(|e| e.to_string())?;
        } else {
            db.insert_timing_results(
                run_id,
                s.ttft_ms,
                cycle.recorder.peak_phys_footprint_bytes(),
                cycle.recorder.peak_mlx_active_bytes(),
                cycle.recorder.avg_cpu_pct(),
            )
            .map_err(|e| e.to_string())?;
        }
    }
    db.finish_run(
        run_id,
        &status,
        cycle.error_message.as_deref(),
    )
    .map_err(|e| e.to_string())?;

    Ok(RunResult {
        run_id,
        kind: config.kind,
        status,
        context_size: config.context,
        stats: cycle.stats,
        peak_phys_footprint_bytes: cycle.recorder.peak_phys_footprint_bytes(),
        peak_mlx_active_bytes: cycle.recorder.peak_mlx_active_bytes(),
        error_message: cycle.error_message,
    })
}

async fn execute_cycle(
    config: &BenchRunConfig,
    prompt: &str,
    run_id: i64,
) -> Result<CycleOutcome, String> {
    let mut handle = LifecycleHandle::spawn();
    register_bench_abort_tx(handle.command_tx.clone());
    let (recorder_arc, collector_task) = spawn_sample_collector(&handle.event_tx);
    let mut saw_watchdog_kill = false;
    let mut reached_ready = false;
    let mut stats: Option<StreamStats> = None;
    let mut error_message: Option<String> = None;
    let mut progress_events = handle.event_tx.subscribe();

    let start = StartParams {
        profile: config.profile.clone(),
        context: config.context,
        mem_limit_gb: config.mem_limit_gb,
        port: config.port,
        python_dir: config.python_dir.clone(),
        child_spec: config.child_spec.clone(),
    };

    let start_msg = format!(
        "run {run_id}: starting server (context={})",
        config.context
    );
    emit_progress(&handle.event_tx, "info", &start_msg).await;
    relay_event(
        &config.progress_tx,
        &CoreEvent::Log {
            level: "info".into(),
            message: start_msg,
        },
    );

    if handle.command_tx.send(Command::Start(start)).await.is_err() {
        let recorder = take_recorder(recorder_arc, collector_task).await;
        return Ok(CycleOutcome {
            outcome: RunOutcome::Failed,
            stats: None,
            recorder,
            error_message: Some("failed to send start command".into()),
        });
    }

    let load_deadline =
        Duration::from_secs(config.profile.load_timeout_sec.saturating_add(30));
    let load_result = timeout(load_deadline, async {
        loop {
            tokio::select! {
                event = progress_events.recv() => {
                    match event {
                        Ok(ev) => {
                            relay_event(&config.progress_tx, &ev);
                            match &ev {
                                CoreEvent::WatchdogKill => saw_watchdog_kill = true,
                                CoreEvent::StateChanged { to, .. } if *to == LifecycleState::Ready => {
                                    reached_ready = true;
                                    break;
                                }
                                CoreEvent::StateChanged { to, .. } if *to == LifecycleState::Idle && !reached_ready => {
                                    break;
                                }
                                _ => {}
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
                _ = handle.state_rx.changed() => {
                    let state = *handle.state_rx.borrow();
                    if state == LifecycleState::Ready {
                        reached_ready = true;
                        break;
                    }
                    if state == LifecycleState::Idle && !reached_ready {
                        break;
                    }
                    if saw_watchdog_kill && state == LifecycleState::Idle {
                        break;
                    }
                }
            }
        }
    })
    .await;

    if load_result.is_err() {
        let _ = handle.command_tx.send(Command::Stop).await;
        handle.wait_for_state(LifecycleState::Idle).await;
        let recorder = take_recorder(recorder_arc, collector_task).await;
        return Ok(CycleOutcome {
            outcome: RunOutcome::Failed,
            stats: None,
            recorder,
            error_message: Some(format!(
                "load timeout exceeded ({}s)",
                config.profile.load_timeout_sec.saturating_add(30)
            )),
        });
    }

    let mut outcome = if saw_watchdog_kill {
        RunOutcome::AbortedWatchdog
    } else if !reached_ready {
        RunOutcome::Failed
    } else {
        RunOutcome::Completed
    };

    if outcome == RunOutcome::Failed && error_message.is_none() {
        error_message = Some("server failed to reach ready state".into());
    }
    if outcome == RunOutcome::AbortedWatchdog && error_message.is_none() {
        error_message = Some("aborted by watchdog (memory limit exceeded)".into());
    }

    if reached_ready && outcome == RunOutcome::Completed {
        let measure_msg = format!("run {run_id}: measuring throughput");
        emit_progress(&handle.event_tx, "info", &measure_msg).await;
        relay_event(
            &config.progress_tx,
            &CoreEvent::Log {
                level: "info".into(),
                message: measure_msg,
            },
        );

        let port = {
            let mut port_rx = handle.port_rx.clone();
            loop {
                if let Some(port) = *port_rx.borrow() {
                    break port;
                }
                if port_rx.changed().await.is_err() {
                    break 0;
                }
            }
        };

        if port == 0 {
            let _ = handle.command_tx.send(Command::Stop).await;
            handle.wait_for_state(LifecycleState::Idle).await;
            let recorder = take_recorder(recorder_arc, collector_task).await;
            return Ok(CycleOutcome {
                outcome: RunOutcome::Failed,
                stats: None,
                recorder,
                error_message: Some("server port unavailable".into()),
            });
        }

        let http = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .map_err(|e| e.to_string())?;

        let _ = handle.command_tx.send(Command::BeginWork).await;

        let model_type = config.profile.model_type.clone();
        let http_worker = http.clone();
        let model_id = config.profile.id.clone();
        let prompt_owned = prompt.to_string();
        let max_tokens = profile_max_tokens(&config.profile);
        let event_tx_chat = handle.event_tx.clone();
        let image_path = config.image_path.clone();
        let audio_path = config.audio_path.clone();

        let mut measure_task = tokio::spawn(async move {
            match model_type.as_str() {
                "llm" => {
                    client::stream_chat_completion(
                        &http_worker,
                        port,
                        &model_id,
                        &prompt_owned,
                        max_tokens,
                        Some(event_tx_chat),
                    )
                    .await
                    .map(|(text, s)| (Some(text), s))
                }
                "multimodal" => {
                    client::stream_chat_completion_with_image(
                        &http_worker,
                        port,
                        &model_id,
                        &prompt_owned,
                        image_path.as_deref(),
                        max_tokens,
                        Some(event_tx_chat),
                    )
                    .await
                    .map(|(text, s)| (Some(text), s))
                }
                "asr" => {
                    let audio = audio_path.ok_or_else(|| "audio path required for asr".to_string())?;
                    let (text, elapsed_ms) =
                        client::transcribe_audio(&http_worker, port, &audio).await?;
                    Ok((Some(text), client::timing_only_stats(elapsed_ms)))
                }
                "tts" => {
                    let elapsed_ms =
                        client::speech_audio(&http_worker, port, &prompt_owned, None).await?;
                    Ok((None, client::timing_only_stats(elapsed_ms)))
                }
                "image_gen" => {
                    let elapsed_ms =
                        client::generate_image(&http_worker, port, &prompt_owned).await?;
                    Ok((None, client::timing_only_stats(elapsed_ms)))
                }
                other => Err(format!("unsupported model_type for bench: {other}")),
            }
        });

        let measure_deadline = Duration::from_secs(60);
        let measure_result = timeout(measure_deadline, async {
            loop {
                tokio::select! {
                    event = progress_events.recv() => {
                        match event {
                            Ok(ev) => {
                                relay_event(&config.progress_tx, &ev);
                                if matches!(ev, CoreEvent::WatchdogKill) {
                                    saw_watchdog_kill = true;
                                    measure_task.abort();
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                    measure = &mut measure_task => {
                        return measure
                            .map_err(|e| e.to_string())
                            .and_then(|inner| inner.map(|(_, s)| s));
                    }
                }
            }
            Err("aborted during measurement".to_string())
        })
        .await;

        let _ = handle.command_tx.send(Command::EndWork).await;

        match measure_result {
            Ok(Ok(s)) => {
                stats = Some(s);
            }
            Ok(Err(err)) => {
                measure_task.abort();
                error_message = Some(err.clone());
                if saw_watchdog_kill {
                    outcome = RunOutcome::AbortedWatchdog;
                } else {
                    outcome = RunOutcome::Failed;
                }
            }
            Err(_) => {
                measure_task.abort();
                if saw_watchdog_kill {
                    outcome = RunOutcome::AbortedWatchdog;
                    error_message = Some("aborted by watchdog during measurement".into());
                } else {
                    outcome = RunOutcome::Failed;
                    error_message = Some("measurement timeout or aborted".into());
                }
            }
        }

        if saw_watchdog_kill {
            outcome = RunOutcome::AbortedWatchdog;
            if error_message.is_none() {
                error_message = Some("aborted by watchdog during measurement".into());
            }
        }
    }

    let cycle = finalize_cycle(handle, outcome, stats, recorder_arc, collector_task, error_message).await?;
    Ok(cycle)
}

async fn finalize_cycle(
    handle: LifecycleHandle,
    outcome: RunOutcome,
    stats: Option<StreamStats>,
    recorder_arc: Arc<Mutex<SampleRecorder>>,
    collector_task: tokio::task::JoinHandle<()>,
    error_message: Option<String>,
) -> Result<CycleOutcome, String> {
    clear_bench_abort_tx();
    let _ = handle.command_tx.send(Command::Stop).await;
    handle.wait_for_state(LifecycleState::Idle).await;
    let recorder = take_recorder(recorder_arc, collector_task).await;
    Ok(CycleOutcome {
        outcome,
        stats,
        recorder,
        error_message,
    })
}

fn resolve_bench_prompt(
    profile: &ModelProfile,
    context: u32,
    prompt_file: Option<&Path>,
    image_path: Option<&Path>,
) -> Result<Option<String>, String> {
    if prompt_file.is_some() {
        return Ok(None);
    }

    let prompt = match profile.model_type.as_str() {
        "tts" => TTS_BENCH_PROMPT.to_string(),
        "image_gen" => IMAGE_GEN_BENCH_PROMPT.to_string(),
        "multimodal" => {
            if image_path.is_some() {
                MULTIMODAL_BENCH_PROMPT.to_string()
            } else {
                resolve_prompt(context, None)?
            }
        }
        "asr" => String::new(),
        _ => resolve_prompt(context, None)?,
    };
    Ok(Some(prompt))
}

pub async fn run_bench_single(
    db: &Database,
    profile: ModelProfile,
    context: u32,
    prompt_file: Option<&Path>,
    image_path: Option<PathBuf>,
    audio_path: Option<PathBuf>,
    mem_limit_gb: Option<f64>,
    python_dir: PathBuf,
    child_spec: Option<ChildSpec>,
    port: Option<u16>,
    progress_tx: Option<broadcast::Sender<CoreEvent>>,
) -> Result<RunResult, String> {
    let prompt = resolve_bench_prompt(
        &profile,
        context,
        prompt_file,
        image_path.as_deref(),
    )?;
    let config = BenchRunConfig {
        profile,
        context,
        prompt,
        image_path,
        audio_path,
        mem_limit_gb,
        kind: BenchKind::Single,
        sweep_id: None,
        python_dir,
        child_spec,
        port,
        progress_tx,
    };
    run_single(db, config, prompt_file).await
}

pub async fn run_context_sweep(
    db: &Database,
    profile: ModelProfile,
    steps: Vec<u32>,
    mem_limit_gb: Option<f64>,
    python_dir: PathBuf,
    child_spec: Option<ChildSpec>,
    port: Option<u16>,
    prompt_file: Option<&Path>,
    progress_tx: Option<broadcast::Sender<CoreEvent>>,
) -> Result<SweepSummary, String> {
    let config_json = serde_json::json!({
        "profile_id": profile.id,
        "steps": steps,
        "mem_limit_gb": mem_limit_gb,
    });
    let sweep_id = db
        .insert_sweep("context_sweep", &config_json.to_string())
        .map_err(|e| e.to_string())?;

    emit_progress_msg(
        &progress_tx,
        &format!("sweep {sweep_id}: {} steps", steps.len()),
    )
    .await;

    let mut rows = Vec::new();
    let mut skipped_remaining = false;

    for (idx, &step) in steps.iter().enumerate() {
        emit_progress_msg(
            &progress_tx,
            &format!("sweep step {}/{}: context={step}", idx + 1, steps.len()),
        )
        .await;

        let prompt = resolve_bench_prompt(&profile, step, prompt_file, None)?;
        let config = BenchRunConfig {
            profile: profile.clone(),
            context: step,
            prompt,
            image_path: None,
            audio_path: None,
            mem_limit_gb,
            kind: BenchKind::SweepStep,
            sweep_id: Some(sweep_id),
            python_dir: python_dir.clone(),
            child_spec: child_spec.clone(),
            port,
            progress_tx: progress_tx.clone(),
        };
        let result = run_single(db, config, prompt_file).await?;

        let row = SweepSummaryRow {
            context_size: step,
            tokens_in: result.stats.as_ref().map(|s| s.tokens_in).unwrap_or(0),
            ttft_ms: result.stats.as_ref().map(|s| s.ttft_ms).unwrap_or(0.0),
            decode_tps: result.stats.as_ref().and_then(|s| s.decode_tps),
            peak_phys_footprint_bytes: result.peak_phys_footprint_bytes,
            peak_mlx_active_bytes: result.peak_mlx_active_bytes,
            status: result.status.clone(),
        };
        rows.push(row);

        if result.status == "aborted_watchdog" || result.status == "failed" {
            skipped_remaining = idx + 1 < steps.len();
            if skipped_remaining {
                emit_progress_msg(
                    &progress_tx,
                    &format!(
                        "sweep {sweep_id}: step {step} {} — skipping remaining larger steps",
                        result.status
                    ),
                )
                .await;
            }
            break;
        }
    }

    Ok(SweepSummary {
        sweep_id,
        rows,
        skipped_remaining,
    })
}

pub async fn run_limit_search(
    db: &Database,
    profile: ModelProfile,
    min_context: u64,
    max_context: u64,
    granularity: u64,
    mem_limit_gb: Option<f64>,
    python_dir: PathBuf,
    child_spec: Option<ChildSpec>,
    port: Option<u16>,
    prompt_file: Option<&Path>,
    progress_tx: Option<broadcast::Sender<CoreEvent>>,
) -> Result<LimitSearchSummary, String> {
    let config_json = serde_json::json!({
        "profile_id": profile.id,
        "min": min_context,
        "max": max_context,
        "granularity": granularity,
        "mem_limit_gb": mem_limit_gb,
    });
    let sweep_id = db
        .insert_sweep("limit_search", &config_json.to_string())
        .map_err(|e| e.to_string())?;

    let mut lo = min_context;
    let mut hi = max_context;
    let mut attempts = Vec::new();

    while let Some(mid) = next_probe(lo, hi, granularity) {
        emit_progress_msg(
            &progress_tx,
            &format!("limit search: probing context={mid} (lo={lo}, hi={hi})"),
        )
        .await;

        let context = mid.min(u32::MAX as u64) as u32;
        let prompt = resolve_bench_prompt(&profile, context, prompt_file, None)?;
        let config = BenchRunConfig {
            profile: profile.clone(),
            context,
            prompt,
            image_path: None,
            audio_path: None,
            mem_limit_gb,
            kind: BenchKind::LimitSearch,
            sweep_id: Some(sweep_id),
            python_dir: python_dir.clone(),
            child_spec: child_spec.clone(),
            port,
            progress_tx: progress_tx.clone(),
        };
        let result = run_single(db, config, prompt_file).await?;
        attempts.push(LimitAttemptRow {
            context_size: mid,
            status: result.status.clone(),
        });

        let probe_outcome = if result.status == "completed" {
            ProbeOutcome::Success
        } else {
            ProbeOutcome::Failure
        };
        (lo, hi) = apply_probe_result(lo, hi, mid, probe_outcome);
    }

    Ok(LimitSearchSummary {
        sweep_id,
        limit_context: lo,
        attempts,
    })
}

pub async fn run_ab_battle(
    db: &Database,
    profile_a: ModelProfile,
    profile_b: ModelProfile,
    context: u32,
    prompt_file: Option<&Path>,
    mem_limit_gb: Option<f64>,
    python_dir: PathBuf,
    child_spec: Option<ChildSpec>,
    port: Option<u16>,
    progress_tx: Option<broadcast::Sender<CoreEvent>>,
) -> Result<AbBattleSummary, String> {
    let config_json = serde_json::json!({
        "a": profile_a.id,
        "b": profile_b.id,
        "context": context,
    });
    let sweep_id = db
        .insert_sweep("ab_battle", &config_json.to_string())
        .map_err(|e| e.to_string())?;

    let mut rows = Vec::new();
    for profile in [profile_a, profile_b] {
        let prompt = resolve_bench_prompt(&profile, context, prompt_file, None)?;
        let config = BenchRunConfig {
            profile: profile.clone(),
            context,
            prompt,
            image_path: None,
            audio_path: None,
            mem_limit_gb,
            kind: BenchKind::AbBattle,
            sweep_id: Some(sweep_id),
            python_dir: python_dir.clone(),
            child_spec: child_spec.clone(),
            port,
            progress_tx: progress_tx.clone(),
        };
        let result = run_single(db, config, prompt_file).await?;
        rows.push(AbBattleRow {
            profile_id: profile.id,
            run_id: result.run_id,
            decode_tps: result.stats.as_ref().and_then(|s| s.decode_tps),
            ttft_ms: result.stats.as_ref().map(|s| s.ttft_ms),
            peak_phys_footprint_bytes: result.peak_phys_footprint_bytes,
            status: result.status,
        });
    }

    let winner = pick_winner(&rows);

    Ok(AbBattleSummary {
        sweep_id,
        context_size: context,
        rows,
        winner,
    })
}

pub async fn run_quant_compare(
    db: &Database,
    profiles: Vec<ModelProfile>,
    context: u32,
    prompt_file: Option<&Path>,
    mem_limit_gb: Option<f64>,
    python_dir: PathBuf,
    child_spec: Option<ChildSpec>,
    port: Option<u16>,
    progress_tx: Option<broadcast::Sender<CoreEvent>>,
) -> Result<QuantCompareSummary, String> {
    if profiles.is_empty() {
        return Err("at least one profile required".into());
    }

    let profile_ids: Vec<&str> = profiles.iter().map(|p| p.id.as_str()).collect();
    let config_json = serde_json::json!({
        "profiles": profile_ids,
        "context": context,
    });
    let sweep_id = db
        .insert_sweep("quant_compare", &config_json.to_string())
        .map_err(|e| e.to_string())?;

    let mut rows = Vec::new();
    for profile in profiles {
        let prompt = resolve_bench_prompt(&profile, context, prompt_file, None)?;
        let config = BenchRunConfig {
            profile: profile.clone(),
            context,
            prompt,
            image_path: None,
            audio_path: None,
            mem_limit_gb,
            kind: BenchKind::QuantCompare,
            sweep_id: Some(sweep_id),
            python_dir: python_dir.clone(),
            child_spec: child_spec.clone(),
            port,
            progress_tx: progress_tx.clone(),
        };
        let result = run_single(db, config, prompt_file).await?;
        rows.push(QuantCompareRow {
            profile_id: profile.id,
            run_id: result.run_id,
            decode_tps: result.stats.as_ref().and_then(|s| s.decode_tps),
            peak_phys_footprint_bytes: result.peak_phys_footprint_bytes,
            status: result.status,
        });
    }

    Ok(QuantCompareSummary {
        sweep_id,
        context_size: context,
        rows,
    })
}

fn pick_winner(rows: &[AbBattleRow]) -> Option<String> {
    let completed: Vec<_> = rows
        .iter()
        .filter(|r| r.status == "completed" && r.decode_tps.is_some())
        .collect();
    if completed.len() < 2 {
        return None;
    }
    let best = completed
        .iter()
        .max_by(|a, b| {
            a.decode_tps
                .unwrap_or(0.0)
                .partial_cmp(&b.decode_tps.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;
    Some(best.profile_id.clone())
}

async fn emit_progress_msg(progress_tx: &Option<broadcast::Sender<CoreEvent>>, message: &str) {
    if let Some(tx) = progress_tx {
        emit_progress(tx, "info", message).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_step_tokens() {
        assert_eq!(parse_step_token("1k").unwrap(), 1024);
        assert_eq!(parse_step_token("2K").unwrap(), 2048);
        assert_eq!(parse_step_token("4096").unwrap(), 4096);
        assert!(parse_step_token("").is_err());
    }

    #[test]
    fn parse_steps_list_csv() {
        let steps = parse_steps_list("1k,2k,4096").unwrap();
        assert_eq!(steps, vec![1024, 2048, 4096]);
    }

    #[test]
    fn context_fill_length() {
        let prompt = build_context_fill_prompt(4096);
        let expected = estimate_chars_for_context(4096, 0.8);
        assert!(prompt.len() <= expected);
        assert!(prompt.len() >= expected.saturating_sub(4));
        assert!(prompt.len() > 1000);
    }

    #[test]
    fn binary_search_next_probe() {
        assert_eq!(next_probe(512, 8192, 2048), Some(4352));
        assert_eq!(next_probe(512, 2048, 2048), None);
        assert_eq!(next_probe(6000, 8192, 2048), Some(7096));
    }

    #[test]
    fn binary_search_apply_result() {
        assert_eq!(
            apply_probe_result(512, 8192, 4352, ProbeOutcome::Success),
            (4352, 8192)
        );
        assert_eq!(
            apply_probe_result(512, 8192, 4352, ProbeOutcome::Failure),
            (512, 4352)
        );
    }

    #[test]
    fn binary_search_converges_within_granularity() {
        let mut lo = 512u64;
        let mut hi = 8192u64;
        let granularity = 2048u64;
        let max_ok = 4096u64;

        while let Some(mid) = next_probe(lo, hi, granularity) {
            let ok = mid <= max_ok;
            (lo, hi) = apply_probe_result(
                lo,
                hi,
                mid,
                if ok {
                    ProbeOutcome::Success
                } else {
                    ProbeOutcome::Failure
                },
            );
        }
        assert!(hi.saturating_sub(lo) <= granularity);
        assert!(lo <= max_ok);
        assert!(hi > lo);
    }
}