//! 결과 카드 HTML 생성

use std::path::PathBuf;

use crate::db::{Database, RunExport};
use crate::profile::{hf_url, source_kind_from_profile_json};
use crate::system_device_label;
use crate::tps_tier::{self, format_processing_time_ms};

#[derive(Debug, Clone)]
pub struct ExportRequest {
    pub run_ids: Vec<u64>,
    pub output_dir: PathBuf,
}

#[derive(Debug)]
pub enum ExportError {
    Db(crate::db::DbError),
    Io(std::io::Error),
    Validation(String),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::Db(e) => write!(f, "{e}"),
            ExportError::Io(e) => write!(f, "io error: {e}"),
            ExportError::Validation(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for ExportError {}

impl From<crate::db::DbError> for ExportError {
    fn from(value: crate::db::DbError) -> Self {
        ExportError::Db(value)
    }
}

impl From<std::io::Error> for ExportError {
    fn from(value: std::io::Error) -> Self {
        ExportError::Io(value)
    }
}

fn format_gib(bytes: Option<i64>) -> String {
    bytes
        .map(|b| format!("{:.2} GiB", b as f64 / (1024.0 * 1024.0 * 1024.0)))
        .unwrap_or_else(|| "-".into())
}

fn format_measured_at(millis_str: &str) -> String {
    let Ok(ms) = millis_str.parse::<i64>() else {
        return millis_str.to_string();
    };
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
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

fn tier_badge_html(decode_tps: Option<f64>) -> String {
    match decode_tps {
        Some(tps) => {
            let tier = tps_tier::tps_tier(tps);
            let info = tier.info();
            let color = match tier {
                tps_tier::TpsTier::Unusable => "#ef4444",
                tps_tier::TpsTier::Sluggish => "#f97316",
                tps_tier::TpsTier::Ideal => "#22c55e",
                tps_tier::TpsTier::Fast => "#3b82f6",
                tps_tier::TpsTier::Realtime => "#a855f7",
            };
            format!(
                r#"<span class="badge" style="background:{color}">{badge} {label} ({tps:.1} TPS)</span>"#,
                color = color,
                badge = info.badge,
                label = info.label,
                tps = tps,
            )
        }
        None => r#"<span class="badge muted">-</span>"#.into(),
    }
}

fn render_card(export: &RunExport, device: &str) -> String {
    let kind = source_kind_from_profile_json(&export.run.profile_id).or_else(|| {
        export
            .run
            .profile_id
            .starts_with("local/")
            .then_some("local".into())
    });
    let hf_link = hf_url(&export.run.profile_id, kind.as_deref());
    let model_html = if let Some(url) = hf_link {
        format!(
            r#"<a href="{url}" target="_blank" rel="noopener">{id}</a>"#,
            url = url,
            id = export.run.profile_id
        )
    } else {
        format!("{} (local)", export.run.profile_id)
    };

    let res = export.results.as_ref();
    let decode_tps = res.and_then(|r| r.decode_tps);
    let metric_line = if decode_tps.is_some() || res.map(|r| r.tokens_out.unwrap_or(0) > 0).unwrap_or(false) {
        format!(
            "<div class=\"metric\"><span class=\"label\">Decode TPS</span>{}</div>",
            tier_badge_html(decode_tps)
        )
    } else {
        format!(
            "<div class=\"metric\"><span class=\"label\">처리</span><span>{}</span></div>",
            format_processing_time_ms(res.and_then(|r| r.ttft_ms))
        )
    };

    let ttft = res
        .and_then(|r| r.ttft_ms)
        .map(|v| format!("{v:.1} ms"))
        .unwrap_or_else(|| "-".into());
    let peak_ram = format_gib(res.and_then(|r| r.peak_phys_footprint_bytes));
    let cpu = res
        .and_then(|r| r.avg_cpu_pct)
        .map(|v| format!("{v:.1}%"))
        .unwrap_or_else(|| "-".into());
    let measured = export
        .run
        .ended_at
        .as_deref()
        .or(Some(export.run.started_at.as_str()))
        .map(format_measured_at)
        .unwrap_or_else(|| "-".into());
    let context = export
        .run
        .context_size
        .map(|c| c.to_string())
        .unwrap_or_else(|| "-".into());

    format!(
        r#"<article class="card">
  <header>
    <h2>{display}</h2>
    <p class="model">{model_html}</p>
    <p class="meta">Run #{run_id} · ctx {context} · {status}</p>
  </header>
  <div class="metrics">
    {metric_line}
    <div class="metric"><span class="label">TTFT</span><span>{ttft}</span></div>
    <div class="metric"><span class="label">Peak RAM</span><span>{peak_ram}</span></div>
    <div class="metric"><span class="label">CPU avg</span><span>{cpu}</span></div>
    <div class="metric"><span class="label">측정일</span><span>{measured}</span></div>
    <div class="metric"><span class="label">기기</span><span>{device}</span></div>
  </div>
</article>"#,
        display = export.run.display_name,
        model_html = model_html,
        run_id = export.run.id,
        context = context,
        status = export.run.status,
        metric_line = metric_line,
        ttft = ttft,
        peak_ram = peak_ram,
        cpu = cpu,
        measured = measured,
        device = device,
    )
}

pub fn generate_card_html(db: &Database, run_ids: &[u64]) -> Result<String, ExportError> {
    if run_ids.is_empty() {
        return Err(ExportError::Validation("no run ids provided".into()));
    }
    let device = system_device_label();
    let mut cards = Vec::new();
    for &id in run_ids {
        let export = db.export_run(id as i64)?;
        cards.push(render_card(&export, &device));
    }

    let cards_html = cards.join("\n");
    let layout_class = if cards.len() > 1 { "compare" } else { "single" };

    Ok(format!(
        r#"<!DOCTYPE html>
<html lang="ko">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>AI Dashboard — Result Card</title>
<style>
* {{ box-sizing: border-box; margin: 0; padding: 0; }}
body {{
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
  background: #0f1117;
  color: #e8eaed;
  padding: 2rem;
  line-height: 1.5;
}}
h1 {{ font-size: 1.25rem; font-weight: 600; margin-bottom: 1.5rem; color: #9aa0a6; }}
.grid.{layout_class} {{
  display: grid;
  gap: 1.25rem;
  grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
}}
.card {{
  background: linear-gradient(145deg, #1a1d27 0%, #14161e 100%);
  border: 1px solid #2d3142;
  border-radius: 12px;
  padding: 1.25rem;
  box-shadow: 0 4px 24px rgba(0,0,0,0.4);
}}
.card h2 {{ font-size: 1.1rem; margin-bottom: 0.25rem; }}
.model a {{ color: #8ab4f8; text-decoration: none; }}
.model a:hover {{ text-decoration: underline; }}
.meta {{ font-size: 0.8rem; color: #9aa0a6; margin-bottom: 1rem; }}
.metrics {{ display: grid; gap: 0.5rem; }}
.metric {{ display: flex; justify-content: space-between; align-items: center; font-size: 0.9rem; }}
.label {{ color: #9aa0a6; }}
.badge {{
  display: inline-block;
  padding: 0.15rem 0.5rem;
  border-radius: 6px;
  font-size: 0.85rem;
  font-weight: 500;
  color: #fff;
}}
.badge.muted {{ background: #3c4043; color: #9aa0a6; }}
.footnote {{
  margin-top: 2rem;
  padding-top: 1rem;
  border-top: 1px solid #2d3142;
  font-size: 0.75rem;
  color: #9aa0a6;
}}
.footnote table {{ width: 100%; border-collapse: collapse; margin-top: 0.5rem; }}
.footnote td {{ padding: 0.2rem 0.5rem 0.2rem 0; }}
</style>
</head>
<body>
<h1>AI Dashboard — Benchmark Result Card</h1>
<div class="grid {layout_class}">
{cards_html}
</div>
<footer class="footnote">
<p><strong>TPS 등급 기준</strong> (decode TPS)</p>
<table>
<tr><td>🔴 사용 불가</td><td>&lt; 10 TPS</td></tr>
<tr><td>🟠 답답함</td><td>10–40 TPS</td></tr>
<tr><td>🟢 이상적</td><td>40–60 TPS</td></tr>
<tr><td>🔵 빠름</td><td>60–100 TPS</td></tr>
<tr><td>🟣 실시간급</td><td>≥ 100 TPS</td></tr>
</table>
</footer>
</body>
</html>"#,
        layout_class = layout_class,
        cards_html = cards_html,
    ))
}

pub fn export_card(db: &Database, request: &ExportRequest) -> Result<PathBuf, ExportError> {
    if request.run_ids.is_empty() {
        return Err(ExportError::Validation("no run ids provided".into()));
    }
    std::fs::create_dir_all(&request.output_dir)?;
    let ids_tag: String = request
        .run_ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join("_");
    let filename = format!("card_{ids_tag}.html");
    let out_path = request.output_dir.join(&filename);
    let html = generate_card_html(db, &request.run_ids)?;
    std::fs::write(&out_path, html)?;
    Ok(out_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_html_smoke() {
        let dir = std::env::temp_dir().join(format!("aidash_export_test_{}", std::process::id()));
        let db_path = dir.join("test.db");
        let _ = std::fs::remove_dir_all(&dir);
        let db = Database::open(Some(&db_path)).expect("open");
        let profile = crate::profile::ModelProfile {
            schema_version: 1,
            id: "test/model".into(),
            display_name: "Test Model".into(),
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
        };
        let model_id = db.upsert_model(&profile).expect("upsert");
        let run_id = db
            .insert_run(model_id, "single", None, Some(1024), "{}")
            .expect("run");
        db.insert_results(
            run_id,
            &crate::client::StreamStats {
                ttft_ms: 100.0,
                prefill_tps: 50.0,
                decode_tps: Some(45.0),
                total_tps: 40.0,
                tokens_in: 100,
                tokens_out: 50,
            },
            1_000_000_000,
            500_000_000,
            12.5,
        )
        .expect("results");
        db.finish_run(run_id, "completed", None).expect("finish");

        let html = generate_card_html(&db, &[run_id as u64]).expect("html");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("test/model"));
        assert!(html.contains("🟢"));
        let _ = std::fs::remove_dir_all(dir);
    }
}