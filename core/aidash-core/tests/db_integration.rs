use aidash_core::client::StreamStats;
use aidash_core::db::{Database, SampleRow};
use aidash_core::profile::ModelProfile;

fn fixture_profile(id: &str) -> ModelProfile {
    ModelProfile {
        schema_version: 1,
        id: id.into(),
        display_name: "Test".into(),
        source: aidash_core::profile::ProfileSource {
            kind: "hf".into(),
            hf_repo: id.into(),
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
            sweep_steps: vec![],
        },
        default_params: serde_json::json!({}),
        quantization: None,
        load_timeout_sec: 60,
        notes: String::new(),
    }
}

fn fixture_stats(decode_tps: f64) -> StreamStats {
    StreamStats {
        ttft_ms: 100.0,
        prefill_tps: 500.0,
        decode_tps: Some(decode_tps),
        total_tps: decode_tps,
        tokens_in: 128,
        tokens_out: 64,
    }
}

fn legacy_schema_db(path: &std::path::Path) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE models (
          id INTEGER PRIMARY KEY,
          profile_id TEXT NOT NULL UNIQUE,
          display_name TEXT NOT NULL,
          model_type TEXT NOT NULL,
          backend TEXT NOT NULL,
          quantization TEXT,
          profile_json TEXT NOT NULL,
          created_at TEXT NOT NULL
        );
        CREATE TABLE sweeps (
          id INTEGER PRIMARY KEY,
          kind TEXT NOT NULL,
          config_json TEXT NOT NULL,
          created_at TEXT NOT NULL
        );
        CREATE TABLE runs (
          id INTEGER PRIMARY KEY,
          model_id INTEGER NOT NULL REFERENCES models(id),
          kind TEXT NOT NULL,
          sweep_id INTEGER REFERENCES sweeps(id),
          context_size INTEGER,
          params_json TEXT NOT NULL,
          status TEXT NOT NULL,
          started_at TEXT NOT NULL,
          ended_at TEXT
        );
        CREATE TABLE results (
          run_id INTEGER PRIMARY KEY REFERENCES runs(id),
          ttft_ms REAL, prefill_tps REAL, decode_tps REAL, total_tps REAL,
          tokens_in INTEGER, tokens_out INTEGER,
          peak_phys_footprint_bytes INTEGER, peak_mlx_active_bytes INTEGER,
          avg_cpu_pct REAL, quality_score REAL
        );
        CREATE TABLE samples (
          id INTEGER PRIMARY KEY,
          run_id INTEGER NOT NULL REFERENCES runs(id),
          ts_ms INTEGER NOT NULL,
          phys_footprint_bytes INTEGER NOT NULL,
          mlx_active_bytes INTEGER,
          cpu_pct REAL,
          sys_available_bytes INTEGER,
          power_w REAL, temp_c REAL, throttled INTEGER
        );
        "#,
    )
    .unwrap();
}

#[test]
fn migration_sets_user_version_and_cascade_delete_run() {
    let dir = std::env::temp_dir().join(format!("aidash_migrate_{}", std::process::id()));
    let path = dir.join("legacy.db");
    legacy_schema_db(&path);

    let profile = fixture_profile("test/model");
    let profile_json = serde_json::to_string(&profile).unwrap();
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO models (profile_id, display_name, model_type, backend, profile_json, created_at)
             VALUES ('test/model', 'Test', 'llm', 'vllm_mlx', ?1, '1')",
            rusqlite::params![profile_json],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO runs (model_id, kind, params_json, status, started_at, context_size)
             VALUES (1, 'single', '{}', 'completed', '1', 1024)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO results (run_id, decode_tps, ttft_ms, tokens_in, tokens_out)
             VALUES (1, 50.0, 100.0, 10, 20)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO samples (run_id, ts_ms, phys_footprint_bytes, cpu_pct, sys_available_bytes)
             VALUES (1, 0, 1000, 1.5, 10000)",
            [],
        )
        .unwrap();
    }

    let db = Database::open(Some(&path)).expect("open migrates");
    assert_eq!(db.user_version().unwrap(), 2);

    db.delete_run(1).expect("delete run");
    assert_eq!(db.count_samples_for_run(1).unwrap(), 0);
    assert_eq!(db.count_results_for_run(1).unwrap(), 0);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn delete_model_removes_all_runs() {
    let dir = std::env::temp_dir().join(format!("aidash_del_model_{}", std::process::id()));
    let path = dir.join("test.db");
    let db = Database::open(Some(&path)).expect("open");

    let profile = fixture_profile("org/delete-me");
    let model_id = db.upsert_model(&profile).expect("upsert");
    for ctx in [1024i32, 2048] {
        let run_id = db
            .insert_run(model_id, "single", None, Some(ctx as u32), "{}")
            .expect("insert");
        db.finish_run(run_id, "completed", None).expect("finish");
        db.insert_results(run_id, &fixture_stats(45.0), 1000, 2000, 5.0)
            .expect("results");
        db.insert_samples(
            run_id,
            &[SampleRow {
                ts_ms: 0,
                phys_footprint_bytes: 1000,
                mlx_active_bytes: None,
                cpu_pct: 2.0,
                sys_available_bytes: 10000,
                power_w: None,
                temp_c: None,
                throttled: None,
            }],
        )
        .expect("samples");
    }

    let summary = db.delete_model_summary("org/delete-me").expect("summary");
    assert_eq!(summary.runs, 2);
    assert_eq!(summary.samples, 2);
    assert_eq!(summary.results, 2);

    db.delete_model("org/delete-me").expect("delete");
    let rows = db.list_runs(None).expect("list");
    assert!(rows.is_empty());

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn delete_run_cascades_samples_and_results() {
    let dir = std::env::temp_dir().join(format!("aidash_del_run_{}", std::process::id()));
    let path = dir.join("test.db");
    let db = Database::open(Some(&path)).expect("open");

    let profile = fixture_profile("org/cascade");
    let model_id = db.upsert_model(&profile).expect("upsert");
    let run_id = db
        .insert_run(model_id, "single", None, Some(1024), "{}")
        .expect("insert");
    db.finish_run(run_id, "failed", None).expect("finish");
    db.insert_results(run_id, &fixture_stats(5.0), 1000, 0, 0.0)
        .expect("results");
    db.insert_samples(
        run_id,
        &[SampleRow {
            ts_ms: 0,
            phys_footprint_bytes: 500,
            mlx_active_bytes: None,
            cpu_pct: 0.0,
            sys_available_bytes: 8000,
            power_w: None,
            temp_c: None,
            throttled: None,
        }],
    )
    .expect("samples");

    db.delete_run(run_id).expect("delete");
    assert_eq!(db.count_samples_for_run(run_id).unwrap(), 0);
    assert_eq!(db.count_results_for_run(run_id).unwrap(), 0);

    let _ = std::fs::remove_dir_all(dir);
}