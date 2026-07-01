//! rusqlite. 스키마 생성·마이그레이션·기록·조회·보내기
//!
//! IN: 결과·샘플·런 메타
//! OUT: 쿼리 결과, JSON/CSV export

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::client::StreamStats;
use crate::monitor::ResourceSample;
use crate::profile::ModelProfile;

const SCHEMA_VERSION: i32 = 4;

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS models (
  id INTEGER PRIMARY KEY,
  profile_id TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  model_type TEXT NOT NULL,
  backend TEXT NOT NULL,
  quantization TEXT,
  profile_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sweeps (
  id INTEGER PRIMARY KEY,
  kind TEXT NOT NULL,
  config_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS runs (
  id INTEGER PRIMARY KEY,
  model_id INTEGER NOT NULL REFERENCES models(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  sweep_id INTEGER REFERENCES sweeps(id) ON DELETE SET NULL,
  context_size INTEGER,
  params_json TEXT NOT NULL,
  status TEXT NOT NULL,
  started_at TEXT NOT NULL,
  ended_at TEXT
);

CREATE TABLE IF NOT EXISTS results (
  run_id INTEGER PRIMARY KEY REFERENCES runs(id) ON DELETE CASCADE,
  ttft_ms REAL,
  prefill_tps REAL,
  decode_tps REAL,
  total_tps REAL,
  tokens_in INTEGER,
  tokens_out INTEGER,
  peak_phys_footprint_bytes INTEGER,
  peak_mlx_active_bytes INTEGER,
  avg_cpu_pct REAL,
  quality_score REAL
);

CREATE TABLE IF NOT EXISTS samples (
  id INTEGER PRIMARY KEY,
  run_id INTEGER NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
  ts_ms INTEGER NOT NULL,
  phys_footprint_bytes INTEGER NOT NULL,
  mlx_active_bytes INTEGER,
  cpu_pct REAL,
  sys_available_bytes INTEGER,
  power_w REAL,
  temp_c REAL,
  throttled INTEGER
);

CREATE INDEX IF NOT EXISTS idx_samples_run ON samples(run_id, ts_ms);
"#;

const MIGRATION_V1_SQL: &str = r#"
CREATE TABLE models_new (
  id INTEGER PRIMARY KEY,
  profile_id TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  model_type TEXT NOT NULL,
  backend TEXT NOT NULL,
  quantization TEXT,
  profile_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE sweeps_new (
  id INTEGER PRIMARY KEY,
  kind TEXT NOT NULL,
  config_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE runs_new (
  id INTEGER PRIMARY KEY,
  model_id INTEGER NOT NULL REFERENCES models_new(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  sweep_id INTEGER REFERENCES sweeps_new(id) ON DELETE SET NULL,
  context_size INTEGER,
  params_json TEXT NOT NULL,
  status TEXT NOT NULL,
  started_at TEXT NOT NULL,
  ended_at TEXT
);

CREATE TABLE results_new (
  run_id INTEGER PRIMARY KEY REFERENCES runs_new(id) ON DELETE CASCADE,
  ttft_ms REAL,
  prefill_tps REAL,
  decode_tps REAL,
  total_tps REAL,
  tokens_in INTEGER,
  tokens_out INTEGER,
  peak_phys_footprint_bytes INTEGER,
  peak_mlx_active_bytes INTEGER,
  avg_cpu_pct REAL,
  quality_score REAL
);

CREATE TABLE samples_new (
  id INTEGER PRIMARY KEY,
  run_id INTEGER NOT NULL REFERENCES runs_new(id) ON DELETE CASCADE,
  ts_ms INTEGER NOT NULL,
  phys_footprint_bytes INTEGER NOT NULL,
  mlx_active_bytes INTEGER,
  cpu_pct REAL,
  sys_available_bytes INTEGER,
  power_w REAL,
  temp_c REAL,
  throttled INTEGER
);

INSERT INTO models_new SELECT * FROM models;
INSERT INTO sweeps_new SELECT * FROM sweeps;
INSERT INTO results_new SELECT * FROM results;
INSERT INTO samples_new SELECT * FROM samples;
INSERT INTO runs_new SELECT * FROM runs;

DROP TABLE samples;
DROP TABLE results;
DROP TABLE runs;
DROP TABLE sweeps;
DROP TABLE models;

ALTER TABLE models_new RENAME TO models;
ALTER TABLE sweeps_new RENAME TO sweeps;
ALTER TABLE runs_new RENAME TO runs;
ALTER TABLE results_new RENAME TO results;
ALTER TABLE samples_new RENAME TO samples;

CREATE INDEX IF NOT EXISTS idx_samples_run ON samples(run_id, ts_ms);
"#;

const MIGRATION_V2_SQL: &str = r#"
ALTER TABLE runs ADD COLUMN error_message TEXT;
"#;

const MIGRATION_V3_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS chat_sessions (
  id INTEGER PRIMARY KEY,
  profile_id TEXT NOT NULL,
  title TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS chat_messages (
  id INTEGER PRIMARY KEY,
  session_id INTEGER NOT NULL REFERENCES chat_sessions(id) ON DELETE CASCADE,
  role TEXT NOT NULL,
  content TEXT NOT NULL,
  created_at TEXT NOT NULL,
  token_count INTEGER
);

CREATE INDEX IF NOT EXISTS idx_chat_messages_session ON chat_messages(session_id, id);
CREATE INDEX IF NOT EXISTS idx_chat_sessions_updated ON chat_sessions(updated_at DESC);
"#;

const MIGRATION_V4_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS eval_template_results (
  id INTEGER PRIMARY KEY,
  profile_id TEXT NOT NULL,
  context_size INTEGER NOT NULL,
  template_id TEXT NOT NULL,
  score INTEGER NOT NULL,
  output_excerpt TEXT NOT NULL,
  elapsed_ms INTEGER NOT NULL,
  created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_eval_template_profile
  ON eval_template_results(profile_id, context_size, created_at DESC);
"#;

#[derive(Debug)]
pub struct Database {
    pub(crate) conn: Mutex<Connection>,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunListRow {
    pub run_id: i64,
    pub profile_id: String,
    pub display_name: String,
    pub generation_kind: String,
    pub kind: String,
    pub context_size: Option<i64>,
    pub status: String,
    pub decode_tps: Option<f64>,
    pub peak_phys_footprint_bytes: Option<i64>,
    pub ended_at: Option<String>,
    pub use_draft: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunExport {
    pub run: RunExportMeta,
    pub results: Option<RunExportResults>,
    pub samples: Vec<RunExportSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunExportMeta {
    pub id: i64,
    pub profile_id: String,
    pub display_name: String,
    pub kind: String,
    pub sweep_id: Option<i64>,
    pub context_size: Option<i64>,
    pub params_json: String,
    pub status: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunExportResults {
    pub ttft_ms: Option<f64>,
    pub prefill_tps: Option<f64>,
    pub decode_tps: Option<f64>,
    pub total_tps: Option<f64>,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
    pub peak_phys_footprint_bytes: Option<i64>,
    pub peak_mlx_active_bytes: Option<i64>,
    pub avg_cpu_pct: Option<f64>,
    pub quality_score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunExportSample {
    pub ts_ms: i64,
    pub phys_footprint_bytes: i64,
    pub mlx_active_bytes: Option<i64>,
    pub cpu_pct: Option<f64>,
    pub sys_available_bytes: Option<i64>,
    pub power_w: Option<f64>,
    pub temp_c: Option<f64>,
    pub throttled: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareRow {
    pub profile_id: String,
    pub display_name: String,
    pub model_type: String,
    pub generation_kind: String,
    pub context_requested: i64,
    pub context_actual: i64,
    pub context_substituted: bool,
    pub decode_tps: Option<f64>,
    pub ttft_ms: Option<f64>,
    pub peak_phys_footprint_bytes: Option<i64>,
    pub peak_mlx_active_bytes: Option<i64>,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
    pub measured_at: Option<String>,
    pub hf_url: Option<String>,
    pub use_draft: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionRow {
    pub id: i64,
    pub profile_id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalTemplateResultRow {
    pub id: i64,
    pub profile_id: String,
    pub context_size: i64,
    pub template_id: String,
    pub score: i64,
    pub output_excerpt: String,
    pub elapsed_ms: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageRow {
    pub id: i64,
    pub session_id: i64,
    pub role: String,
    pub content: String,
    pub created_at: String,
    pub token_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteSummary {
    pub runs: i64,
    pub samples: i64,
    pub results: i64,
}

#[derive(Debug)]
pub enum DbError {
    Sql(rusqlite::Error),
    Io(std::io::Error),
    NotFound(i64),
    Json(serde_json::Error),
    Validation(String),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Sql(e) => write!(f, "sqlite error: {e}"),
            DbError::Io(e) => write!(f, "io error: {e}"),
            DbError::NotFound(id) => write!(f, "run {id} not found"),
            DbError::Json(e) => write!(f, "json error: {e}"),
            DbError::Validation(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(value: rusqlite::Error) -> Self {
        DbError::Sql(value)
    }
}

impl From<std::io::Error> for DbError {
    fn from(value: std::io::Error) -> Self {
        DbError::Io(value)
    }
}

impl From<serde_json::Error> for DbError {
    fn from(value: serde_json::Error) -> Self {
        DbError::Json(value)
    }
}

pub fn default_db_path() -> PathBuf {
    if let Ok(path) = std::env::var("AIDASH_DB") {
        return PathBuf::from(path);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("AI_Dashboard")
        .join("aidash.db")
}

pub fn iso_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().to_string())
        .unwrap_or_else(|_| "0".into())
}

pub struct SampleRecorder {
    run_start: Instant,
    last_recorded_ms: u64,
    samples: Vec<SampleRow>,
    peak_phys: u64,
    peak_mlx: u64,
    cpu_sum: f64,
    cpu_count: u64,
}

#[derive(Debug, Clone)]
pub struct SampleRow {
    pub ts_ms: i64,
    pub phys_footprint_bytes: i64,
    pub mlx_active_bytes: Option<i64>,
    pub cpu_pct: f64,
    pub sys_available_bytes: i64,
    pub power_w: Option<f64>,
    pub temp_c: Option<f64>,
    pub throttled: Option<bool>,
}

impl SampleRecorder {
    pub fn new() -> Self {
        Self {
            run_start: Instant::now(),
            last_recorded_ms: 0,
            samples: Vec::new(),
            peak_phys: 0,
            peak_mlx: 0,
            cpu_sum: 0.0,
            cpu_count: 0,
        }
    }

    pub fn on_sample(&mut self, sample: &ResourceSample) {
        self.peak_phys = self.peak_phys.max(sample.phys_footprint_bytes);
        if let Some(mlx) = sample.mlx_active_bytes {
            self.peak_mlx = self.peak_mlx.max(mlx);
        }
        self.cpu_sum += sample.cpu_pct;
        self.cpu_count += 1;

        let elapsed_ms = self.run_start.elapsed().as_millis() as u64;
        let first_sample = self.samples.is_empty();
        if !first_sample && elapsed_ms.saturating_sub(self.last_recorded_ms) < 500 {
            return;
        }
        self.last_recorded_ms = elapsed_ms;

        self.samples.push(SampleRow {
            ts_ms: elapsed_ms as i64,
            phys_footprint_bytes: sample.phys_footprint_bytes as i64,
            mlx_active_bytes: sample.mlx_active_bytes.map(|v| v as i64),
            cpu_pct: sample.cpu_pct,
            sys_available_bytes: sample.sys_available_bytes as i64,
            power_w: sample.power_w,
            temp_c: sample.temp_c,
            throttled: sample.throttled,
        });
    }

    pub fn peak_phys_footprint_bytes(&self) -> u64 {
        self.peak_phys
    }

    pub fn peak_mlx_active_bytes(&self) -> u64 {
        self.peak_mlx
    }

    pub fn avg_cpu_pct(&self) -> f64 {
        if self.cpu_count == 0 {
            0.0
        } else {
            self.cpu_sum / self.cpu_count as f64
        }
    }

    pub fn samples(&self) -> &[SampleRow] {
        &self.samples
    }
}

impl Database {
    pub fn open(path: Option<&Path>) -> Result<Self, DbError> {
        let path = path
            .map(PathBuf::from)
            .unwrap_or_else(default_db_path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        migrate_schema(&conn)?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        Ok(Self {
            conn: Mutex::new(conn),
            path,
        })
    }

    pub fn upsert_model(&self, profile: &ModelProfile) -> Result<i64, DbError> {
        let profile_json = serde_json::to_string(profile)?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO models (profile_id, display_name, model_type, backend, quantization, profile_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(profile_id) DO UPDATE SET
               display_name=excluded.display_name,
               model_type=excluded.model_type,
               backend=excluded.backend,
               quantization=excluded.quantization,
               profile_json=excluded.profile_json",
            params![
                profile.id,
                profile.display_name,
                profile.model_type,
                profile.backend,
                profile.quantization,
                profile_json,
                iso_timestamp(),
            ],
        )?;
        let id: i64 = conn.query_row(
            "SELECT id FROM models WHERE profile_id = ?1",
            params![profile.id],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    pub fn insert_sweep(&self, kind: &str, config_json: &str) -> Result<i64, DbError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sweeps (kind, config_json, created_at) VALUES (?1, ?2, ?3)",
            params![kind, config_json, iso_timestamp()],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn insert_run(
        &self,
        model_id: i64,
        kind: &str,
        sweep_id: Option<i64>,
        context_size: Option<u32>,
        params_json: &str,
    ) -> Result<i64, DbError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO runs (model_id, kind, sweep_id, context_size, params_json, status, started_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'running', ?6)",
            params![
                model_id,
                kind,
                sweep_id,
                context_size.map(|v| v as i64),
                params_json,
                iso_timestamp(),
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn finish_run(
        &self,
        run_id: i64,
        status: &str,
        error_message: Option<&str>,
    ) -> Result<(), DbError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE runs SET status = ?1, ended_at = ?2, error_message = ?3 WHERE id = ?4",
            params![status, iso_timestamp(), error_message, run_id],
        )?;
        Ok(())
    }

    pub fn insert_results(
        &self,
        run_id: i64,
        stats: &StreamStats,
        peak_phys: u64,
        peak_mlx: u64,
        avg_cpu: f64,
    ) -> Result<(), DbError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO results (
                run_id, ttft_ms, prefill_tps, decode_tps, total_tps,
                tokens_in, tokens_out, peak_phys_footprint_bytes, peak_mlx_active_bytes, avg_cpu_pct
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                run_id,
                stats.ttft_ms,
                stats.prefill_tps,
                stats.decode_tps,
                // decode_tps may be NULL when unmeasurable
                stats.total_tps,
                stats.tokens_in as i64,
                stats.tokens_out as i64,
                peak_phys as i64,
                peak_mlx as i64,
                avg_cpu,
            ],
        )?;
        Ok(())
    }

    pub fn insert_quality_result(&self, run_id: i64, quality_score: f64) -> Result<(), DbError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO results (run_id, quality_score) VALUES (?1, ?2)",
            params![run_id, quality_score],
        )?;
        Ok(())
    }

    pub fn insert_timing_results(
        &self,
        run_id: i64,
        ttft_ms: f64,
        peak_phys: u64,
        peak_mlx: u64,
        avg_cpu: f64,
    ) -> Result<(), DbError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO results (
                run_id, ttft_ms, prefill_tps, decode_tps, total_tps,
                tokens_in, tokens_out, peak_phys_footprint_bytes, peak_mlx_active_bytes, avg_cpu_pct
             ) VALUES (?1, ?2, NULL, NULL, NULL, NULL, NULL, ?3, ?4, ?5)",
            params![run_id, ttft_ms, peak_phys as i64, peak_mlx as i64, avg_cpu],
        )?;
        Ok(())
    }

    pub fn insert_samples(&self, run_id: i64, samples: &[SampleRow]) -> Result<(), DbError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "INSERT INTO samples (
                run_id, ts_ms, phys_footprint_bytes, mlx_active_bytes, cpu_pct,
                sys_available_bytes, power_w, temp_c, throttled
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        for s in samples {
            stmt.execute(params![
                run_id,
                s.ts_ms,
                s.phys_footprint_bytes,
                s.mlx_active_bytes,
                s.cpu_pct,
                s.sys_available_bytes,
                s.power_w,
                s.temp_c,
                s.throttled.map(|v| if v { 1i64 } else { 0i64 }),
            ])?;
        }
        Ok(())
    }

    /// 완료된 런에 존재하는 컨텍스트 크기의 합집합(오름차순).
    pub fn measured_contexts(&self) -> Result<Vec<i64>, DbError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT r.context_size
             FROM runs r
             WHERE r.status = 'completed' AND r.context_size IS NOT NULL
             ORDER BY r.context_size ASC",
        )?;
        let mapped = stmt.query_map([], |row| row.get(0))?;
        let mut out = Vec::new();
        for row in mapped {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_runs(&self, model_profile_id: Option<&str>) -> Result<Vec<RunListRow>, DbError> {
        let conn = self.conn.lock().unwrap();
        let sql = if model_profile_id.is_some() {
            "SELECT r.id, m.profile_id, m.display_name, m.profile_json, r.kind, r.context_size,
                    r.status, res.decode_tps, res.total_tps, res.peak_phys_footprint_bytes,
                    r.ended_at, r.params_json
             FROM runs r
             JOIN models m ON r.model_id = m.id
             LEFT JOIN results res ON res.run_id = r.id
             WHERE m.profile_id = ?1
             ORDER BY r.id DESC"
        } else {
            "SELECT r.id, m.profile_id, m.display_name, m.profile_json, r.kind, r.context_size,
                    r.status, res.decode_tps, res.total_tps, res.peak_phys_footprint_bytes,
                    r.ended_at, r.params_json
             FROM runs r
             JOIN models m ON r.model_id = m.id
             LEFT JOIN results res ON res.run_id = r.id
             ORDER BY r.id DESC"
        };

        let mut rows = Vec::new();
        if let Some(pid) = model_profile_id {
            let mut stmt = conn.prepare(sql)?;
            let mapped = stmt.query_map(params![pid], map_run_list_row)?;
            for row in mapped {
                rows.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(sql)?;
            let mapped = stmt.query_map([], map_run_list_row)?;
            for row in mapped {
                rows.push(row?);
            }
        }
        Ok(rows)
    }

    pub fn export_run(&self, run_id: i64) -> Result<RunExport, DbError> {
        let conn = self.conn.lock().unwrap();
        let meta = conn
            .query_row(
                "SELECT r.id, m.profile_id, m.display_name, r.kind, r.sweep_id, r.context_size,
                        r.params_json, r.status, r.started_at, r.ended_at, r.error_message
                 FROM runs r
                 JOIN models m ON r.model_id = m.id
                 WHERE r.id = ?1",
                params![run_id],
                |row| {
                    Ok(RunExportMeta {
                        id: row.get(0)?,
                        profile_id: row.get(1)?,
                        display_name: row.get(2)?,
                        kind: row.get(3)?,
                        sweep_id: row.get(4)?,
                        context_size: row.get(5)?,
                        params_json: row.get(6)?,
                        status: row.get(7)?,
                        started_at: row.get(8)?,
                        ended_at: row.get(9)?,
                        error_message: row.get(10)?,
                    })
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => DbError::NotFound(run_id),
                other => DbError::Sql(other),
            })?;

        let results = conn
            .query_row(
                "SELECT ttft_ms, prefill_tps, decode_tps, total_tps, tokens_in, tokens_out,
                        peak_phys_footprint_bytes, peak_mlx_active_bytes, avg_cpu_pct, quality_score
                 FROM results WHERE run_id = ?1",
                params![run_id],
                |row| {
                    Ok(RunExportResults {
                        ttft_ms: row.get(0)?,
                        prefill_tps: row.get(1)?,
                        decode_tps: row.get(2)?,
                        total_tps: row.get(3)?,
                        tokens_in: row.get(4)?,
                        tokens_out: row.get(5)?,
                        peak_phys_footprint_bytes: row.get(6)?,
                        peak_mlx_active_bytes: row.get(7)?,
                        avg_cpu_pct: row.get(8)?,
                        quality_score: row.get(9)?,
                    })
                },
            )
            .ok();

        let mut sample_stmt = conn.prepare(
            "SELECT ts_ms, phys_footprint_bytes, mlx_active_bytes, cpu_pct, sys_available_bytes,
                    power_w, temp_c, throttled
             FROM samples WHERE run_id = ?1 ORDER BY ts_ms",
        )?;
        let sample_rows = sample_stmt.query_map(params![run_id], |row| {
            Ok(RunExportSample {
                ts_ms: row.get(0)?,
                phys_footprint_bytes: row.get(1)?,
                mlx_active_bytes: row.get(2)?,
                cpu_pct: row.get(3)?,
                sys_available_bytes: row.get(4)?,
                power_w: row.get(5)?,
                temp_c: row.get(6)?,
                throttled: row.get(7)?,
            })
        })?;
        let mut samples = Vec::new();
        for s in sample_rows {
            samples.push(s?);
        }

        Ok(RunExport {
            run: meta,
            results,
            samples,
        })
    }

    pub fn export_run_json(&self, run_id: i64) -> Result<String, DbError> {
        let export = self.export_run(run_id)?;
        Ok(serde_json::to_string_pretty(&export)?)
    }

    pub fn compare_models(
        &self,
        profile_ids: &[String],
        target_context: i64,
    ) -> Result<Vec<CompareRow>, DbError> {
        use crate::profile::{
            generation_kind_from_profile_json, hf_url, model_type_from_profile_json,
            source_kind_from_profile_json,
        };
        use crate::stats::{pick_representative_run, DEFAULT_OVERVIEW_CONTEXT};
        use crate::tps_tier;

        let context = if target_context > 0 {
            target_context
        } else {
            DEFAULT_OVERVIEW_CONTEXT
        };

        let mut rows = Vec::new();
        for profile_id in profile_ids {
            let models = self.load_model_runs_for_compare(profile_id)?;
            let Some(model) = models else {
                return Err(DbError::Validation(format!("model not found: {profile_id}")));
            };

            let kind = source_kind_from_profile_json(&model.profile_json);
            let url = hf_url(profile_id, kind.as_deref());
            let model_type = model_type_from_profile_json(&model.profile_json)
                .unwrap_or_else(|| "llm".into());
            let generation_kind = generation_kind_from_profile_json(&model.profile_json);

            if let Some((run, pick)) = pick_representative_run(&model.runs, context) {
                rows.push(CompareRow {
                    profile_id: profile_id.clone(),
                    display_name: model.display_name,
                    model_type: model_type.clone(),
                    generation_kind: generation_kind.clone(),
                    context_requested: pick.requested,
                    context_actual: pick.actual,
                    context_substituted: pick.substituted,
                    decode_tps: tps_tier::effective_display_tps(
                        &generation_kind,
                        run.decode_tps,
                        run.total_tps,
                    ),
                    ttft_ms: Some(run.ttft_ms),
                    peak_phys_footprint_bytes: Some(run.peak_phys),
                    peak_mlx_active_bytes: Some(run.peak_mlx),
                    tokens_in: run.tokens_in,
                    tokens_out: run.tokens_out,
                    measured_at: run.ended_at.clone(),
                    hf_url: url,
                    use_draft: run.use_draft,
                });
            } else {
                rows.push(CompareRow {
                    profile_id: profile_id.clone(),
                    display_name: model.display_name,
                    model_type: model_type.clone(),
                    generation_kind: generation_kind.clone(),
                    context_requested: context,
                    context_actual: context,
                    context_substituted: false,
                    decode_tps: None,
                    ttft_ms: None,
                    peak_phys_footprint_bytes: None,
                    peak_mlx_active_bytes: None,
                    tokens_in: None,
                    tokens_out: None,
                    measured_at: None,
                    hf_url: url,
                    use_draft: None,
                });
            }
        }
        Ok(rows)
    }

    pub fn delete_run_summary(&self, run_id: i64) -> Result<DeleteSummary, DbError> {
        let conn = self.conn.lock().unwrap();
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM runs WHERE id = ?1",
                params![run_id],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if !exists {
            return Err(DbError::NotFound(run_id));
        }
        let samples: i64 = conn.query_row(
            "SELECT COUNT(*) FROM samples WHERE run_id = ?1",
            params![run_id],
            |row| row.get(0),
        )?;
        let results: i64 = conn.query_row(
            "SELECT COUNT(*) FROM results WHERE run_id = ?1",
            params![run_id],
            |row| row.get(0),
        )?;
        Ok(DeleteSummary {
            runs: 1,
            samples,
            results,
        })
    }

    pub fn delete_model_summary(&self, profile_id: &str) -> Result<DeleteSummary, DbError> {
        let conn = self.conn.lock().unwrap();
        let model_id: i64 = conn
            .query_row(
                "SELECT id FROM models WHERE profile_id = ?1",
                params![profile_id],
                |row| row.get(0),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    DbError::Validation(format!("model not found: {profile_id}"))
                }
                other => DbError::Sql(other),
            })?;
        let runs: i64 = conn.query_row(
            "SELECT COUNT(*) FROM runs WHERE model_id = ?1",
            params![model_id],
            |row| row.get(0),
        )?;
        let samples: i64 = conn.query_row(
            "SELECT COUNT(*) FROM samples s JOIN runs r ON s.run_id = r.id WHERE r.model_id = ?1",
            params![model_id],
            |row| row.get(0),
        )?;
        let results: i64 = conn.query_row(
            "SELECT COUNT(*) FROM results res JOIN runs r ON res.run_id = r.id WHERE r.model_id = ?1",
            params![model_id],
            |row| row.get(0),
        )?;
        Ok(DeleteSummary {
            runs,
            samples,
            results,
        })
    }

    pub fn delete_run(&self, run_id: i64) -> Result<(), DbError> {
        self.delete_run_summary(run_id)?;
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM runs WHERE id = ?1", params![run_id])?;
        Ok(())
    }

    pub fn delete_model(&self, profile_id: &str) -> Result<(), DbError> {
        self.delete_model_summary(profile_id)?;
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM models WHERE profile_id = ?1", params![profile_id])?;
        Ok(())
    }

    pub fn count_samples_for_run(&self, run_id: i64) -> Result<i64, DbError> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM samples WHERE run_id = ?1",
            params![run_id],
            |row| row.get(0),
        )?)
    }

    pub fn user_version(&self) -> Result<i32, DbError> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?)
    }

    pub fn count_results_for_run(&self, run_id: i64) -> Result<i64, DbError> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM results WHERE run_id = ?1",
            params![run_id],
            |row| row.get(0),
        )?)
    }

    pub fn create_chat_session(&self, profile_id: &str, title: &str) -> Result<i64, DbError> {
        let now = iso_timestamp();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO chat_sessions (profile_id, title, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![profile_id, title, now, now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn list_chat_sessions(&self) -> Result<Vec<ChatSessionRow>, DbError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, profile_id, title, created_at, updated_at
             FROM chat_sessions ORDER BY updated_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ChatSessionRow {
                id: row.get(0)?,
                profile_id: row.get(1)?,
                title: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn delete_chat_session(&self, session_id: i64) -> Result<(), DbError> {
        let conn = self.conn.lock().unwrap();
        let deleted = conn.execute(
            "DELETE FROM chat_sessions WHERE id = ?1",
            params![session_id],
        )?;
        if deleted == 0 {
            return Err(DbError::NotFound(session_id));
        }
        Ok(())
    }

    pub fn touch_chat_session(&self, session_id: i64) -> Result<(), DbError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE chat_sessions SET updated_at = ?1 WHERE id = ?2",
            params![iso_timestamp(), session_id],
        )?;
        Ok(())
    }

    pub fn update_chat_session_title(&self, session_id: i64, title: &str) -> Result<(), DbError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE chat_sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, iso_timestamp(), session_id],
        )?;
        Ok(())
    }

    pub fn load_chat_messages(&self, session_id: i64) -> Result<Vec<ChatMessageRow>, DbError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, created_at, token_count
             FROM chat_messages WHERE session_id = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![session_id], |row| {
            Ok(ChatMessageRow {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
                token_count: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn append_chat_message(
        &self,
        session_id: i64,
        role: &str,
        content: &str,
        token_count: Option<u32>,
    ) -> Result<i64, DbError> {
        let now = iso_timestamp();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO chat_messages (session_id, role, content, created_at, token_count)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                session_id,
                role,
                content,
                now,
                token_count.map(|v| v as i64),
            ],
        )?;
        let id = conn.last_insert_rowid();
        conn.execute(
            "UPDATE chat_sessions SET updated_at = ?1 WHERE id = ?2",
            params![now, session_id],
        )?;
        Ok(id)
    }

    pub fn insert_eval_template_result(
        &self,
        profile_id: &str,
        context_size: u32,
        template_id: &str,
        score: u32,
        output_excerpt: &str,
        elapsed_ms: u64,
    ) -> Result<i64, DbError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO eval_template_results
             (profile_id, context_size, template_id, score, output_excerpt, elapsed_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                profile_id,
                context_size as i64,
                template_id,
                score as i64,
                output_excerpt,
                elapsed_ms as i64,
                iso_timestamp(),
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn list_eval_template_results(
        &self,
        profile_id: &str,
        context_size: Option<u32>,
    ) -> Result<Vec<EvalTemplateResultRow>, DbError> {
        let conn = self.conn.lock().unwrap();
        let mut rows = Vec::new();
        if let Some(ctx) = context_size {
            let mut stmt = conn.prepare(
                "SELECT id, profile_id, context_size, template_id, score,
                        output_excerpt, elapsed_ms, created_at
                 FROM eval_template_results
                 WHERE profile_id = ?1 AND context_size = ?2
                 ORDER BY created_at DESC, id DESC",
            )?;
            let mapped = stmt.query_map(params![profile_id, ctx as i64], map_eval_template_row)?;
            for row in mapped {
                rows.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, profile_id, context_size, template_id, score,
                        output_excerpt, elapsed_ms, created_at
                 FROM eval_template_results
                 WHERE profile_id = ?1
                 ORDER BY created_at DESC, id DESC",
            )?;
            let mapped = stmt.query_map(params![profile_id], map_eval_template_row)?;
            for row in mapped {
                rows.push(row?);
            }
        }
        Ok(rows)
    }

    pub fn export_run_csv(&self, run_id: i64) -> Result<String, DbError> {
        let export = self.export_run(run_id)?;
        let mut out = String::new();
        out.push_str("section,key,value\n");
        out.push_str(&format!("run,id,{}\n", export.run.id));
        out.push_str(&format!("run,profile_id,{}\n", export.run.profile_id));
        out.push_str(&format!("run,kind,{}\n", export.run.kind));
        out.push_str(&format!(
            "run,context_size,{}\n",
            export.run.context_size.unwrap_or(0)
        ));
        out.push_str(&format!("run,status,{}\n", export.run.status));
        if let Some(res) = &export.results {
            out.push_str(&format!("results,ttft_ms,{}\n", res.ttft_ms.unwrap_or(0.0)));
            out.push_str(&format!(
                "results,decode_tps,{}\n",
                res.decode_tps.unwrap_or(0.0)
            ));
            out.push_str(&format!(
                "results,tokens_in,{}\n",
                res.tokens_in.unwrap_or(0)
            ));
            out.push_str(&format!(
                "results,tokens_out,{}\n",
                res.tokens_out.unwrap_or(0)
            ));
            out.push_str(&format!(
                "results,peak_phys_footprint_bytes,{}\n",
                res.peak_phys_footprint_bytes.unwrap_or(0)
            ));
            out.push_str(&format!(
                "results,peak_mlx_active_bytes,{}\n",
                res.peak_mlx_active_bytes.unwrap_or(0)
            ));
        }
        out.push('\n');
        out.push_str(
            "ts_ms,phys_footprint_bytes,mlx_active_bytes,cpu_pct,sys_available_bytes\n",
        );
        for s in &export.samples {
            out.push_str(&format!(
                "{},{},{},{},{}\n",
                s.ts_ms,
                s.phys_footprint_bytes,
                s.mlx_active_bytes.unwrap_or(0),
                s.cpu_pct.unwrap_or(0.0),
                s.sys_available_bytes.unwrap_or(0),
            ));
        }
        Ok(out)
    }
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool, DbError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        params![name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, DbError> {
    let sql = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for name in rows.flatten() {
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn migrate_schema(conn: &Connection) -> Result<(), DbError> {
    let mut version: i32 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    if version < 1 {
        if table_exists(conn, "models")? {
            conn.execute_batch("PRAGMA foreign_keys=OFF;")?;
            conn.execute_batch(MIGRATION_V1_SQL)?;
        } else {
            conn.execute_batch(SCHEMA_SQL)?;
        }
        conn.execute_batch("PRAGMA user_version=1;")?;
        version = 1;
    }

    if version < 2 {
        if table_exists(conn, "runs")? && !column_exists(conn, "runs", "error_message")? {
            conn.execute_batch(MIGRATION_V2_SQL)?;
        }
        conn.execute_batch("PRAGMA user_version=2;")?;
        version = 2;
    }

    if version < 3 {
        conn.execute_batch(MIGRATION_V3_SQL)?;
        conn.execute_batch("PRAGMA user_version=3;")?;
        version = 3;
    }

    if version < 4 {
        conn.execute_batch(MIGRATION_V4_SQL)?;
        conn.execute_batch("PRAGMA user_version=4;")?;
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct CompareModelRuns {
    display_name: String,
    profile_json: String,
    runs: Vec<crate::stats::CompletedRunRow>,
}

impl Database {
    fn load_model_runs_for_compare(
        &self,
        profile_id: &str,
    ) -> Result<Option<CompareModelRuns>, DbError> {
        let conn = self.conn.lock().unwrap();
        let meta = conn.query_row(
            "SELECT display_name, profile_json FROM models WHERE profile_id = ?1",
            params![profile_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        );

        let (display_name, profile_json) = match meta {
            Ok(v) => v,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(DbError::Sql(e)),
        };

        let mut stmt = conn.prepare(
            "SELECT r.id, r.context_size, r.ended_at, r.started_at,
                    res.decode_tps, res.total_tps, res.ttft_ms,
                    res.peak_phys_footprint_bytes, res.peak_mlx_active_bytes,
                    res.tokens_in, res.tokens_out, r.params_json
             FROM runs r
             JOIN models m ON r.model_id = m.id
             JOIN results res ON res.run_id = r.id AND res.ttft_ms IS NOT NULL
             WHERE m.profile_id = ?1 AND r.status = 'completed'
             ORDER BY r.id DESC",
        )?;
        let rows = stmt.query_map(params![profile_id], |row| {
            Ok(crate::stats::CompletedRunRow {
                run_id: row.get(0)?,
                context_size: row.get(1)?,
                ended_at: row.get(2)?,
                started_at: row.get(3)?,
                decode_tps: row.get(4)?,
                total_tps: row.get(5)?,
                ttft_ms: row.get(6)?,
                peak_phys: row.get(7)?,
                peak_mlx: row.get(8)?,
                tokens_in: row.get(9)?,
                tokens_out: row.get(10)?,
                use_draft: crate::bench::parse_use_draft(&row.get::<_, String>(11)?),
            })
        })?;
        let mut runs = Vec::new();
        for row in rows {
            runs.push(row?);
        }

        Ok(Some(CompareModelRuns {
            display_name,
            profile_json,
            runs,
        }))
    }
}

fn map_eval_template_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EvalTemplateResultRow> {
    Ok(EvalTemplateResultRow {
        id: row.get(0)?,
        profile_id: row.get(1)?,
        context_size: row.get(2)?,
        template_id: row.get(3)?,
        score: row.get(4)?,
        output_excerpt: row.get(5)?,
        elapsed_ms: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn map_run_list_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunListRow> {
    use crate::profile::generation_kind_from_profile_json;

    let profile_json: String = row.get(3)?;
    let params_json: String = row.get(11)?;
    let generation_kind = generation_kind_from_profile_json(&profile_json);
    let decode_tps: Option<f64> = row.get(7)?;
    let total_tps: Option<f64> = row.get(8)?;
    Ok(RunListRow {
        run_id: row.get(0)?,
        profile_id: row.get(1)?,
        display_name: row.get(2)?,
        generation_kind: generation_kind.clone(),
        kind: row.get(4)?,
        context_size: row.get(5)?,
        status: row.get(6)?,
        decode_tps: crate::tps_tier::effective_display_tps(
            generation_kind.as_str(),
            decode_tps,
            total_tps,
        ),
        peak_phys_footprint_bytes: row.get(9)?,
        ended_at: row.get(10)?,
        use_draft: crate::bench::parse_use_draft(&params_json),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_template_results_crud() {
        let dir = std::env::temp_dir().join(format!("aidash_eval_tpl_{}", std::process::id()));
        let path = dir.join("test.db");
        let db = Database::open(Some(&path)).expect("open");
        assert_eq!(db.user_version().expect("version"), 4);

        let id = db
            .insert_eval_template_result(
                "test/model",
                4096,
                "ctx4k-1",
                85,
                "서울",
                1200,
            )
            .expect("insert");
        assert!(id > 0);

        let rows = db
            .list_eval_template_results("test/model", Some(4096))
            .expect("list");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].template_id, "ctx4k-1");
        assert_eq!(rows[0].score, 85);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn chat_session_crud() {
        let dir = std::env::temp_dir().join(format!("aidash_chat_test_{}", std::process::id()));
        let path = dir.join("test.db");
        let db = Database::open(Some(&path)).expect("open");
        let sid = db
            .create_chat_session("test/model", "첫 대화")
            .expect("create session");
        let msg_id = db
            .append_chat_message(sid, "user", "안녕", None)
            .expect("append");
        assert!(msg_id > 0);
        let sessions = db.list_chat_sessions().expect("list");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].title, "첫 대화");
        let messages = db.load_chat_messages(sid).expect("load");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");
        db.delete_chat_session(sid).expect("delete");
        assert!(db.list_chat_sessions().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn measured_contexts_distinct_completed_only() {
        let dir = std::env::temp_dir().join(format!("aidash_ctx_test_{}", std::process::id()));
        let path = dir.join("test.db");
        let db = Database::open(Some(&path)).expect("open");
        let profile = ModelProfile {
            schema_version: 1,
            id: "test/model".into(),
            display_name: "Test".into(),
            source: crate::profile::ProfileSource {
                kind: "hf".into(),
                hf_repo: "test/model".into(),
                hf_file: String::new(),
                local_path: String::new(),
            },
            model_type: "llm".into(),
            backend: "vllm_mlx".into(),
            io: crate::profile::ProfileIo {
                input: vec!["chat".into()],
                output: "text".into(),
            },
            context: crate::profile::ProfileContext {
                min: 512,
                max: 8192,
                default: 1024,
                sweep_steps: vec![],
            },
            default_params: serde_json::json!({}),
            quantization: None,
            load_timeout_sec: 60,
            notes: String::new(),
            draft_model: None,
            generation_kind: crate::profile::GENERATION_KIND_AUTOREGRESSIVE.into(),
            base_model: None,
        };
        let model_id = db.upsert_model(&profile).expect("upsert");
        let run_a = db
            .insert_run(model_id, "single", None, Some(4096), "{}")
            .expect("insert 4k");
        db.finish_run(run_a, "completed", None).expect("finish 4k");
        let run_b = db
            .insert_run(model_id, "single", None, Some(262144), "{}")
            .expect("insert 256k");
        db.finish_run(run_b, "completed", None).expect("finish 256k");
        let run_c = db
            .insert_run(model_id, "single", None, Some(8192), "{}")
            .expect("insert 8k pending");
        db.finish_run(run_c, "failed", None).expect("fail 8k");

        let contexts = db.measured_contexts().expect("contexts");
        assert_eq!(contexts, vec![4096, 262144]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn eval_template_history_without_profile_file() {
        let dir = std::env::temp_dir().join(format!("aidash_eval_hist_{}", std::process::id()));
        let path = dir.join("test.db");
        let db = Database::open(Some(&path)).expect("open");
        db.insert_eval_template_result("archived/model", 4096, "ctx4k-1", 80, "ok", 1000)
            .expect("insert");

        let rows = db
            .list_eval_template_results("archived/model", None)
            .expect("list");
        assert_eq!(rows.len(), 1);

        let missing = db
            .list_eval_template_results("no/such-model", None)
            .expect("missing");
        assert!(missing.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn schema_roundtrip() {
        let dir = std::env::temp_dir().join(format!("aidash_db_test_{}", std::process::id()));
        let path = dir.join("test.db");
        let db = Database::open(Some(&path)).expect("open");
        let profile = ModelProfile {
            schema_version: 1,
            id: "test/model".into(),
            display_name: "Test".into(),
            source: crate::profile::ProfileSource {
                kind: "hf".into(),
                hf_repo: "test/model".into(),
                hf_file: String::new(),
                local_path: String::new(),
            },
            model_type: "llm".into(),
            backend: "vllm_mlx".into(),
            io: crate::profile::ProfileIo {
                input: vec!["chat".into()],
                output: "text".into(),
            },
            context: crate::profile::ProfileContext {
                min: 512,
                max: 8192,
                default: 1024,
                sweep_steps: vec![],
            },
            default_params: serde_json::json!({}),
            quantization: None,
            load_timeout_sec: 60,
            notes: String::new(),
            draft_model: None,
            generation_kind: crate::profile::GENERATION_KIND_AUTOREGRESSIVE.into(),
            base_model: None,
        };
        let model_id = db.upsert_model(&profile).expect("upsert");
        let run_id = db
            .insert_run(model_id, "single", None, Some(1024), "{}")
            .expect("insert run");
        db.finish_run(run_id, "completed", None).expect("finish");
        let rows = db.list_runs(None).expect("list");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].run_id, run_id);
        let _ = std::fs::remove_dir_all(dir);
    }
}