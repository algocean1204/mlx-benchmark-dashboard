//! 품질 미니평가 (스펙 §8 ⑧)

use std::path::PathBuf;
use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::time::timeout;

use crate::bench::{spawn_sample_collector, take_recorder};
use crate::client;
use crate::db::Database;
use crate::events::CoreEvent;
use crate::lifecycle::{Command, LifecycleHandle, LifecycleState, StartParams};
use crate::profile::ModelProfile;
use crate::pyproc::ChildSpec;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSet {
    pub version: u32,
    pub name: String,
    pub items: Vec<EvalItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalItem {
    pub id: String,
    pub prompt: String,
    pub answer: String,
    #[serde(default = "default_match")]
    pub r#match: String,
}

fn default_match() -> String {
    "exact".into()
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalItemResult {
    pub id: String,
    pub correct: bool,
    pub expected: String,
    pub actual: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalSummary {
    pub run_id: i64,
    pub profile_id: String,
    pub total: usize,
    pub correct: usize,
    pub score: f64,
    pub items: Vec<EvalItemResult>,
}

pub fn load_eval_set(path: &std::path::Path) -> Result<EvalSet, String> {
    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&contents).map_err(|e| e.to_string())
}

pub fn normalize_answer(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn score_answer(expected: &str, actual: &str, match_kind: &str) -> bool {
    let exp = normalize_answer(expected);
    let act = normalize_answer(actual);
    match match_kind {
        "exact" => act == exp,
        "contains" => act.contains(&exp),
        _ => false,
    }
}

pub fn ensure_llm_profile(profile: &ModelProfile) -> Result<(), String> {
    match profile.model_type.as_str() {
        "llm" | "multimodal" => Ok(()),
        other => Err(format!(
            "LLM/multimodal 프로파일만 평가 가능 (현재 model_type={other})"
        )),
    }
}

pub async fn run_quality_eval(
    db: &Database,
    profile: ModelProfile,
    eval_set: &EvalSet,
    python_dir: PathBuf,
    child_spec: Option<ChildSpec>,
    port: Option<u16>,
    progress_tx: Option<broadcast::Sender<CoreEvent>>,
) -> Result<EvalSummary, String> {
    ensure_llm_profile(&profile)?;

    let model_id = db.upsert_model(&profile).map_err(|e| e.to_string())?;
    let context = profile.context.default;
    let params_json = serde_json::json!({
        "eval_set": eval_set.name,
        "context": context,
        "items": eval_set.items.len(),
    });
    let run_id = db
        .insert_run(
            model_id,
            "quality_eval",
            None,
            Some(context),
            &params_json.to_string(),
        )
        .map_err(|e| e.to_string())?;

    let handle = LifecycleHandle::spawn();
    let (recorder_arc, collector_task) = spawn_sample_collector(&handle.event_tx);

    let start = StartParams {
        profile: profile.clone(),
        context,
        mem_limit_gb: None,
        port,
        python_dir,
        child_spec,
    };

    if handle.command_tx.send(Command::Start(start)).await.is_err() {
        db.finish_run(run_id, "failed", Some("failed to start server"))
            .map_err(|e| e.to_string())?;
        return Err("failed to start server".into());
    }

    let load_deadline = Duration::from_secs(profile.load_timeout_sec.saturating_add(30));
    let ready = timeout(load_deadline, handle.wait_for_state(LifecycleState::Ready)).await;

    if ready.is_err() || *handle.state_rx.borrow() != LifecycleState::Ready {
        let _ = handle.command_tx.send(Command::Stop).await;
        handle.wait_for_state(LifecycleState::Idle).await;
        let msg = "server failed to reach ready state";
        db.finish_run(run_id, "failed", Some(msg))
            .map_err(|e| e.to_string())?;
        return Err(msg.into());
    }

    let server_port = {
        let mut port_rx = handle.port_rx.clone();
        loop {
            if let Some(p) = *port_rx.borrow() {
                break p;
            }
            if port_rx.changed().await.is_err() {
                break 0;
            }
        }
    };

    if server_port == 0 {
        let _ = handle.command_tx.send(Command::Stop).await;
        handle.wait_for_state(LifecycleState::Idle).await;
        let msg = "server port unavailable";
        db.finish_run(run_id, "failed", Some(msg))
            .map_err(|e| e.to_string())?;
        return Err(msg.into());
    }

    let http = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let _ = handle.command_tx.send(Command::BeginWork).await;

    let mut item_results = Vec::new();
    let mut correct_count = 0usize;

    for item in &eval_set.items {
        if let Some(tx) = &progress_tx {
            let _ = tx.send(CoreEvent::Log {
                level: "info".into(),
                message: format!("eval {}: {}", eval_set.name, item.id),
            });
        }

        let actual = client::chat_completion(
            &http,
            server_port,
            &profile.id,
            &item.prompt,
            64,
            0.0,
        )
        .await
        .unwrap_or_else(|e| format!("[error: {e}]"));

        let correct = score_answer(&item.answer, &actual, &item.r#match);
        if correct {
            correct_count += 1;
        }
        item_results.push(EvalItemResult {
            id: item.id.clone(),
            correct,
            expected: item.answer.clone(),
            actual,
        });
    }

    let _ = handle.command_tx.send(Command::EndWork).await;
    let _ = handle.command_tx.send(Command::Stop).await;
    handle.wait_for_state(LifecycleState::Idle).await;

    let recorder = take_recorder(recorder_arc, collector_task).await;
    db.insert_samples(run_id, recorder.samples())
        .map_err(|e| e.to_string())?;

    let score = if eval_set.items.is_empty() {
        0.0
    } else {
        correct_count as f64 / eval_set.items.len() as f64
    };

    db.insert_quality_result(run_id, score)
        .map_err(|e| e.to_string())?;
    db.finish_run(run_id, "completed", None)
        .map_err(|e| e.to_string())?;

    Ok(EvalSummary {
        run_id,
        profile_id: profile.id,
        total: eval_set.items.len(),
        correct: correct_count,
        score,
        items: item_results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_exact_match() {
        assert!(score_answer("42", "  42  ", "exact"));
        assert!(!score_answer("42", "43", "exact"));
    }

    #[test]
    fn score_contains_match() {
        assert!(score_answer("서울", "대한민국의 수도는 서울입니다", "contains"));
        assert!(!score_answer("부산", "서울", "contains"));
    }

    #[test]
    fn normalize_strips_case_and_spaces() {
        assert_eq!(normalize_answer("  Hello   World "), "hello world");
    }
}