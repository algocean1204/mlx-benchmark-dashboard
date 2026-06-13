//! 컨텍스트별 평가 템플릿 — 필러 합성, needle 삽입, 키워드 채점

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio::time::timeout;

use crate::bench::{spawn_sample_collector, take_recorder};
use crate::client;
use crate::db::{Database, EvalTemplateResultRow};
use crate::find_project_root;
use crate::eval::ensure_llm_profile;
use crate::events::CoreEvent;
use crate::lifecycle::{Command, LifecycleHandle, LifecycleState, StartParams};
use crate::profile::ModelProfile;
use crate::pyproc::ChildSpec;

pub const KO_CHARS_PER_TOKEN: f64 = 2.5;
pub const FILLER_FILL_RATIO: f64 = 0.80;
pub const MAX_OUTPUT_EXCERPT_CHARS: usize = 500;
pub const EVAL_MAX_TOKENS: u32 = 256;

pub const STANDARD_CONTEXT_SIZES: [u32; 9] = [
    1024, 4096, 16384, 32768, 65536, 131072, 262144, 524288, 1_048_576,
];

const FILLER_PARAGRAPHS: &[&str] = &[
    "도시 계획 보고서에 따르면 공원 면적은 주민 1인당 최소 9제곱미터를 권장한다. 녹지 확충은 열섬 현상 완화와 생활 만족도 향상에 기여한다.",
    "지역 문화재 보존 지침은 원형 유지를 원칙으로 하되, 안전 점검과 접근성 개선을 위한 최소한의 보수는 허용한다.",
    "농업 통계 연보는 작물별 수확량, 재배 면적, 기상 영향을 연도별로 정리한다. 데이터는 표준 양식으로 제출되어야 한다.",
    "교통 안전 캠페인은 보행자 횡단보도 준수, 어린이 보호구역 감속, 음주 운전 예방을 핵심 메시지로 삼는다.",
    "공공 도서관 운영 규정은 대출 기한, 연장 횟수, 예약 도서 처리 절차를 명시한다. 회원 자격은 지역 거주자에게 우선 부여된다.",
    "에너지 효율 가이드는 건물 단열, 조명 교체, 냉난방 설정 온도 권장치를 포함한다. 절감 효과는 계절별로 다르게 나타난다.",
    "수질 관리 요약은 정수장 처리 단계, 잔류 염소 농도, 배수지 점검 일정을 기록한다. 이상 징후 발견 시 즉시 보고한다.",
    "소상공인 지원 프로그램은 창업 교육, 저리 융자, 온라인 판로 개척 컨설팅을 제공한다. 신청 자격은 사업자 등록 기준을 따른다.",
    "기상 정보 일지는 일최고기온, 일최저기온, 강수량, 풍속을 시간대별로 기록한다. 특보 발령 시 별도 항목에 표시한다.",
    "건강 검진 안내서는 연령대별 권장 검사 항목, 주기, 생활 습관 개선 팁을 담고 있다. 결과 상담은 전문의와 진행한다.",
    "재활용 분리 배출 지침은 플라스틱, 종이, 캔, 유리, 일반 쓰레기의 분류 기준을 설명한다. 오분류 시 재처리 비용이 증가한다.",
    "관광 안내 자료는 지역 명소, 축제 일정, 대중교통 이용 방법을 소개한다. 외국어 번역본은 주요 관광지에서 배포된다.",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalTemplateSet {
    pub schema_version: u32,
    pub templates: Vec<EvalTemplate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalTemplate {
    pub id: String,
    pub context_size: u32,
    pub kind: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub needles: Option<Vec<NeedleSpec>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    pub scoring: KeywordScoring,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeedleSpec {
    pub text: String,
    pub depth_pct: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeywordScoring {
    pub r#type: String,
    pub keywords: Vec<String>,
    pub weights: Vec<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalTemplateItemResult {
    pub template_id: String,
    pub description: String,
    pub score: u32,
    pub output: String,
    pub output_excerpt: String,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalTemplateSummary {
    pub profile_id: String,
    pub context_size: u32,
    pub total_score: u32,
    pub items: Vec<EvalTemplateItemResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvalTemplateHistoryEntry {
    pub context_size: u32,
    pub total_score: u32,
    pub created_at: String,
    pub items: Vec<EvalTemplateItemResult>,
}

pub fn default_template_set_path(root: &Path) -> PathBuf {
    root.join("eval_sets").join("context_templates_ko.json")
}

pub fn load_template_set(path: &Path) -> Result<EvalTemplateSet, String> {
    let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let set: EvalTemplateSet = serde_json::from_str(&contents).map_err(|e| e.to_string())?;
    validate_template_set(&set)?;
    Ok(set)
}

pub fn validate_template_set(set: &EvalTemplateSet) -> Result<(), String> {
    if set.templates.is_empty() {
        return Err("template set is empty".into());
    }
    for tpl in &set.templates {
        validate_template(tpl)?;
    }
    Ok(())
}

fn validate_template(tpl: &EvalTemplate) -> Result<(), String> {
    if tpl.id.is_empty() {
        return Err("template id is empty".into());
    }
    match tpl.kind.as_str() {
        "direct" => {
            if tpl.prompt.as_ref().is_none_or(|p| p.trim().is_empty()) {
                return Err(format!("direct template {} missing prompt", tpl.id));
            }
        }
        "needle" => {
            let needles = tpl
                .needles
                .as_ref()
                .ok_or_else(|| format!("needle template {} missing needles", tpl.id))?;
            if needles.is_empty() {
                return Err(format!("needle template {} has empty needles", tpl.id));
            }
            for n in needles {
                if n.depth_pct > 100 {
                    return Err(format!(
                        "needle template {} depth_pct out of range",
                        tpl.id
                    ));
                }
            }
            if tpl.question.as_ref().is_none_or(|q| q.trim().is_empty()) {
                return Err(format!("needle template {} missing question", tpl.id));
            }
        }
        other => return Err(format!("unknown template kind: {other}")),
    }
    validate_scoring(&tpl.scoring, &tpl.id)?;
    Ok(())
}

fn validate_scoring(scoring: &KeywordScoring, id: &str) -> Result<(), String> {
    if scoring.r#type != "keyword" {
        return Err(format!("template {id}: unsupported scoring type"));
    }
    if scoring.keywords.is_empty() {
        return Err(format!("template {id}: no keywords"));
    }
    if scoring.weights.len() != scoring.keywords.len() {
        return Err(format!("template {id}: keywords/weights length mismatch"));
    }
    if scoring.weights.iter().all(|w| *w == 0) {
        return Err(format!("template {id}: all weights are zero"));
    }
    Ok(())
}

pub fn templates_for_context(set: &EvalTemplateSet, context_size: u32) -> Vec<&EvalTemplate> {
    set.templates
        .iter()
        .filter(|t| t.context_size == context_size)
        .collect()
}

pub fn available_context_sizes(set: &EvalTemplateSet) -> Vec<u32> {
    let mut sizes: Vec<u32> = set
        .templates
        .iter()
        .map(|t| t.context_size)
        .collect();
    sizes.sort_unstable();
    sizes.dedup();
    sizes
}

pub fn estimate_tokens(text: &str) -> u32 {
    let chars = text.chars().count();
    if chars == 0 {
        return 0;
    }
    ((chars as f64) / KO_CHARS_PER_TOKEN).ceil() as u32
}

pub fn normalize_for_match(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn score_keywords(output: &str, keywords: &[String], weights: &[u32]) -> u32 {
    let norm = normalize_for_match(output);
    let total_weight: u32 = weights.iter().sum();
    if total_weight == 0 {
        return 0;
    }
    let mut earned = 0u32;
    for (kw, w) in keywords.iter().zip(weights.iter()) {
        if norm.contains(&normalize_for_match(kw)) {
            earned = earned.saturating_add(*w);
        }
    }
    ((earned as f64 / total_weight as f64) * 100.0).round() as u32
}

fn shuffle_indices(len: usize, seed: u64) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..len).collect();
    let mut state = seed;
    for i in (1..indices.len()).rev() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let j = (state as usize) % (i + 1);
        indices.swap(i, j);
    }
    indices
}

pub fn synthesize_filler(target_tokens: u32, seed: u64) -> String {
    if target_tokens == 0 {
        return String::new();
    }
    let target_chars = (target_tokens as f64 * KO_CHARS_PER_TOKEN).ceil() as usize;
    let order = shuffle_indices(FILLER_PARAGRAPHS.len(), seed);
    let mut result = String::with_capacity(target_chars.saturating_add(target_chars / 10));
    let mut char_count = 0usize;
    let mut idx = 0usize;
    while char_count < target_chars {
        if !result.is_empty() {
            result.push_str("\n\n");
            char_count += 2;
        }
        let para = FILLER_PARAGRAPHS[order[idx % order.len()]];
        char_count += para.chars().count();
        result.push_str(para);
        idx += 1;
    }
    result
}

pub fn build_needle_prompt(
    context_size: u32,
    needles: &[NeedleSpec],
    question: &str,
    seed: u64,
) -> String {
    let question_tokens = estimate_tokens(question);
    let needle_tokens: u32 = needles.iter().map(|n| estimate_tokens(&n.text)).sum();
    let filler_target = ((context_size as f64 * FILLER_FILL_RATIO) as u32)
        .saturating_sub(question_tokens + needle_tokens)
        .max(64);

    let filler = synthesize_filler(filler_target, seed);
    let mut text = filler;

    let mut sorted: Vec<&NeedleSpec> = needles.iter().collect();
    sorted.sort_by(|a, b| b.depth_pct.cmp(&a.depth_pct));

    for needle in sorted {
        let total_chars = text.chars().count();
        let pos = ((total_chars as f64) * needle.depth_pct as f64 / 100.0).round() as usize;
        let pos = pos.min(total_chars);
        let before: String = text.chars().take(pos).collect();
        let after: String = text.chars().skip(pos).collect();
        text = format!("{before}\n\n{}\n\n{after}", needle.text);
    }

    format!("{text}\n\n{question}")
}

pub fn build_prompt_for_template(tpl: &EvalTemplate) -> Result<String, String> {
    match tpl.kind.as_str() {
        "direct" => tpl
            .prompt
            .clone()
            .ok_or_else(|| format!("template {} missing prompt", tpl.id)),
        "needle" => {
            let needles = tpl
                .needles
                .as_ref()
                .ok_or_else(|| format!("template {} missing needles", tpl.id))?;
            let question = tpl
                .question
                .as_ref()
                .ok_or_else(|| format!("template {} missing question", tpl.id))?;
            let seed = template_seed(&tpl.id, tpl.context_size);
            Ok(build_needle_prompt(tpl.context_size, needles, question, seed))
        }
        other => Err(format!("unknown template kind: {other}")),
    }
}

fn template_seed(id: &str, context_size: u32) -> u64 {
    let mut hash: u64 = context_size as u64;
    for b in id.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(b as u64);
    }
    hash
}

pub fn excerpt_output(output: &str) -> String {
    let chars: Vec<char> = output.chars().collect();
    if chars.len() <= MAX_OUTPUT_EXCERPT_CHARS {
        return output.to_string();
    }
    chars
        .into_iter()
        .take(MAX_OUTPUT_EXCERPT_CHARS)
        .collect()
}

const HISTORY_GROUP_GAP_MS: i64 = 30_000;

pub fn group_eval_template_history(rows: Vec<EvalTemplateResultRow>) -> Vec<EvalTemplateHistoryEntry> {
    if rows.is_empty() {
        return Vec::new();
    }

    let set = find_project_root()
        .and_then(|root| load_template_set(&default_template_set_path(&root)).ok());

    let mut groups: Vec<EvalTemplateHistoryEntry> = Vec::new();

    for row in rows {
        let ts = row.created_at.parse::<i64>().unwrap_or(0);
        let context_size = row.context_size as u32;
        let description = set
            .as_ref()
            .and_then(|s| s.templates.iter().find(|t| t.id == row.template_id))
            .map(|t| t.description.clone())
            .unwrap_or_else(|| row.template_id.clone());

        let item = EvalTemplateItemResult {
            template_id: row.template_id.clone(),
            description,
            score: row.score as u32,
            output: row.output_excerpt.clone(),
            output_excerpt: row.output_excerpt,
            elapsed_ms: row.elapsed_ms as u64,
        };

        let merge = groups.last_mut().filter(|g| {
            let g_ts = g.created_at.parse::<i64>().unwrap_or(0);
            let gap = (ts - g_ts).unsigned_abs() as i64;
            g.context_size == context_size && gap <= HISTORY_GROUP_GAP_MS
        });

        if let Some(group) = merge {
            group.items.push(item);
            let sum: u32 = group.items.iter().map(|i| i.score).sum();
            group.total_score = sum / group.items.len() as u32;
        } else {
            groups.push(EvalTemplateHistoryEntry {
                context_size,
                total_score: item.score,
                created_at: row.created_at.clone(),
                items: vec![item],
            });
        }
    }

    for group in &mut groups {
        if group.items.len() > 1 {
            let sum: u32 = group.items.iter().map(|i| i.score).sum();
            group.total_score = sum / group.items.len() as u32;
        }
    }

    groups
}

pub fn measurable_context_sizes(profile: &ModelProfile, set: &EvalTemplateSet) -> Vec<u32> {
    let available = available_context_sizes(set);
    available
        .into_iter()
        .filter(|s| *s >= profile.context.min && *s <= profile.context.max)
        .collect()
}

/// 프로파일이 없으면 빈 목록(기록 전용 모델 폴백).
pub fn measurable_context_sizes_for_profile_id(
    profiles_dir: &std::path::Path,
    profile_id: &str,
    set: &EvalTemplateSet,
) -> Result<Vec<u32>, String> {
    let profile = match crate::profile::load_profile_by_id(profiles_dir, profile_id) {
        Ok(p) => p,
        Err(crate::profile::ProfileError::NotFound { .. }) => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    Ok(measurable_context_sizes(&profile, set))
}

fn chat_timeout_for_context(context_size: u32) -> Duration {
    if context_size >= 131072 {
        Duration::from_secs(1800)
    } else if context_size >= 65536 {
        Duration::from_secs(900)
    } else if context_size >= 16384 {
        Duration::from_secs(300)
    } else {
        Duration::from_secs(120)
    }
}

pub async fn run_template_eval(
    db: &Database,
    profile: ModelProfile,
    context_size: u32,
    template_set: &EvalTemplateSet,
    python_dir: PathBuf,
    child_spec: Option<ChildSpec>,
    port: Option<u16>,
    progress_tx: Option<broadcast::Sender<CoreEvent>>,
) -> Result<EvalTemplateSummary, String> {
    ensure_llm_profile(&profile)?;

    let templates: Vec<EvalTemplate> = template_set
        .templates
        .iter()
        .filter(|t| t.context_size == context_size)
        .cloned()
        .collect();

    if templates.is_empty() {
        return Err(format!("no templates for context {context_size}"));
    }
    if templates.len() != 3 {
        return Err(format!(
            "expected 3 templates for context {context_size}, got {}",
            templates.len()
        ));
    }

    let model_id = db.upsert_model(&profile).map_err(|e| e.to_string())?;
    let params_json = serde_json::json!({
        "eval_template": "context_templates_ko",
        "context": context_size,
        "templates": templates.iter().map(|t| &t.id).collect::<Vec<_>>(),
    });
    let run_id = db
        .insert_run(
            model_id,
            "eval_template",
            None,
            Some(context_size),
            &params_json.to_string(),
        )
        .map_err(|e| e.to_string())?;

    let handle = LifecycleHandle::spawn();
    let (recorder_arc, collector_task) = spawn_sample_collector(&handle.event_tx);

    let start = StartParams {
        profile: profile.clone(),
        context: context_size,
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

    let load_deadline = Duration::from_secs(profile.load_timeout_sec.saturating_add(60));
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
        .timeout(chat_timeout_for_context(context_size))
        .build()
        .map_err(|e| e.to_string())?;

    let _ = handle.command_tx.send(Command::BeginWork).await;

    let mut item_results = Vec::new();
    let total = templates.len();

    for (idx, tpl) in templates.iter().enumerate() {
        if let Some(tx) = &progress_tx {
            let _ = tx.send(CoreEvent::Log {
                level: "info".into(),
                message: format!(
                    "eval_template_start:{}:{}/{}",
                    tpl.id,
                    idx + 1,
                    total
                ),
            });
        }

        let prompt = build_prompt_for_template(tpl)?;
        let started = Instant::now();

        let actual = client::chat_completion(
            &http,
            server_port,
            &profile.id,
            &prompt,
            EVAL_MAX_TOKENS,
            0.0,
        )
        .await
        .unwrap_or_else(|e| format!("[error: {e}]"));

        let elapsed_ms = started.elapsed().as_millis() as u64;
        let score = score_keywords(
            &actual,
            &tpl.scoring.keywords,
            &tpl.scoring.weights,
        );
        let output_excerpt = excerpt_output(&actual);

        db.insert_eval_template_result(
            &profile.id,
            context_size,
            &tpl.id,
            score,
            &output_excerpt,
            elapsed_ms,
        )
        .map_err(|e| e.to_string())?;

        if let Some(tx) = &progress_tx {
            let _ = tx.send(CoreEvent::Log {
                level: "info".into(),
                message: format!("eval_template_done:{}:{}", tpl.id, score),
            });
        }

        item_results.push(EvalTemplateItemResult {
            template_id: tpl.id.clone(),
            description: tpl.description.clone(),
            score,
            output: actual,
            output_excerpt,
            elapsed_ms,
        });
    }

    let _ = handle.command_tx.send(Command::EndWork).await;
    let _ = handle.command_tx.send(Command::Stop).await;
    handle.wait_for_state(LifecycleState::Idle).await;

    let recorder = take_recorder(recorder_arc, collector_task).await;
    db.insert_samples(run_id, recorder.samples())
        .map_err(|e| e.to_string())?;

    let total_score = if item_results.is_empty() {
        0
    } else {
        item_results.iter().map(|i| i.score).sum::<u32>() / item_results.len() as u32
    };

    let quality = total_score as f64 / 100.0;
    db.insert_quality_result(run_id, quality)
        .map_err(|e| e.to_string())?;
    db.finish_run(run_id, "completed", None)
        .map_err(|e| e.to_string())?;

    Ok(EvalTemplateSummary {
        profile_id: profile.id,
        context_size,
        total_score,
        items: item_results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{eval_sets_dir, find_project_root};

    #[test]
    fn estimate_tokens_korean() {
        let text = "가".repeat(250);
        let tokens = estimate_tokens(&text);
        assert!(tokens >= 95 && tokens <= 105, "tokens={tokens}");
    }

    #[test]
    fn filler_within_target_tolerance() {
        for &target in &[1024u32, 4096, 16384, 65536, 524288, 1_048_576] {
            let filler = synthesize_filler(target, 42);
            let actual = estimate_tokens(&filler);
            let low = (target as f64 * 0.90) as u32;
            let high = (target as f64 * 1.10) as u32;
            assert!(
                actual >= low && actual <= high,
                "target={target} actual={actual}"
            );
        }
    }

    #[test]
    fn filler_1m_performance_and_capacity() {
        let start = Instant::now();
        let filler = synthesize_filler(1_048_576, 7);
        let elapsed = start.elapsed();
        assert!(
            filler.len() >= 2_000_000,
            "filler byte len={}",
            filler.len()
        );
        let actual = estimate_tokens(&filler);
        let low = (1_048_576f64 * 0.90) as u32;
        let high = (1_048_576f64 * 1.10) as u32;
        assert!(actual >= low && actual <= high, "tokens={actual}");
        assert!(
            elapsed.as_secs() < 30,
            "1M filler synthesis took {:?}",
            elapsed
        );
    }

    #[test]
    fn needle_insertion_positions() {
        let needles = vec![
            NeedleSpec {
                text: "NEEDLE_START".into(),
                depth_pct: 10,
            },
            NeedleSpec {
                text: "NEEDLE_MID".into(),
                depth_pct: 50,
            },
            NeedleSpec {
                text: "NEEDLE_END".into(),
                depth_pct: 90,
            },
        ];
        let prompt = build_needle_prompt(4096, &needles, "질문?", 99);
        assert!(prompt.contains("NEEDLE_START"));
        assert!(prompt.contains("NEEDLE_MID"));
        assert!(prompt.contains("NEEDLE_END"));
        assert!(prompt.ends_with("질문?"));

        let start_pos = prompt.find("NEEDLE_START").unwrap();
        let mid_pos = prompt.find("NEEDLE_MID").unwrap();
        let end_pos = prompt.find("NEEDLE_END").unwrap();
        assert!(start_pos < mid_pos);
        assert!(mid_pos < end_pos);
    }

    #[test]
    fn score_keywords_partial_and_weights() {
        let score = score_keywords(
            "답: 서울입니다",
            &["서울".into(), "부산".into()],
            &[70, 30],
        );
        assert_eq!(score, 70);

        let full = score_keywords(
            "서울과 부산",
            &["서울".into(), "부산".into()],
            &[50, 50],
        );
        assert_eq!(full, 100);
    }

    #[test]
    fn normalize_case_and_spaces() {
        assert_eq!(normalize_for_match("  Hello   World "), "hello world");
        assert!(normalize_for_match("SaFfRoN-7214")
            .contains(&normalize_for_match("saffron-7214")));
    }

    #[test]
    fn load_all_27_templates_valid() {
        let root = find_project_root().expect("project root");
        let path = eval_sets_dir(&root).join("context_templates_ko.json");
        let set = load_template_set(&path).expect("load templates");
        assert_eq!(set.templates.len(), 27);

        for size in STANDARD_CONTEXT_SIZES {
            let group = templates_for_context(&set, size);
            assert_eq!(
                group.len(),
                3,
                "context {size} should have 3 templates"
            );
        }
    }

    #[test]
    fn direct_template_builds_prompt() {
        let tpl = EvalTemplate {
            id: "test".into(),
            context_size: 1024,
            kind: "direct".into(),
            description: String::new(),
            prompt: Some("hello".into()),
            needles: None,
            question: None,
            scoring: KeywordScoring {
                r#type: "keyword".into(),
                keywords: vec!["hi".into()],
                weights: vec![100],
            },
        };
        assert_eq!(build_prompt_for_template(&tpl).unwrap(), "hello");
    }

    #[test]
    fn measurable_contexts_missing_profile_returns_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let profiles = dir.path().join("profiles");
        std::fs::create_dir_all(&profiles).expect("profiles dir");
        let set = EvalTemplateSet {
            schema_version: 1,
            templates: vec![EvalTemplate {
                id: "ctx4k-1".into(),
                context_size: 4096,
                kind: "direct".into(),
                description: String::new(),
                prompt: Some("q".into()),
                needles: None,
                question: None,
                scoring: KeywordScoring {
                    r#type: "keyword".into(),
                    keywords: vec!["a".into()],
                    weights: vec![100],
                },
            }],
        };
        let out = measurable_context_sizes_for_profile_id(
            &profiles,
            "mlx-community/missing-model",
            &set,
        )
        .expect("no error for missing profile");
        assert!(out.is_empty());
    }
}