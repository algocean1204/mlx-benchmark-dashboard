use std::path::PathBuf;

use aidash_core::bench;
use aidash_core::db::Database;
use aidash_core::profile::ModelProfile;
use aidash_core::pyproc::{pick_free_port, ChildSpec};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn fake_server_path() -> PathBuf {
    fixture_root()
        .join("tests")
        .join("fixtures")
        .join("fake_server.py")
}

fn test_profile() -> ModelProfile {
    ModelProfile {
        schema_version: 1,
        id: "test/fake".into(),
        display_name: "Fake".into(),
        source: aidash_core::profile::ProfileSource {
            kind: "hf".into(),
            hf_repo: "test/model".into(),
            hf_file: String::new(),
            local_path: String::new(),
        },
        model_type: "llm".into(),
        backend: "vllm_mlx".into(),
        io: aidash_core::profile::ProfileIo {
            input: vec!["chat".into()],
            output: "text".into(),
        },
        context: aidash_core::profile::ProfileContext {
            min: 512,
            max: 8192,
            default: 1024,
            sweep_steps: vec![1024, 2048],
        },
        default_params: serde_json::json!({"max_tokens": 32}),
        quantization: Some("4bit".into()),
        load_timeout_sec: 30,
        notes: String::new(),
        draft_model: None,
        generation_kind: aidash_core::profile::GENERATION_KIND_AUTOREGRESSIVE.into(),
    }
}

fn fake_child_spec(port: u16, alloc_on_ready_mb: u32) -> ChildSpec {
    ChildSpec {
        program: "python3".into(),
        args: vec![
            fake_server_path().display().to_string(),
            "--port".into(),
            port.to_string(),
            "--load-delay-sec".into(),
            "0.2".into(),
            "--alloc-at-start-mb".into(),
            alloc_on_ready_mb.to_string(),
        ],
        envs: vec![],
    }
}

fn temp_db_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "aidash_bench_test_{}_{name}.db",
        std::process::id()
    ))
}

#[tokio::test]
async fn bench_run_persists_db_rows() {
    let db_path = temp_db_path("run");
    let _ = std::fs::remove_file(&db_path);
    std::env::set_var("AIDASH_DB", db_path.to_string_lossy().to_string());

    let db = Database::open(Some(&db_path)).expect("open db");
    let profile = test_profile();
    let port = pick_free_port().expect("port");

    let result = bench::run_bench_single(
        &db,
        profile,
        1024,
        None,
        None,
        None,
        None,
        fixture_root().join("python"),
        Some(fake_child_spec(port, 0)),
        Some(port),
        None,
    )
    .await
    .expect("bench run");

    assert_eq!(result.status, "completed");
    assert!(result.stats.is_some());

    let export = db.export_run(result.run_id).expect("export");
    assert_eq!(export.run.status, "completed");
    assert!(export.results.is_some());
    assert!(!export.samples.is_empty());

    let _ = std::fs::remove_file(db_path);
}

#[tokio::test]
async fn watchdog_run_records_aborted_watchdog_with_samples() {
    let db_path = temp_db_path("watchdog");
    let _ = std::fs::remove_file(&db_path);
    std::env::set_var("AIDASH_DB", db_path.to_string_lossy().to_string());

    let db = Database::open(Some(&db_path)).expect("open db");
    let port = pick_free_port().expect("port");

    let result = bench::run_bench_single(
        &db,
        test_profile(),
        1024,
        None,
        None,
        None,
        Some(0.05),
        fixture_root().join("python"),
        Some(fake_child_spec(port, 80)),
        Some(port),
        None,
    )
    .await
    .expect("watchdog bench");

    assert_eq!(result.status, "aborted_watchdog");
    let export = db.export_run(result.run_id).expect("export");
    assert_eq!(export.run.status, "aborted_watchdog");
    assert!(!export.samples.is_empty());

    let _ = std::fs::remove_file(db_path);
}

#[tokio::test]
async fn monitor_samples_during_long_busy_phase() {
    let db_path = temp_db_path("monitor_busy");
    let _ = std::fs::remove_file(&db_path);
    std::env::set_var("AIDASH_DB", db_path.to_string_lossy().to_string());

    let db = Database::open(Some(&db_path)).expect("open db");
    let port = pick_free_port().expect("port");

    let mut spec = fake_child_spec(port, 0);
    spec.args.push("--chat-delay-sec".into());
    spec.args.push("1.5".into());

    let result = bench::run_bench_single(
        &db,
        test_profile(),
        1024,
        None,
        None,
        None,
        None,
        fixture_root().join("python"),
        Some(spec),
        Some(port),
        None,
    )
    .await
    .expect("bench run");

    assert_eq!(result.status, "completed");
    let export = db.export_run(result.run_id).expect("export");
    let samples = &export.samples;
    assert!(
        samples.len() >= 3,
        "expected samples during 1.5s+ run, got {}",
        samples.len()
    );

    let max_ts = samples.iter().map(|s| s.ts_ms).max().unwrap_or(0);
    let threshold = max_ts.saturating_sub(500);
    let late_samples: Vec<_> = samples
        .iter()
        .filter(|s| s.ts_ms >= threshold)
        .collect();
    assert!(
        !late_samples.is_empty(),
        "expected samples in last 500ms window (max_ts={max_ts})"
    );

    let _ = std::fs::remove_file(db_path);
}