//! runs/results 집계 — 별도 테이블 없이 SELECT로 산출.

use serde::Serialize;

use crate::db::{Database, DbError};
use crate::profile::{
    generation_kind_from_profile_json, hf_url, model_type_from_profile_json,
    source_kind_from_profile_json,
};
use crate::tps_tier::{self, TpsTier};

pub const DEFAULT_OVERVIEW_CONTEXT: i64 = 4096;

#[derive(Debug, Clone, Serialize)]
pub struct ContextPick {
    pub requested: i64,
    pub actual: i64,
    pub substituted: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverviewRow {
    pub profile_id: String,
    pub display_name: String,
    pub model_type: String,
    pub generation_kind: String,
    pub decode_tps: Option<f64>,
    pub tier: Option<TpsTier>,
    pub ttft_ms: Option<f64>,
    pub context: ContextPick,
    pub hf_url: Option<String>,
    pub measured_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContextStatsRow {
    pub context_size: i64,
    pub decode_tps_min: f64,
    pub decode_tps_avg: f64,
    pub decode_tps_max: f64,
    pub ttft_avg_ms: f64,
    pub run_count: i64,
    pub peak_phys_footprint_bytes: i64,
    pub peak_phys_avg_bytes: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelStats {
    pub profile_id: String,
    pub display_name: String,
    pub generation_kind: String,
    pub total_runs: i64,
    pub latest_measured_at: Option<String>,
    pub current_tier: Option<TpsTier>,
    pub current_decode_tps: Option<f64>,
    pub peak_phys_footprint_bytes: i64,
    pub peak_mlx_active_bytes: i64,
    pub hf_url: Option<String>,
    pub by_context: Vec<ContextStatsRow>,
}

#[derive(Debug, Clone)]
pub(crate) struct CompletedRunRow {
    pub run_id: i64,
    pub context_size: Option<i64>,
    pub ended_at: Option<String>,
    pub started_at: String,
    pub decode_tps: Option<f64>,
    pub total_tps: Option<f64>,
    pub ttft_ms: f64,
    pub peak_phys: i64,
    pub peak_mlx: i64,
    pub tokens_in: Option<i64>,
    pub tokens_out: Option<i64>,
    pub use_draft: Option<bool>,
}

#[derive(Debug, Clone)]
struct ModelRuns {
    profile_id: String,
    display_name: String,
    profile_json: String,
    runs: Vec<CompletedRunRow>,
}

/// 가장 가까운 컨텍스트를 선택한다. 동일 거리면 더 작은 컨텍스트를 우선한다.
pub fn nearest_context(target: i64, available: &[i64]) -> Option<ContextPick> {
    if available.is_empty() {
        return None;
    }
    if available.contains(&target) {
        return Some(ContextPick {
            requested: target,
            actual: target,
            substituted: false,
        });
    }

    let mut best = available[0];
    let mut best_dist = (best - target).unsigned_abs();
    for &ctx in &available[1..] {
        let dist = (ctx - target).unsigned_abs();
        if dist < best_dist || (dist == best_dist && ctx < best) {
            best = ctx;
            best_dist = dist;
        }
    }

    Some(ContextPick {
        requested: target,
        actual: best,
        substituted: true,
    })
}

fn pick_latest_at_context(runs: &[CompletedRunRow], context: i64) -> Option<&CompletedRunRow> {
    runs.iter()
        .filter(|r| r.context_size == Some(context))
        .max_by(|a, b| {
            a.ended_at
                .cmp(&b.ended_at)
                .then_with(|| a.run_id.cmp(&b.run_id))
        })
}

pub(crate) fn pick_representative_run<'a>(
    runs: &'a [CompletedRunRow],
    target_context: i64,
) -> Option<(&'a CompletedRunRow, ContextPick)> {
    let contexts: Vec<i64> = runs
        .iter()
        .filter_map(|r| r.context_size)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let pick = nearest_context(target_context, &contexts)?;
    let run = pick_latest_at_context(runs, pick.actual)?;
    Some((run, pick))
}

impl Database {
    fn load_model_runs(&self, profile_id: Option<&str>) -> Result<Vec<ModelRuns>, DbError> {
        let conn = self.conn.lock().unwrap();
        let sql = if profile_id.is_some() {
            "SELECT m.profile_id, m.display_name, m.profile_json,
                    r.id, r.context_size, r.ended_at, r.started_at,
                    res.decode_tps, res.total_tps, res.ttft_ms,
                    res.peak_phys_footprint_bytes, res.peak_mlx_active_bytes,
                    res.tokens_in, res.tokens_out, r.params_json
             FROM models m
             JOIN runs r ON r.model_id = m.id AND r.status = 'completed'
             JOIN results res ON res.run_id = r.id AND res.ttft_ms IS NOT NULL
             WHERE m.profile_id = ?1
             ORDER BY m.profile_id, r.id DESC"
        } else {
            "SELECT m.profile_id, m.display_name, m.profile_json,
                    r.id, r.context_size, r.ended_at, r.started_at,
                    res.decode_tps, res.total_tps, res.ttft_ms,
                    res.peak_phys_footprint_bytes, res.peak_mlx_active_bytes,
                    res.tokens_in, res.tokens_out, r.params_json
             FROM models m
             JOIN runs r ON r.model_id = m.id AND r.status = 'completed'
             JOIN results res ON res.run_id = r.id AND res.ttft_ms IS NOT NULL
             ORDER BY m.profile_id, r.id DESC"
        };

        let mut by_model: std::collections::BTreeMap<String, ModelRuns> =
            std::collections::BTreeMap::new();

        let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<CompletedRunRow> {
            let params_json: String = row.get(14)?;
            Ok(CompletedRunRow {
                run_id: row.get(3)?,
                context_size: row.get(4)?,
                ended_at: row.get(5)?,
                started_at: row.get(6)?,
                decode_tps: row.get(7)?,
                total_tps: row.get(8)?,
                ttft_ms: row.get(9)?,
                peak_phys: row.get(10)?,
                peak_mlx: row.get(11)?,
                tokens_in: row.get(12)?,
                tokens_out: row.get(13)?,
                use_draft: crate::bench::parse_use_draft(&params_json),
            })
        };

        if let Some(pid) = profile_id {
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt.query_map(rusqlite::params![pid], |row| {
                let profile_id: String = row.get(0)?;
                let display_name: String = row.get(1)?;
                let profile_json: String = row.get(2)?;
                let run = map_row(row)?;
                Ok((profile_id, display_name, profile_json, run))
            })?;
            for row in rows {
                let (profile_id, display_name, profile_json, run) = row?;
                by_model
                    .entry(profile_id.clone())
                    .or_insert_with(|| ModelRuns {
                        profile_id,
                        display_name,
                        profile_json,
                        runs: Vec::new(),
                    })
                    .runs
                    .push(run);
            }
        } else {
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt.query_map([], |row| {
                let profile_id: String = row.get(0)?;
                let display_name: String = row.get(1)?;
                let profile_json: String = row.get(2)?;
                let run = map_row(row)?;
                Ok((profile_id, display_name, profile_json, run))
            })?;
            for row in rows {
                let (profile_id, display_name, profile_json, run) = row?;
                by_model
                    .entry(profile_id.clone())
                    .or_insert_with(|| ModelRuns {
                        profile_id,
                        display_name,
                        profile_json,
                        runs: Vec::new(),
                    })
                    .runs
                    .push(run);
            }
        }

        // 모델은 있지만 completed 런이 없는 경우도 리더보드에 포함할 수 있도록 models 테이블 스캔
        let all_models_sql = if profile_id.is_some() {
            "SELECT profile_id, display_name, profile_json FROM models WHERE profile_id = ?1"
        } else {
            "SELECT profile_id, display_name, profile_json FROM models ORDER BY profile_id"
        };
        if let Some(pid) = profile_id {
            let mut stmt = conn.prepare(all_models_sql)?;
            let rows = stmt.query_map(rusqlite::params![pid], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;
            for row in rows {
                let (profile_id, display_name, profile_json) = row?;
                by_model.entry(profile_id.clone()).or_insert_with(|| ModelRuns {
                    profile_id,
                    display_name,
                    profile_json,
                    runs: Vec::new(),
                });
            }
        } else {
            let mut stmt = conn.prepare(all_models_sql)?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;
            for row in rows {
                let (profile_id, display_name, profile_json) = row?;
                by_model.entry(profile_id.clone()).or_insert_with(|| ModelRuns {
                    profile_id,
                    display_name,
                    profile_json,
                    runs: Vec::new(),
                });
            }
        }

        Ok(by_model.into_values().collect())
    }

    pub fn stats_overview(&self, target_context: i64) -> Result<Vec<OverviewRow>, DbError> {
        let models = self.load_model_runs(None)?;
        let mut tps_rows = Vec::new();
        let mut timing_rows = Vec::new();

        for model in models {
            let kind = source_kind_from_profile_json(&model.profile_json);
            let url = hf_url(&model.profile_id, kind.as_deref());

            let model_type = model_type_from_profile_json(&model.profile_json)
                .unwrap_or_else(|| "llm".into());
            let generation_kind = generation_kind_from_profile_json(&model.profile_json);

            if let Some((run, pick)) = pick_representative_run(&model.runs, target_context) {
                let display_tps = tps_tier::effective_display_tps(
                    &generation_kind,
                    run.decode_tps,
                    run.total_tps,
                );
                let row = OverviewRow {
                    profile_id: model.profile_id.clone(),
                    display_name: model.display_name,
                    model_type: model_type.clone(),
                    generation_kind: generation_kind.clone(),
                    decode_tps: display_tps,
                    tier: tps_tier::tier_for_run(
                        &generation_kind,
                        run.decode_tps,
                        run.total_tps,
                    ),
                    ttft_ms: Some(run.ttft_ms),
                    context: pick,
                    hf_url: url,
                    measured_at: run.ended_at.clone(),
                };
                if display_tps.is_some() {
                    tps_rows.push(row);
                } else {
                    timing_rows.push(row);
                }
            }
        }

        tps_rows.sort_by(|a, b| {
            b.decode_tps
                .unwrap_or(0.0)
                .partial_cmp(&a.decode_tps.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        timing_rows.sort_by(|a, b| {
            b.ttft_ms
                .unwrap_or(0.0)
                .partial_cmp(&a.ttft_ms.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut rows = tps_rows;
        rows.extend(timing_rows);
        Ok(rows)
    }

    pub fn stats_model(&self, profile_id: &str) -> Result<ModelStats, DbError> {
        let conn = self.conn.lock().unwrap();
        let (display_name, profile_json): (String, String) = conn
            .query_row(
                "SELECT display_name, profile_json FROM models WHERE profile_id = ?1",
                rusqlite::params![profile_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    DbError::Validation(format!("model not found: {profile_id}"))
                }
                other => DbError::Sql(other),
            })?;

        let total_runs: i64 = conn.query_row(
            "SELECT COUNT(*) FROM runs r JOIN models m ON r.model_id = m.id WHERE m.profile_id = ?1",
            rusqlite::params![profile_id],
            |row| row.get(0),
        )?;

        drop(conn);

        let models = self.load_model_runs(Some(profile_id))?;
        let model_runs = models.into_iter().next().unwrap_or(ModelRuns {
            profile_id: profile_id.to_string(),
            display_name: display_name.clone(),
            profile_json: profile_json.clone(),
            runs: Vec::new(),
        });

        let generation_kind = generation_kind_from_profile_json(&profile_json);

        let latest = model_runs.runs.iter().max_by(|a, b| {
            a.ended_at
                .cmp(&b.ended_at)
                .then_with(|| a.run_id.cmp(&b.run_id))
        });

        let mut by_ctx: std::collections::BTreeMap<i64, Vec<&CompletedRunRow>> =
            std::collections::BTreeMap::new();
        for run in &model_runs.runs {
            if let Some(ctx) = run.context_size {
                by_ctx.entry(ctx).or_default().push(run);
            }
        }

        let by_context: Vec<ContextStatsRow> = by_ctx
            .into_iter()
            .map(|(ctx, runs)| {
                let count = runs.len() as i64;
                let tps_values: Vec<f64> = runs
                    .iter()
                    .filter_map(|r| {
                        tps_tier::effective_display_tps(
                            &generation_kind,
                            r.decode_tps,
                            r.total_tps,
                        )
                    })
                    .collect();
                let decode_min = tps_values
                    .iter()
                    .copied()
                    .fold(f64::INFINITY, f64::min);
                let decode_max = tps_values
                    .iter()
                    .copied()
                    .fold(f64::NEG_INFINITY, f64::max);
                let decode_avg = if tps_values.is_empty() {
                    0.0
                } else {
                    tps_values.iter().sum::<f64>() / tps_values.len() as f64
                };
                let ttft_avg = runs.iter().map(|r| r.ttft_ms).sum::<f64>() / count as f64;
                let peak_phys = runs.iter().map(|r| r.peak_phys).max().unwrap_or(0);
                let peak_phys_avg = if count > 0 {
                    runs.iter().map(|r| r.peak_phys).sum::<i64>() / count
                } else {
                    0
                };
                ContextStatsRow {
                    context_size: ctx,
                    decode_tps_min: decode_min,
                    decode_tps_avg: decode_avg,
                    decode_tps_max: decode_max,
                    ttft_avg_ms: ttft_avg,
                    run_count: count,
                    peak_phys_footprint_bytes: peak_phys,
                    peak_phys_avg_bytes: peak_phys_avg,
                }
            })
            .collect();

        let peak_phys = model_runs
            .runs
            .iter()
            .map(|r| r.peak_phys)
            .max()
            .unwrap_or(0);
        let peak_mlx = model_runs
            .runs
            .iter()
            .map(|r| r.peak_mlx)
            .max()
            .unwrap_or(0);

        let kind = source_kind_from_profile_json(&profile_json);

        Ok(ModelStats {
            profile_id: profile_id.to_string(),
            display_name,
            generation_kind: generation_kind.clone(),
            total_runs,
            latest_measured_at: latest.and_then(|r| r.ended_at.clone()),
            current_tier: latest.and_then(|r| {
                tps_tier::tier_for_run(&generation_kind, r.decode_tps, r.total_tps)
            }),
            current_decode_tps: latest.and_then(|r| {
                tps_tier::effective_display_tps(&generation_kind, r.decode_tps, r.total_tps)
            }),
            peak_phys_footprint_bytes: peak_phys,
            peak_mlx_active_bytes: peak_mlx,
            hf_url: hf_url(profile_id, kind.as_deref()),
            by_context,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_context_exact() {
        let pick = nearest_context(4096, &[1024, 4096, 8192]).unwrap();
        assert_eq!(pick.actual, 4096);
        assert!(!pick.substituted);
    }

    #[test]
    fn nearest_context_below() {
        let pick = nearest_context(4096, &[1024, 2048]).unwrap();
        assert_eq!(pick.actual, 2048);
        assert!(pick.substituted);
    }

    #[test]
    fn nearest_context_above() {
        let pick = nearest_context(4096, &[8192, 16384]).unwrap();
        assert_eq!(pick.actual, 8192);
        assert!(pick.substituted);
    }

    #[test]
    fn nearest_context_none() {
        assert!(nearest_context(4096, &[]).is_none());
    }
}