use std::path::PathBuf;
use std::process::ExitCode;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use aidash_core::api::{self, ApiServerConfig, ApiState};
use aidash_core::auth;
use aidash_core::bench::{self, parse_steps_list};
use aidash_core::client::{self, StreamStats};
use aidash_core::db::{CompareRow, Database, DeleteSummary};
use aidash_core::eval;
use aidash_core::eval_templates::{self, EvalTemplateSummary};
use aidash_core::export::{self, ExportRequest};
use aidash_core::stats::{self, DEFAULT_OVERVIEW_CONTEXT};
use aidash_core::tps_tier::{self, format_decode_tps, format_decode_tps_opt, format_processing_time_ms};
use aidash_core::env_detect::{self, DoctorStatus};
use aidash_core::events::CoreEvent;
use aidash_core::lifecycle::{Command, LifecycleHandle, LifecycleState, StartParams};
use aidash_core::monitor::{run_system_monitor_loop, sample_system};
use aidash_core::profile;
use aidash_core::{eval_sets_dir, find_project_root, profiles_dir, python_dir, resolve_file_path};
use tokio::sync::broadcast;
use reqwest::Client;
use clap::{Args, Parser, Subcommand, ValueEnum};

const EXIT_NOT_IMPLEMENTED: i32 = 2;
const EXIT_ERROR: i32 = 1;

#[derive(Parser)]
#[command(name = "aidash", about = "AI Dashboard CLI")]
struct Cli {
    #[arg(long, global = true, help = "Machine-readable JSON output")]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Profile {
        #[command(subcommand)]
        command: ProfileCmd,
    },
    Serve {
        #[command(subcommand)]
        command: ServeCmd,
    },
    Status,
    Chat(ChatArgs),
    Bench {
        #[command(subcommand)]
        command: BenchCmd,
    },
    Eval {
        #[command(subcommand)]
        command: EvalCmd,
    },
    Monitor,
    Db {
        #[command(subcommand)]
        command: DbCmd,
    },
    Stats {
        #[command(subcommand)]
        command: StatsCmd,
    },
    Export {
        #[command(subcommand)]
        command: ExportCmd,
    },
    Doctor,
    Auth {
        #[command(subcommand)]
        command: AuthCmd,
    },
    Api {
        #[command(subcommand)]
        command: ApiCmd,
    },
}

#[derive(Subcommand)]
enum ProfileCmd {
    Generate(ProfileGenerateArgs),
    List,
    Validate { file: String },
}

#[derive(Args)]
#[group(required = true, multiple = false)]
struct ProfileGenerateArgs {
    #[arg(long)]
    hf: Option<String>,
    #[arg(long)]
    local: Option<String>,
}

#[derive(Subcommand)]
enum ServeCmd {
    Start(ServeStartArgs),
    Stop,
}

#[derive(Args)]
struct ServeStartArgs {
    #[arg(long)]
    profile: String,
    #[arg(long)]
    context: u32,
    #[arg(long)]
    port: Option<u16>,
    #[arg(long)]
    mem_limit_gb: Option<f64>,
}

#[derive(Args)]
struct ChatArgs {
    #[arg(long)]
    profile: String,
    #[arg(long)]
    prompt: Option<String>,
    #[arg(long)]
    image: Option<String>,
}

#[derive(Subcommand)]
enum BenchCmd {
    Run(BenchRunArgs),
    Sweep(BenchSweepArgs),
    Limit(BenchLimitArgs),
    Ab(BenchAbArgs),
    Quant(BenchQuantArgs),
}

#[derive(Args)]
struct BenchRunArgs {
    #[arg(long)]
    profile: String,
    #[arg(long)]
    context: u32,
    #[arg(long)]
    prompt_file: Option<String>,
    #[arg(long)]
    image: Option<String>,
    #[arg(long)]
    audio: Option<String>,
    #[arg(long)]
    mem_limit_gb: Option<f64>,
}

#[derive(Args)]
struct BenchSweepArgs {
    #[arg(long)]
    profile: String,
    #[arg(long)]
    steps: Option<String>,
    #[arg(long)]
    mem_limit_gb: Option<f64>,
    #[arg(long)]
    prompt_file: Option<String>,
}

#[derive(Args)]
struct BenchLimitArgs {
    #[arg(long)]
    profile: String,
    #[arg(long)]
    min: Option<u64>,
    #[arg(long)]
    max: Option<u64>,
    #[arg(long)]
    granularity: Option<u64>,
    #[arg(long)]
    mem_limit_gb: Option<f64>,
    #[arg(long)]
    prompt_file: Option<String>,
}

#[derive(Args)]
struct BenchAbArgs {
    #[arg(long)]
    a: String,
    #[arg(long)]
    b: String,
    #[arg(long)]
    context: Option<u32>,
    #[arg(long)]
    prompt_file: Option<String>,
}

#[derive(Args)]
struct BenchQuantArgs {
    #[arg(long, value_delimiter = ',')]
    profiles: Vec<String>,
    #[arg(long)]
    context: Option<u32>,
    #[arg(long)]
    prompt_file: Option<String>,
}

#[derive(Subcommand)]
enum ApiCmd {
    Serve(ApiServeArgs),
}

#[derive(Args)]
struct ApiServeArgs {
    #[arg(long, default_value_t = 8787)]
    port: u16,
}

#[derive(Subcommand)]
enum EvalCmd {
    Run(EvalRunArgs),
    Template(EvalTemplateArgs),
}

#[derive(Args)]
struct EvalRunArgs {
    #[arg(long)]
    profile: String,
}

#[derive(Args)]
struct EvalTemplateArgs {
    #[arg(long)]
    profile: String,
    #[arg(long)]
    context: u32,
}

#[derive(Subcommand)]
enum DbCmd {
    ListRuns { #[arg(long)] model: Option<String> },
    Export {
        #[arg(long)]
        run: u64,
        #[arg(long, value_enum)]
        format: ExportFormat,
    },
    Compare {
        #[arg(long, value_delimiter = ',')]
        models: Vec<String>,
        #[arg(long)]
        context: Option<i64>,
    },
    Delete {
        #[arg(long, group = "target")]
        run: Option<u64>,
        #[arg(long, group = "target")]
        model: Option<String>,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum StatsCmd {
    Overview {
        #[arg(long)]
        context: Option<i64>,
    },
    Model {
        id: String,
    },
}

#[derive(Clone, ValueEnum)]
enum ExportFormat {
    Json,
    Csv,
}

#[derive(Subcommand)]
enum ExportCmd {
    Card(ExportCardArgs),
}

#[derive(Args)]
struct ExportCardArgs {
    #[arg(long, value_delimiter = ',')]
    runs: Vec<u64>,
    #[arg(long)]
    out: String,
}

#[derive(Subcommand)]
enum AuthCmd {
    Status,
    Set,
    Import,
    Clear,
}

fn not_implemented(json: bool) -> ! {
    if json {
        eprintln!(r#"{{"error":"not implemented"}}"#);
    } else {
        eprintln!("not implemented");
    }
    std::process::exit(EXIT_NOT_IMPLEMENTED);
}

fn project_root_or_exit(json: bool) -> PathBuf {
    match find_project_root() {
        Some(root) => root,
        None => {
            if json {
                eprintln!(r#"{{"error":"project root not found"}}"#);
            } else {
                eprintln!("project root not found (expected profiles/ and python/)");
            }
            std::process::exit(EXIT_ERROR);
        }
    }
}

fn print_event(event: &CoreEvent, json: bool, bench_mode: bool) {
    if json {
        if let Ok(line) = serde_json::to_string(event) {
            println!("{line}");
        }
    } else {
        match event {
            CoreEvent::StateChanged { from, to } => {
                println!("state: {from:?} -> {to:?}");
            }
            CoreEvent::Log { level, message } => {
                println!("[{level}] {message}");
            }
            CoreEvent::WatchdogWarn => println!("watchdog: soft limit exceeded"),
            CoreEvent::WatchdogKill => println!("watchdog: hard limit exceeded, killing"),
            CoreEvent::Sample(sample) if !bench_mode => {
                println!(
                    "sample ts={} phys={} cpu={:.1}% avail={}",
                    sample.ts,
                    sample.phys_footprint_bytes,
                    sample.cpu_pct,
                    sample.sys_available_bytes
                );
            }
            CoreEvent::Token { .. } => {}
            CoreEvent::RunFinished { .. } => {}
            _ => {}
        }
    }
}

fn spawn_progress_printer(
    mut rx: broadcast::Receiver<CoreEvent>,
    json: bool,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            print_event(&event, json, true);
        }
    })
}

fn open_db_or_exit(json: bool) -> Database {
    match Database::open(None) {
        Ok(db) => db,
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            std::process::exit(EXIT_ERROR);
        }
    }
}

fn load_profile_or_exit(root: &std::path::Path, id: &str, json: bool) -> aidash_core::profile::ModelProfile {
    match profile::load_profile_by_id(&profiles_dir(root), id) {
        Ok(p) => p,
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            std::process::exit(EXIT_ERROR);
        }
    }
}

fn tier_json(decode_tps: f64) -> serde_json::Value {
    tps_tier::tps_tier(decode_tps).json_value()
}

fn bench_run_exit_code(result: &bench::RunResult) -> i32 {
    if result.status == "failed" || result.status == "aborted_watchdog" {
        EXIT_ERROR
    } else {
        0
    }
}

/// Unix millis 문자열 → "YYYY-MM-DD HH:MM UTC" (표시용. 파싱 불가 시 원문 그대로)
fn format_measured_at(millis_str: &str) -> String {
    let Ok(ms) = millis_str.parse::<i64>() else {
        return millis_str.to_string();
    };
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    // Howard Hinnant civil_from_days
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02} UTC",
        y,
        m,
        d,
        tod / 3600,
        (tod % 3600) / 60
    )
}

fn print_bench_result(result: &bench::RunResult, json: bool) {
    if json {
        let mut payload = serde_json::json!({
            "run_id": result.run_id,
            "status": result.status,
            "context_size": result.context_size,
            "stats": result.stats,
            "peak_phys_footprint_bytes": result.peak_phys_footprint_bytes,
            "peak_mlx_active_bytes": result.peak_mlx_active_bytes,
        });
        if let Some(stats) = &result.stats {
            if let Some(tps) = stats.decode_tps {
                payload["tier"] = tier_json(tps);
            }
        }
        if let Some(ref msg) = result.error_message {
            payload["error_message"] = serde_json::Value::String(msg.clone());
        }
        println!("{}", payload);
    } else if let Some(stats) = &result.stats {
        println!();
        println!("--- bench result ---");
        println!("Context:     {}", result.context_size);
        println!("Status:      {}", result.status);
        println!("TTFT:        {:.1} ms", stats.ttft_ms);
        if client::is_token_benchmark(stats) {
            println!("Prefill TPS: {:.1}", stats.prefill_tps);
            println!("Decode TPS:  {}", format_decode_tps_opt(stats.decode_tps));
            println!("Total TPS:   {:.1}", stats.total_tps);
            println!("Tokens in:   {}", stats.tokens_in);
            println!("Tokens out:  {}", stats.tokens_out);
        } else {
            println!("Decode TPS:  -");
        }
        println!(
            "Peak phys:   {} bytes ({:.2} GiB)",
            result.peak_phys_footprint_bytes,
            result.peak_phys_footprint_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
        );
        if result.peak_mlx_active_bytes > 0 {
            println!(
                "Peak MLX:    {} bytes ({:.2} GiB)",
                result.peak_mlx_active_bytes,
                result.peak_mlx_active_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
            );
        }
    } else {
        println!("Status: {}", result.status);
    }
}

async fn bench_run_cmd(args: BenchRunArgs, json: bool) -> i32 {
    let root = project_root_or_exit(json);
    let model_profile = load_profile_or_exit(&root, &args.profile, json);
    let db = open_db_or_exit(json);
    let (progress_tx, progress_rx) = broadcast::channel(128);
    let printer = spawn_progress_printer(progress_rx, json);

    let prompt_file = args.prompt_file.as_deref().map(std::path::Path::new);

    let image_path = if let Some(ref path) = args.image {
        match resolve_file_path(path, &root) {
            Ok(p) => Some(p),
            Err(e) => {
                if json {
                    eprintln!(r#"{{"error":"{e}"}}"#);
                } else {
                    eprintln!("{e}");
                }
                return EXIT_ERROR;
            }
        }
    } else {
        None
    };

    let audio_path = if let Some(ref path) = args.audio {
        match resolve_file_path(path, &root) {
            Ok(p) => Some(p),
            Err(e) => {
                if json {
                    eprintln!(r#"{{"error":"{e}"}}"#);
                } else {
                    eprintln!("{e}");
                }
                return EXIT_ERROR;
            }
        }
    } else if model_profile.model_type == "asr" {
        match resolve_file_path("tests/fixtures/test_audio.wav", &root) {
            Ok(p) => Some(p),
            Err(e) => {
                if json {
                    eprintln!(r#"{{"error":"{e}"}}"#);
                } else {
                    eprintln!("{e}");
                }
                return EXIT_ERROR;
            }
        }
    } else {
        None
    };
    let result = bench::run_bench_single(
        &db,
        model_profile,
        args.context,
        prompt_file,
        image_path,
        audio_path,
        args.mem_limit_gb,
        python_dir(&root),
        None,
        None,
        Some(progress_tx),
    )
    .await;

    printer.abort();

    match result {
        Ok(result) => {
            print_bench_result(&result, json);
            if !json {
                println!("run id {} 저장됨", result.run_id);
            }
            bench_run_exit_code(&result)
        }
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            EXIT_ERROR
        }
    }
}

async fn bench_sweep_cmd(args: BenchSweepArgs, json: bool) -> i32 {
    let root = project_root_or_exit(json);
    let model_profile = load_profile_or_exit(&root, &args.profile, json);
    let db = open_db_or_exit(json);

    let steps = if let Some(steps) = &args.steps {
        match parse_steps_list(steps) {
            Ok(v) => v,
            Err(e) => {
                if json {
                    eprintln!(r#"{{"error":"{e}"}}"#);
                } else {
                    eprintln!("{e}");
                }
                return EXIT_ERROR;
            }
        }
    } else {
        model_profile.context.sweep_steps.clone()
    };

    if steps.is_empty() {
        if json {
            eprintln!(r#"{{"error":"no sweep steps configured"}}"#);
        } else {
            eprintln!("no sweep steps configured");
        }
        return EXIT_ERROR;
    }

    let (progress_tx, progress_rx) = broadcast::channel(128);
    let printer = spawn_progress_printer(progress_rx, json);
    let prompt_file = args.prompt_file.as_deref().map(std::path::Path::new);

    let summary = bench::run_context_sweep(
        &db,
        model_profile,
        steps,
        args.mem_limit_gb,
        python_dir(&root),
        None,
        None,
        prompt_file,
        Some(progress_tx),
        true,
    )
    .await;

    printer.abort();

    match summary {
        Ok(summary) => {
            if json {
                if let Ok(line) = serde_json::to_string(&summary) {
                    println!("{line}");
                }
            } else {
                println!();
                println!("--- sweep summary (sweep id {}) ---", summary.sweep_id);
                println!(
                    "{:<8} {:<10} {:<10} {:<22} {:<14} {:<14} {}",
                    "context", "tokens_in", "TTFT(ms)", "decode TPS", "peak phys", "peak mlx", "status"
                );
                for row in &summary.rows {
                    println!(
                        "{:<8} {:<10} {:<10.1} {:<22} {:<14} {:<14} {}",
                        row.context_size,
                        row.tokens_in,
                        row.ttft_ms,
                        format_decode_tps_opt(row.decode_tps),
                        row.peak_phys_footprint_bytes,
                        row.peak_mlx_active_bytes,
                        row.status,
                    );
                }
                if summary.skipped_remaining {
                    println!("(remaining larger steps skipped after failure)");
                }
            }
            0
        }
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            EXIT_ERROR
        }
    }
}

async fn bench_limit_cmd(args: BenchLimitArgs, json: bool) -> i32 {
    let root = project_root_or_exit(json);
    let model_profile = load_profile_or_exit(&root, &args.profile, json);
    let db = open_db_or_exit(json);

    let min_context = args.min.unwrap_or(model_profile.context.min as u64);
    let max_context = args.max.unwrap_or(model_profile.context.max as u64);
    let granularity = args.granularity.unwrap_or(2048);

    let (progress_tx, progress_rx) = broadcast::channel(128);
    let printer = spawn_progress_printer(progress_rx, json);
    let prompt_file = args.prompt_file.as_deref().map(std::path::Path::new);

    let summary = bench::run_limit_search(
        &db,
        model_profile,
        min_context,
        max_context,
        granularity,
        args.mem_limit_gb,
        python_dir(&root),
        None,
        None,
        prompt_file,
        Some(progress_tx),
    )
    .await;

    printer.abort();

    match summary {
        Ok(summary) => {
            if json {
                if let Ok(line) = serde_json::to_string(&summary) {
                    println!("{line}");
                }
            } else {
                println!("Limit context: {}", summary.limit_context);
                println!("Sweep id: {}", summary.sweep_id);
            }
            0
        }
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            EXIT_ERROR
        }
    }
}

fn context_label(ctx: &stats::ContextPick) -> String {
    if ctx.substituted {
        format!("(ctx {}*)", ctx.actual)
    } else {
        ctx.actual.to_string()
    }
}

async fn db_list_runs_cmd(model: Option<String>, json: bool) -> i32 {
    let db = open_db_or_exit(json);
    match db.list_runs(model.as_deref()) {
        Ok(rows) => {
            if json {
                let enriched: Vec<serde_json::Value> = rows
                    .iter()
                    .map(|row| {
                        let mut v = serde_json::to_value(row).unwrap_or_default();
                        if let Some(tps) = row.decode_tps {
                            v["tier"] = tier_json(tps);
                        }
                        v
                    })
                    .collect();
                if let Ok(line) = serde_json::to_string(&enriched) {
                    println!("{line}");
                }
            } else if rows.is_empty() {
                println!("(no runs)");
            } else {
                println!(
                    "{:<6} {:<40} {:<14} {:<8} {:<18} {:<22} {:<12}",
                    "id", "model", "kind", "context", "status", "decode TPS", "peak RAM"
                );
                for row in rows {
                    let decode = row
                        .decode_tps
                        .map(format_decode_tps)
                        .unwrap_or_else(|| "-".into());
                    println!(
                        "{:<6} {:<40} {:<14} {:<8} {:<18} {:<22} {:<12}",
                        row.run_id,
                        row.profile_id,
                        row.kind,
                        row.context_size.unwrap_or(0),
                        row.status,
                        decode,
                        row.peak_phys_footprint_bytes.unwrap_or(0),
                    );
                }
            }
            0
        }
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            EXIT_ERROR
        }
    }
}

fn confirm_delete(summary: &DeleteSummary, json: bool) -> bool {
    if json {
        return true;
    }
    println!(
        "삭제 대상: runs={}, samples={}, results={}",
        summary.runs, summary.samples, summary.results
    );
    print!("계속하려면 y 입력: ");
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    line.trim().eq_ignore_ascii_case("y")
}

fn print_compare_rows(rows: &[CompareRow], json: bool) {
    if json {
        let enriched: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                let mut v = serde_json::to_value(row).unwrap_or_default();
                if let Some(tps) = row.decode_tps {
                    v["tier"] = tier_json(tps);
                }
                v
            })
            .collect();
        if let Ok(line) = serde_json::to_string(&enriched) {
            println!("{line}");
        }
        return;
    }

    println!(
        "{:<42} {:<10} {:<22} {:<10} {:<12} {:<10} {:<10} {}",
        "model", "context", "decode TPS", "TTFT(ms)", "peak RAM", "tok in", "tok out", "measured"
    );
    for row in rows {
        let ctx = if row.context_substituted {
            format!("{}*", row.context_actual)
        } else {
            row.context_actual.to_string()
        };
        let decode = row
            .decode_tps
            .map(format_decode_tps)
            .unwrap_or_else(|| "-".into());
        println!(
            "{:<42} {:<10} {:<22} {:<10.1} {:<12} {:<10} {:<10} {}",
            row.profile_id,
            ctx,
            decode,
            row.ttft_ms.unwrap_or(0.0),
            row.peak_phys_footprint_bytes.unwrap_or(0),
            row.tokens_in.unwrap_or(0),
            row.tokens_out.unwrap_or(0),
            row.measured_at
                .as_deref()
                .map(format_measured_at)
                .unwrap_or_else(|| "-".into()),
        );
    }

    let links: Vec<_> = rows
        .iter()
        .filter_map(|r| r.hf_url.as_ref().map(|u| (r.profile_id.as_str(), u.as_str())))
        .collect();
    if !links.is_empty() {
        println!();
        println!("링크:");
        for (id, url) in links {
            println!("  {id}: {url}");
        }
    }
}

async fn db_compare_cmd(models: Vec<String>, context: Option<i64>, json: bool) -> i32 {
    if models.len() < 2 {
        if json {
            eprintln!(r#"{{"error":"at least two models required"}}"#);
        } else {
            eprintln!("at least two models required");
        }
        return EXIT_ERROR;
    }
    let db = open_db_or_exit(json);
    let target = context.unwrap_or(DEFAULT_OVERVIEW_CONTEXT);
    match db.compare_models(&models, target) {
        Ok(rows) => {
            print_compare_rows(&rows, json);
            0
        }
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            EXIT_ERROR
        }
    }
}

async fn db_delete_cmd(run: Option<u64>, model: Option<String>, yes: bool, json: bool) -> i32 {
    if run.is_some() == model.is_some() {
        if json {
            eprintln!(r#"{{"error":"specify exactly one of --run or --model"}}"#);
        } else {
            eprintln!("specify exactly one of --run or --model");
        }
        return EXIT_ERROR;
    }

    let db = open_db_or_exit(json);

    if let Some(run_id) = run {
        match db.delete_run_summary(run_id as i64) {
            Ok(summary) => {
                if !yes && !confirm_delete(&summary, json) {
                    if !json {
                        println!("취소됨");
                    }
                    return 0;
                }
                match db.delete_run(run_id as i64) {
                    Ok(()) => {
                        if json {
                            println!(r#"{{"ok":true,"deleted":{}}}"#, summary.runs);
                        } else {
                            println!("run {run_id} 삭제됨 (samples={}, results={})", summary.samples, summary.results);
                        }
                        0
                    }
                    Err(e) => {
                        if json {
                            eprintln!(r#"{{"error":"{e}"}}"#);
                        } else {
                            eprintln!("{e}");
                        }
                        EXIT_ERROR
                    }
                }
            }
            Err(e) => {
                if json {
                    eprintln!(r#"{{"error":"{e}"}}"#);
                } else {
                    eprintln!("{e}");
                }
                EXIT_ERROR
            }
        }
    } else if let Some(profile_id) = model {
        match db.delete_model_summary(&profile_id) {
            Ok(summary) => {
                if !yes && !confirm_delete(&summary, json) {
                    if !json {
                        println!("취소됨");
                    }
                    return 0;
                }
                match db.delete_model(&profile_id) {
                    Ok(()) => {
                        if json {
                            println!(r#"{{"ok":true,"deleted_runs":{}}}"#, summary.runs);
                        } else {
                            println!(
                                "model {profile_id} 삭제됨 (runs={}, samples={}, results={})",
                                summary.runs, summary.samples, summary.results
                            );
                        }
                        0
                    }
                    Err(e) => {
                        if json {
                            eprintln!(r#"{{"error":"{e}"}}"#);
                        } else {
                            eprintln!("{e}");
                        }
                        EXIT_ERROR
                    }
                }
            }
            Err(e) => {
                if json {
                    eprintln!(r#"{{"error":"{e}"}}"#);
                } else {
                    eprintln!("{e}");
                }
                EXIT_ERROR
            }
        }
    } else {
        EXIT_ERROR
    }
}

async fn stats_overview_cmd(context: Option<i64>, json: bool) -> i32 {
    let db = open_db_or_exit(json);
    let target = context.unwrap_or(DEFAULT_OVERVIEW_CONTEXT);
    match db.stats_overview(target) {
        Ok(rows) => {
            if json {
                let enriched: Vec<serde_json::Value> = rows
                    .iter()
                    .map(|row| {
                        let mut v = serde_json::to_value(row).unwrap_or_default();
                        if let Some(tier) = row.tier {
                            v["tier"] = tier.json_value();
                        }
                        v
                    })
                    .collect();
                if let Ok(line) = serde_json::to_string(&enriched) {
                    println!("{line}");
                }
            } else if rows.is_empty() {
                println!("(no models with completed runs)");
            } else {
                println!("--- stats overview (requested ctx {}) ---", target);
                println!(
                    "{:<42} {:<10} {:<22} {}",
                    "model", "context", "decode TPS", "measured"
                );
                for row in &rows {
                    let metric = if row.decode_tps.is_some() {
                        format_decode_tps_opt(row.decode_tps)
                    } else {
                        format_processing_time_ms(row.ttft_ms)
                    };
                    println!(
                        "{:<42} {:<10} {:<22} {}",
                        row.profile_id,
                        context_label(&row.context),
                        metric,
                        row.measured_at
                            .as_deref()
                            .map(format_measured_at)
                            .unwrap_or_else(|| "-".into()),
                    );
                }
                let links: Vec<_> = rows
                    .iter()
                    .filter_map(|r| r.hf_url.as_ref().map(|u| (r.profile_id.as_str(), u.as_str())))
                    .collect();
                if !links.is_empty() {
                    println!();
                    println!("링크:");
                    for (id, url) in links {
                        println!("  {id}: {url}");
                    }
                }
            }
            0
        }
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            EXIT_ERROR
        }
    }
}

async fn stats_model_cmd(id: String, json: bool) -> i32 {
    let db = open_db_or_exit(json);
    match db.stats_model(&id) {
        Ok(stats) => {
            if json {
                let mut v = serde_json::to_value(&stats).unwrap_or_default();
                if let Some(tier) = stats.current_tier {
                    v["current_tier"] = tier.json_value();
                }
                if let Ok(line) = serde_json::to_string(&v) {
                    println!("{line}");
                }
            } else {
                println!("--- stats model: {} ---", stats.profile_id);
                if let Some(url) = &stats.hf_url {
                    println!("HF: {url}");
                }
                println!("총 런 수: {}", stats.total_runs);
                println!(
                    "최근 측정: {}",
                    stats
                        .latest_measured_at
                        .as_deref()
                        .map(format_measured_at)
                        .unwrap_or_else(|| "-".into())
                );
                if let (Some(tps), Some(tier)) = (stats.current_decode_tps, stats.current_tier) {
                    println!("현재 등급: {} ({:.1} TPS)", tier.display(), tps);
                }
                println!(
                    "Peak phys: {} bytes | Peak MLX: {} bytes",
                    stats.peak_phys_footprint_bytes, stats.peak_mlx_active_bytes
                );
                if !stats.by_context.is_empty() {
                    println!();
                    println!(
                        "{:<8} {:<8} {:<8} {:<8} {:<10} {}",
                        "ctx", "min", "avg", "max", "TTFT avg", "runs"
                    );
                    for row in &stats.by_context {
                        println!(
                            "{:<8} {:<8.1} {:<8.1} {:<8.1} {:<10.1} {}",
                            row.context_size,
                            row.decode_tps_min,
                            row.decode_tps_avg,
                            row.decode_tps_max,
                            row.ttft_avg_ms,
                            row.run_count,
                        );
                    }
                }
            }
            0
        }
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            EXIT_ERROR
        }
    }
}

async fn db_export_cmd(run: u64, format: ExportFormat, json: bool) -> i32 {
    let db = open_db_or_exit(json);
    let result = match format {
        ExportFormat::Json => db.export_run_json(run as i64),
        ExportFormat::Csv => db.export_run_csv(run as i64),
    };
    match result {
        Ok(output) => {
            println!("{output}");
            0
        }
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            EXIT_ERROR
        }
    }
}

fn format_system_line(sample: &aidash_core::monitor::ResourceSample) -> String {
    format!(
        "cpu={:.1}% avail={} phys=0",
        sample.cpu_pct, sample.sys_available_bytes
    )
}

async fn serve_start(args: ServeStartArgs, json: bool) -> i32 {
    let root = project_root_or_exit(json);
    let model_profile = match profile::load_profile_by_id(&profiles_dir(&root), &args.profile) {
        Ok(p) => p,
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            return EXIT_ERROR;
        }
    };

    let mut handle = LifecycleHandle::spawn();
    let start = StartParams {
        profile: model_profile,
        context: args.context,
        mem_limit_gb: args.mem_limit_gb,
        port: args.port,
        python_dir: python_dir(&root),
        child_spec: None,
    };

    if handle.command_tx.send(Command::Start(start)).await.is_err() {
        return EXIT_ERROR;
    }

    let mut events = handle.event_tx.subscribe();
    let mut reached_active = false;
    let mut saw_ready = false;

    loop {
        tokio::select! {
            event = events.recv() => {
                match event {
                    Ok(ev) => {
                        if matches!(&ev, CoreEvent::StateChanged { to: LifecycleState::Ready, .. }) {
                            saw_ready = true;
                        }
                        print_event(&ev, json, false);
                    }
                    Err(_) => break,
                }
            }
            changed = handle.state_rx.changed() => {
                if changed.is_err() {
                    break;
                }
                let state = *handle.state_rx.borrow();
                if state != LifecycleState::Idle {
                    reached_active = true;
                }
                if reached_active && state == LifecycleState::Idle {
                    break;
                }
            }
            res = tokio::signal::ctrl_c() => {
                if res.is_ok() {
                    if saw_ready {
                        let _ = handle.command_tx.send(Command::Stop).await;
                        handle.wait_for_state(LifecycleState::Idle).await;
                    }
                }
                break;
            }
        }
    }

    0
}

async fn serve_stop(json: bool) -> i32 {
    if json {
        eprintln!(r#"{{"error":"use Ctrl-C on the serving process"}}"#);
    } else {
        eprintln!("use Ctrl-C on the serving process");
    }
    EXIT_NOT_IMPLEMENTED
}

async fn status_cmd(json: bool) -> i32 {
    let sample = sample_system();
    if json {
        if let Ok(line) = serde_json::to_string(&sample) {
            println!("{line}");
        }
    } else {
        println!("{}", format_system_line(&sample));
    }
    0
}

fn status_icon(status: DoctorStatus) -> &'static str {
    match status {
        DoctorStatus::Ok => "ok",
        DoctorStatus::Warn => "warn",
        DoctorStatus::Missing => "missing",
        DoctorStatus::Info => "info",
    }
}

async fn doctor_cmd(json: bool) -> i32 {
    let root = project_root_or_exit(json);
    let report = env_detect::run_doctor(root).await;

    if json {
        if let Ok(line) = serde_json::to_string(&report) {
            println!("{line}");
        }
    } else {
        println!("AI Dashboard environment report");
        println!("{:-<60}", "");
        let mut last_category = String::new();
        for item in &report.items {
            if item.category != last_category {
                if !last_category.is_empty() {
                    println!();
                }
                println!("[{}]", item.category);
                last_category = item.category.clone();
            }
            print!("  {:<28} {:<8} {}", item.name, status_icon(item.status), item.detail);
            if let Some(fix) = &item.fix_action {
                print!(" → {fix}");
            }
            println!();
        }
    }
    0
}

async fn auth_status_cmd(json: bool) -> i32 {
    let status = auth::build_auth_status().await;

    if json {
        if let Ok(line) = serde_json::to_string(&status) {
            println!("{line}");
        }
    } else {
        println!("HF token sources:");
        for src in &status.sources {
            let mark = if src.present { "yes" } else { "no" };
            println!("  {:<35} {}", src.source.label(), mark);
        }
        println!();
        match status.active_source {
            Some(src) => println!("Active source: {}", src.label()),
            None => println!("Active source: (none)"),
        }
        match &status.masked_token {
            Some(masked) => println!("Masked token: {masked}"),
            None => println!("Masked token: (none)"),
        }
        println!("Whoami: {}", status.whoami_user);
    }
    0
}

async fn auth_set_cmd(json: bool) -> i32 {
    match auth::set_token_from_stdin().await {
        Ok(username) => {
            if json {
                println!(r#"{{"ok":true,"username":"{username}"}}"#);
            } else {
                println!("Token saved to Keychain. User: {username}");
            }
            0
        }
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            EXIT_ERROR
        }
    }
}

async fn auth_import_cmd(json: bool) -> i32 {
    match auth::import_from_hf_cli().await {
        Ok(username) => {
            if json {
                println!(r#"{{"ok":true,"username":"{username}"}}"#);
            } else {
                println!("Token imported to Keychain. User: {username}");
            }
            0
        }
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            EXIT_ERROR
        }
    }
}

async fn auth_clear_cmd(json: bool) -> i32 {
    match auth::keychain_clear() {
        Ok(()) => {
            if json {
                println!(r#"{{"ok":true}}"#);
            } else {
                println!("Keychain token cleared.");
            }
            0
        }
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            EXIT_ERROR
        }
    }
}

fn profile_max_tokens(profile: &aidash_core::profile::ModelProfile) -> u32 {
    profile
        .default_params
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .filter(|v| *v > 0)
        .unwrap_or(512)
}

fn print_stream_stats(stats: &StreamStats, peak_mlx_active_bytes: u64, json: bool) {
    if json {
        let payload = serde_json::json!({
            "ttft_ms": stats.ttft_ms,
            "prefill_tps": stats.prefill_tps,
            "decode_tps": stats.decode_tps,
            "total_tps": stats.total_tps,
            "tokens_in": stats.tokens_in,
            "tokens_out": stats.tokens_out,
            "peak_mlx_active_bytes": peak_mlx_active_bytes,
        });
        println!("{}", payload);
    } else {
        println!();
        println!("--- stream stats ---");
        println!("TTFT:        {:.1} ms", stats.ttft_ms);
        println!("Prefill TPS: {:.1}", stats.prefill_tps);
        println!("Decode TPS:  {}", format_decode_tps_opt(stats.decode_tps));
        println!("Total TPS:   {:.1}", stats.total_tps);
        println!("Tokens in:   {}", stats.tokens_in);
        println!("Tokens out:  {}", stats.tokens_out);
        if peak_mlx_active_bytes > 0 {
            println!(
                "Peak MLX active: {} bytes ({:.2} GiB)",
                peak_mlx_active_bytes,
                peak_mlx_active_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
            );
        }
    }
}

async fn chat_cmd(args: ChatArgs, json: bool) -> i32 {
    let prompt = match args.prompt {
        Some(p) if !p.trim().is_empty() => p,
        _ => {
            if json {
                eprintln!(r#"{{"error":"--prompt is required for one-shot chat"}}"#);
            } else {
                eprintln!("--prompt is required for one-shot chat");
            }
            return EXIT_ERROR;
        }
    };

    let root = project_root_or_exit(json);
    let model_profile = match profile::load_profile_by_id(&profiles_dir(&root), &args.profile) {
        Ok(p) => p,
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            return EXIT_ERROR;
        }
    };

    let context = model_profile.context.default;
    let max_tokens = profile_max_tokens(&model_profile);
    let handle = LifecycleHandle::spawn();
    let mut events = handle.event_tx.subscribe();
    let peak_mlx = Arc::new(AtomicU64::new(0));
    let peak_mlx_task = peak_mlx.clone();

    let listener = tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            match event {
                CoreEvent::Token { text, .. } => {
                    if !json {
                        print!("{text}");
                        let _ = std::io::Write::flush(&mut std::io::stdout());
                    }
                }
                CoreEvent::Sample(sample) => {
                    if let Some(mlx) = sample.mlx_active_bytes {
                        peak_mlx_task.fetch_max(mlx, Ordering::Relaxed);
                    }
                }
                CoreEvent::Log { level, message } if !json => {
                    eprintln!("[{level}] {message}");
                }
                _ => {}
            }
        }
    });

    let start = StartParams {
        profile: model_profile.clone(),
        context,
        mem_limit_gb: None,
        port: None,
        python_dir: python_dir(&root),
        child_spec: None,
    };

    if handle.command_tx.send(Command::Start(start)).await.is_err() {
        listener.abort();
        return EXIT_ERROR;
    }

    handle.wait_for_state(LifecycleState::Ready).await;

    let port = {
        let mut port_rx = handle.port_rx.clone();
        loop {
            if let Some(port) = *port_rx.borrow() {
                break port;
            }
            if port_rx.changed().await.is_err() {
                listener.abort();
                return EXIT_ERROR;
            }
        }
    };

    let http = Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .unwrap_or_else(|_| Client::new());

    let _ = handle.command_tx.send(Command::BeginWork).await;
    let chat_result = client::stream_chat_completion(
        &http,
        port,
        &model_profile.id,
        &prompt,
        max_tokens,
        Some(handle.event_tx.clone()),
    )
    .await;
    let _ = handle.command_tx.send(Command::EndWork).await;

    let result = match chat_result {
        Ok((text, stats)) => {
            if !json {
                if !text.ends_with('\n') {
                    println!();
                }
                print_stream_stats(&stats, peak_mlx.load(Ordering::Relaxed), false);
            } else {
                print_stream_stats(&stats, peak_mlx.load(Ordering::Relaxed), true);
            }
            0
        }
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            EXIT_ERROR
        }
    };

    let _ = handle.command_tx.send(Command::Stop).await;
    handle.wait_for_state(LifecycleState::Idle).await;
    listener.abort();
    result
}

async fn profile_generate_cmd(args: ProfileGenerateArgs, json: bool) -> i32 {
    let root = project_root_or_exit(json);
    let profiles = profiles_dir(&root);

    let result = if let Some(repo) = args.hf {
        profile::generate_profile_hf(&profiles, &repo)
    } else if let Some(local) = args.local {
        profile::generate_profile_local(&profiles, PathBuf::from(local).as_path())
    } else {
        unreachable!("clap group ensures hf or local")
    };

    match result {
        Ok(path) => {
            if json {
                println!(r#"{{"path":"{}"}}"#, path.display());
            } else {
                println!("profile draft written: {}", path.display());
            }
            0
        }
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            EXIT_ERROR
        }
    }
}

async fn profile_list_cmd(json: bool) -> i32 {
    let root = project_root_or_exit(json);
    match profile::list_profiles(&profiles_dir(&root)) {
        Ok(rows) => {
            if json {
                if let Ok(line) = serde_json::to_string(&rows) {
                    println!("{line}");
                }
            } else if rows.is_empty() {
                println!("(no profiles)");
            } else {
                println!(
                    "{:<45} {:<12} {:<12} {:<8} {}",
                    "id", "backend", "model_type", "ctx", "file"
                );
                for row in rows {
                    println!(
                        "{:<45} {:<12} {:<12} {:<8} {}",
                        row.id, row.backend, row.model_type, row.context_default, row.filename
                    );
                }
            }
            0
        }
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            EXIT_ERROR
        }
    }
}

async fn profile_validate_cmd(file: String, json: bool) -> i32 {
    let path = PathBuf::from(&file);
    match profile::validate_profile_file(&path) {
        Ok(()) => {
            if json {
                println!(r#"{{"valid":true,"file":"{file}"}}"#);
            } else {
                println!("valid: {}", path.display());
            }
            0
        }
        Err(issues) => {
            if json {
                let msgs: Vec<String> = issues.iter().map(|i| i.to_string()).collect();
                eprintln!(r#"{{"valid":false,"errors":{}}}"#, serde_json::to_string(&msgs).unwrap_or_default());
            } else {
                eprintln!("validation failed for {}", path.display());
                for issue in issues {
                    eprintln!("  - {issue}");
                }
            }
            EXIT_ERROR
        }
    }
}

async fn export_card_cmd(args: ExportCardArgs, json: bool) -> i32 {
    let db = open_db_or_exit(json);
    let request = ExportRequest {
        run_ids: args.runs,
        output_dir: PathBuf::from(args.out),
    };
    match export::export_card(&db, &request) {
        Ok(path) => {
            if json {
                println!(r#"{{"path":"{}"}}"#, path.display());
            } else {
                println!("card exported: {}", path.display());
            }
            0
        }
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            EXIT_ERROR
        }
    }
}

fn print_eval_template_summary(summary: &EvalTemplateSummary, json: bool) {
    if json {
        if let Ok(line) = serde_json::to_string(summary) {
            println!("{line}");
        }
    } else {
        println!();
        println!("--- eval template ctx {} ---", summary.context_size);
        for item in &summary.items {
            println!(
                "  {} ({}) — {}점 · {}ms",
                item.template_id, item.description, item.score, item.elapsed_ms
            );
        }
        println!("총점: {}/100", summary.total_score);
    }
}

async fn eval_template_cmd(args: EvalTemplateArgs, json: bool) -> i32 {
    let root = project_root_or_exit(json);
    let model_profile = load_profile_or_exit(&root, &args.profile, json);
    let db = open_db_or_exit(json);

    let template_path = eval_sets_dir(&root).join("context_templates_ko.json");
    let template_set = match eval_templates::load_template_set(&template_path) {
        Ok(s) => s,
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            return EXIT_ERROR;
        }
    };

    if args.context < model_profile.context.min || args.context > model_profile.context.max {
        let msg = format!(
            "context {} out of profile range {}-{}",
            args.context, model_profile.context.min, model_profile.context.max
        );
        if json {
            eprintln!(r#"{{"error":"{msg}"}}"#);
        } else {
            eprintln!("{msg}");
        }
        return EXIT_ERROR;
    }

    let (progress_tx, progress_rx) = broadcast::channel(128);
    let printer = spawn_progress_printer(progress_rx, json);

    let summary = eval_templates::run_template_eval(
        &db,
        model_profile,
        args.context,
        &template_set,
        python_dir(&root),
        None,
        None,
        Some(progress_tx),
    )
    .await;

    printer.abort();

    match summary {
        Ok(summary) => {
            print_eval_template_summary(&summary, json);
            0
        }
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            EXIT_ERROR
        }
    }
}

async fn eval_run_cmd(args: EvalRunArgs, json: bool) -> i32 {
    let root = project_root_or_exit(json);
    let model_profile = load_profile_or_exit(&root, &args.profile, json);
    let db = open_db_or_exit(json);

    let eval_path = eval_sets_dir(&root).join("llm_basic_ko.json");
    let eval_set = match eval::load_eval_set(&eval_path) {
        Ok(s) => s,
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            return EXIT_ERROR;
        }
    };

    let (progress_tx, progress_rx) = broadcast::channel(128);
    let printer = spawn_progress_printer(progress_rx, json);

    let summary = eval::run_quality_eval(
        &db,
        model_profile,
        &eval_set,
        python_dir(&root),
        None,
        None,
        Some(progress_tx),
    )
    .await;

    printer.abort();

    match summary {
        Ok(summary) => {
            if json {
                if let Ok(line) = serde_json::to_string(&summary) {
                    println!("{line}");
                }
            } else {
                println!();
                println!("--- eval {} ---", eval_set.name);
                for item in &summary.items {
                    let mark = if item.correct { "○" } else { "✕" };
                    println!("{mark} {} (expected: {})", item.id, item.expected);
                }
                println!(
                    "총점: {}/{} ({:.0}%)",
                    summary.correct,
                    summary.total,
                    summary.score * 100.0
                );
                println!("run id {} · quality_score={:.2}", summary.run_id, summary.score);
            }
            0
        }
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            EXIT_ERROR
        }
    }
}

async fn bench_ab_cmd(args: BenchAbArgs, json: bool) -> i32 {
    let root = project_root_or_exit(json);
    let profile_a = load_profile_or_exit(&root, &args.a, json);
    let profile_b = load_profile_or_exit(&root, &args.b, json);
    let context = args.context.unwrap_or(profile_a.context.default);
    let db = open_db_or_exit(json);
    let prompt_file = args.prompt_file.as_deref().map(std::path::Path::new);

    let (progress_tx, progress_rx) = broadcast::channel(128);
    let printer = spawn_progress_printer(progress_rx, json);

    let summary = bench::run_ab_battle(
        &db,
        profile_a,
        profile_b,
        context,
        prompt_file,
        None,
        python_dir(&root),
        None,
        None,
        Some(progress_tx),
    )
    .await;

    printer.abort();

    match summary {
        Ok(summary) => {
            if json {
                if let Ok(line) = serde_json::to_string(&summary) {
                    println!("{line}");
                }
            } else {
                println!();
                println!("--- A/B battle (sweep {}) ctx {} ---", summary.sweep_id, summary.context_size);
                println!(
                    "{:<40} {:<8} {:<12} {:<22} {:<14} {}",
                    "model", "run", "TTFT(ms)", "decode TPS", "peak RAM", "status"
                );
                for row in &summary.rows {
                    let ttft = row.ttft_ms.map(|v| format!("{v:.1}")).unwrap_or_else(|| "-".into());
                    let decode = format_decode_tps_opt(row.decode_tps);
                    let ram_gib = row.peak_phys_footprint_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                    let winner = summary
                        .winner
                        .as_deref()
                        .map(|w| if w == row.profile_id { " ★" } else { "" })
                        .unwrap_or("");
                    println!(
                        "{:<40} {:<8} {:<12} {:<22} {:<14.2} {}{}",
                        row.profile_id,
                        row.run_id,
                        ttft,
                        decode,
                        ram_gib,
                        row.status,
                        winner
                    );
                }
                if let Some(w) = &summary.winner {
                    println!("승자: {w}");
                }
            }
            if summary.rows.iter().any(|r| r.status == "failed" || r.status == "aborted_watchdog") {
                EXIT_ERROR
            } else {
                0
            }
        }
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            EXIT_ERROR
        }
    }
}

async fn bench_quant_cmd(args: BenchQuantArgs, json: bool) -> i32 {
    let root = project_root_or_exit(json);
    let db = open_db_or_exit(json);
    if args.profiles.is_empty() {
        if json {
            eprintln!(r#"{{"error":"--profiles required"}}"#);
        } else {
            eprintln!("--profiles required");
        }
        return EXIT_ERROR;
    }

    let mut profiles = Vec::new();
    let default_ctx = args.context;
    for id in &args.profiles {
        profiles.push(load_profile_or_exit(&root, id, json));
    }
    let context = default_ctx.unwrap_or_else(|| profiles[0].context.default);
    let prompt_file = args.prompt_file.as_deref().map(std::path::Path::new);

    let (progress_tx, progress_rx) = broadcast::channel(128);
    let printer = spawn_progress_printer(progress_rx, json);

    let summary = bench::run_quant_compare(
        &db,
        profiles,
        context,
        prompt_file,
        None,
        python_dir(&root),
        None,
        None,
        Some(progress_tx),
    )
    .await;

    printer.abort();

    match summary {
        Ok(summary) => {
            if json {
                if let Ok(line) = serde_json::to_string(&summary) {
                    println!("{line}");
                }
            } else {
                println!();
                println!("--- quant compare (sweep {}) ctx {} ---", summary.sweep_id, summary.context_size);
                println!(
                    "{:<40} {:<8} {:<22} {:<14} {}",
                    "model", "run", "decode TPS", "peak RAM", "status"
                );
                for row in &summary.rows {
                    let decode = format_decode_tps_opt(row.decode_tps);
                    let ram_gib = row.peak_phys_footprint_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                    println!(
                        "{:<40} {:<8} {:<22} {:<14.2} {}",
                        row.profile_id, row.run_id, decode, ram_gib, row.status
                    );
                }
            }
            if summary.rows.iter().any(|r| r.status == "failed" || r.status == "aborted_watchdog") {
                EXIT_ERROR
            } else {
                0
            }
        }
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            EXIT_ERROR
        }
    }
}

async fn api_serve_cmd(args: ApiServeArgs, json: bool) -> i32 {
    let root = project_root_or_exit(json);
    let db = match Database::open(None) {
        Ok(db) => std::sync::Arc::new(db),
        Err(e) => {
            if json {
                eprintln!(r#"{{"error":"{e}"}}"#);
            } else {
                eprintln!("{e}");
            }
            return EXIT_ERROR;
        }
    };

    let state = ApiState {
        db,
        event_tx: api::create_event_bus(),
        project_root: root,
    };

    let config = ApiServerConfig {
        bind_addr: "127.0.0.1".into(),
        port: args.port,
    };

    if let Err(e) = api::serve(config, state).await {
        if json {
            eprintln!(r#"{{"error":"{e}"}}"#);
        } else {
            eprintln!("{e}");
        }
        return EXIT_ERROR;
    }
    0
}

async fn monitor_cmd(json: bool) -> i32 {
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    let result = run_system_monitor_loop(|sample| {
        if json {
            if let Ok(line) = serde_json::to_string(&sample) {
                println!("{line}");
            }
        } else {
            println!("{}", format_system_line(&sample));
        }
        true
    });

    tokio::select! {
        res = result => {
            if res.is_err() { EXIT_ERROR } else { 0 }
        }
        _ = &mut ctrl_c => 0,
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    let code = match cli.command {
        Commands::Serve { command } => match command {
            ServeCmd::Start(args) => serve_start(args, cli.json).await,
            ServeCmd::Stop => serve_stop(cli.json).await,
        },
        Commands::Status => status_cmd(cli.json).await,
        Commands::Chat(args) => chat_cmd(args, cli.json).await,
        Commands::Monitor => monitor_cmd(cli.json).await,
        Commands::Doctor => doctor_cmd(cli.json).await,
        Commands::Auth { command } => match command {
            AuthCmd::Status => auth_status_cmd(cli.json).await,
            AuthCmd::Set => auth_set_cmd(cli.json).await,
            AuthCmd::Import => auth_import_cmd(cli.json).await,
            AuthCmd::Clear => auth_clear_cmd(cli.json).await,
        },
        Commands::Bench { command } => match command {
            BenchCmd::Run(args) => bench_run_cmd(args, cli.json).await,
            BenchCmd::Sweep(args) => bench_sweep_cmd(args, cli.json).await,
            BenchCmd::Limit(args) => bench_limit_cmd(args, cli.json).await,
            BenchCmd::Ab(args) => bench_ab_cmd(args, cli.json).await,
            BenchCmd::Quant(args) => bench_quant_cmd(args, cli.json).await,
        },
        Commands::Db { command } => match command {
            DbCmd::ListRuns { model } => db_list_runs_cmd(model, cli.json).await,
            DbCmd::Export { run, format } => db_export_cmd(run, format, cli.json).await,
            DbCmd::Compare { models, context } => {
                db_compare_cmd(models, context, cli.json).await
            }
            DbCmd::Delete { run, model, yes } => db_delete_cmd(run, model, yes, cli.json).await,
        },
        Commands::Stats { command } => match command {
            StatsCmd::Overview { context } => stats_overview_cmd(context, cli.json).await,
            StatsCmd::Model { id } => stats_model_cmd(id, cli.json).await,
        },
        Commands::Profile { command } => match command {
            ProfileCmd::Generate(args) => profile_generate_cmd(args, cli.json).await,
            ProfileCmd::List => profile_list_cmd(cli.json).await,
            ProfileCmd::Validate { file } => profile_validate_cmd(file, cli.json).await,
        },
        Commands::Eval { command } => match command {
            EvalCmd::Run(args) => eval_run_cmd(args, cli.json).await,
            EvalCmd::Template(args) => eval_template_cmd(args, cli.json).await,
        },
        Commands::Export { command } => match command {
            ExportCmd::Card(args) => export_card_cmd(args, cli.json).await,
        },
        Commands::Api { command } => match command {
            ApiCmd::Serve(args) => api_serve_cmd(args, cli.json).await,
        },
    };

    ExitCode::from(code as u8)
}