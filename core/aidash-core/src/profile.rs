//! 모델 프로파일 JSON 로드·검증·HF config.json 파싱으로 초안 생성
//!
//! IN: 프로파일 파일 경로, HF repo id, 로컬 모델 경로
//! OUT: `ModelProfile` 구조체, 검증 오류

use std::fmt;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::{Deserialize, Serialize};

static HF_REPO_ID_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();

pub fn hf_repo_id_regex() -> &'static Regex {
    HF_REPO_ID_RE.get_or_init(|| {
        Regex::new(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$").expect("valid hf repo id regex")
    })
}

pub fn is_valid_hf_repo_id(id: &str) -> bool {
    hf_repo_id_regex().is_match(id)
}

pub fn hf_url(profile_id: &str, source_kind: Option<&str>) -> Option<String> {
    if source_kind == Some("local") {
        return None;
    }
    if is_valid_hf_repo_id(profile_id) {
        Some(format!("https://huggingface.co/{profile_id}"))
    } else {
        None
    }
}

pub fn model_link_label(profile_id: &str, source_kind: Option<&str>) -> String {
    if source_kind == Some("local") {
        format!("{profile_id} (local)")
    } else if let Some(url) = hf_url(profile_id, source_kind) {
        format!("{profile_id}\n  {url}")
    } else {
        profile_id.to_string()
    }
}

pub fn source_kind_from_profile_json(profile_json: &str) -> Option<String> {
    serde_json::from_str::<ModelProfile>(profile_json)
        .ok()
        .map(|p| p.source.kind)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    pub schema_version: u32,
    pub id: String,
    pub display_name: String,
    pub source: ProfileSource,
    pub model_type: String,
    pub backend: String,
    pub io: ProfileIo,
    pub context: ProfileContext,
    pub default_params: serde_json::Value,
    pub quantization: Option<String>,
    pub load_timeout_sec: u64,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSource {
    pub kind: String,
    #[serde(default)]
    pub hf_repo: String,
    #[serde(default)]
    pub hf_file: String,
    #[serde(default)]
    pub local_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileIo {
    pub input: Vec<String>,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileContext {
    pub min: u32,
    pub max: u32,
    pub default: u32,
    #[serde(default)]
    pub sweep_steps: Vec<u32>,
}

#[derive(Debug)]
pub enum ProfileError {
    Io(std::io::Error),
    Parse(serde_json::Error),
    Validation(String),
    NotFound { id: String, dir: PathBuf },
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfileError::Io(e) => write!(f, "io error: {e}"),
            ProfileError::Parse(e) => write!(f, "json parse error: {e}"),
            ProfileError::Validation(msg) => write!(f, "validation error: {msg}"),
            ProfileError::NotFound { id, dir } => {
                write!(f, "profile '{id}' not found under {}", dir.display())
            }
        }
    }
}

impl std::error::Error for ProfileError {}

impl From<std::io::Error> for ProfileError {
    fn from(value: std::io::Error) -> Self {
        ProfileError::Io(value)
    }
}

impl From<serde_json::Error> for ProfileError {
    fn from(value: serde_json::Error) -> Self {
        ProfileError::Parse(value)
    }
}

/// context.max까지 2배수 단계 생성. max가 2^n이 아니면 마지막에 max 포함.
pub fn generate_sweep_steps(context_max: u32) -> Vec<u32> {
    let mut steps = Vec::new();
    let mut step = 1024u32;
    while step <= context_max {
        steps.push(step);
        if step == context_max {
            break;
        }
        let next = step.saturating_mul(2);
        if next > context_max {
            if steps.last().copied() != Some(context_max) {
                steps.push(context_max);
            }
            break;
        }
        step = next;
    }
    steps
}

/// 기존 프로파일 sweep_steps를 context.max까지 메모리상 확장.
pub fn extend_sweep_steps(steps: &[u32], context_max: u32) -> Vec<u32> {
    let expected = generate_sweep_steps(context_max);
    if steps.is_empty() {
        return expected;
    }
    if steps.last().copied().unwrap_or(0) >= context_max {
        return steps.to_vec();
    }
    let mut merged = steps.to_vec();
    for s in expected {
        if !merged.contains(&s) {
            merged.push(s);
        }
    }
    merged.sort_unstable();
    merged.dedup();
    merged
}

pub fn load_profile_file(path: &Path) -> Result<ModelProfile, ProfileError> {
    let contents = std::fs::read_to_string(path)?;
    let mut profile: ModelProfile = serde_json::from_str(&contents)?;
    validate_profile(&profile)?;
    profile.context.sweep_steps =
        extend_sweep_steps(&profile.context.sweep_steps, profile.context.max);
    Ok(profile)
}

pub fn load_profile_by_id(profiles_dir: &Path, id: &str) -> Result<ModelProfile, ProfileError> {
    if !profiles_dir.is_dir() {
        return Err(ProfileError::Validation(format!(
            "profiles directory not found: {}",
            profiles_dir.display()
        )));
    }

    for entry in std::fs::read_dir(profiles_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let profile = load_profile_file(&path)?;
        if profile.id == id {
            return Ok(profile);
        }
    }

    Err(ProfileError::NotFound {
        id: id.to_string(),
        dir: profiles_dir.to_path_buf(),
    })
}

pub fn validate_profile(profile: &ModelProfile) -> Result<(), ProfileError> {
    if profile.schema_version != 1 {
        return Err(ProfileError::Validation(format!(
            "unsupported schema_version: {}",
            profile.schema_version
        )));
    }
    if profile.id.trim().is_empty() {
        return Err(ProfileError::Validation("id must not be empty".into()));
    }

    if profile.source.kind != "local" && !is_valid_hf_repo_id(&profile.id) {
        return Err(ProfileError::Validation(format!(
            "id must match HuggingFace repo id format (org/name): {}",
            profile.id
        )));
    }
    if profile.display_name.trim().is_empty() {
        return Err(ProfileError::Validation(
            "display_name must not be empty".into(),
        ));
    }

    match profile.source.kind.as_str() {
        "hf" => {
            if profile.source.hf_repo.trim().is_empty() {
                return Err(ProfileError::Validation(
                    "source.hf_repo required when kind=hf".into(),
                ));
            }
        }
        "local" => {
            if profile.source.local_path.trim().is_empty() {
                return Err(ProfileError::Validation(
                    "source.local_path required when kind=local".into(),
                ));
            }
        }
        other => {
            return Err(ProfileError::Validation(format!(
                "unsupported source.kind: {other}"
            )));
        }
    }

    const BACKENDS: &[&str] = &[
        "vllm_mlx",
        "mlx_lm",
        "mlx_vlm",
        "llama_cpp",
        "transformers",
        "mflux",
        "mlx_whisper",
        "mlx_audio",
    ];
    if !BACKENDS.contains(&profile.backend.as_str()) {
        return Err(ProfileError::Validation(format!(
            "unsupported backend: {}",
            profile.backend
        )));
    }

    if profile.context.min == 0 || profile.context.max < profile.context.min {
        return Err(ProfileError::Validation("invalid context bounds".into()));
    }
    if profile.load_timeout_sec == 0 {
        return Err(ProfileError::Validation(
            "load_timeout_sec must be > 0".into(),
        ));
    }

    Ok(())
}

impl ModelProfile {
    pub fn model_path(&self) -> &str {
        match self.source.kind.as_str() {
            "local" => &self.source.local_path,
            _ => &self.source.hf_repo,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileListRow {
    pub id: String,
    pub backend: String,
    pub model_type: String,
    pub context_default: u32,
    pub filename: String,
}

#[derive(Debug)]
pub enum ValidationIssue {
    Field { field: String, message: String },
}

impl fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationIssue::Field { field, message } => write!(f, "{field}: {message}"),
        }
    }
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"))
}

pub fn hf_cache_dir() -> PathBuf {
    home_dir().join(".cache").join("huggingface").join("hub")
}

pub fn profile_filename_from_id(id: &str) -> String {
    format!("{}.json", id.replace('/', "-").to_lowercase())
}

pub fn find_config_json_in_cache(repo_id: &str) -> Result<PathBuf, ProfileError> {
    let dir_name = format!("models--{}", repo_id.replace('/', "--"));
    let model_dir = hf_cache_dir().join(dir_name);
    if !model_dir.is_dir() {
        return Err(ProfileError::Validation(format!(
            "모델이 캐시에 없음: {repo_id}. \
             HuggingFace에서 먼저 다운로드하세요 (네트워크 호출 없음 — 로컬 캐시만 검색)."
        )));
    }
    let snapshots = model_dir.join("snapshots");
    if !snapshots.is_dir() {
        return Err(ProfileError::Validation(format!(
            "모델이 캐시에 없음: {repo_id} (snapshots 디렉터리 없음)"
        )));
    }
    for entry in std::fs::read_dir(&snapshots)? {
        let entry = entry?;
        let config = entry.path().join("config.json");
        if config.is_file() {
            return Ok(config);
        }
    }
    Err(ProfileError::Validation(format!(
        "모델이 캐시에 없음: {repo_id} (config.json 없음)"
    )))
}

pub fn find_config_json_local(path: &Path) -> Result<PathBuf, ProfileError> {
    let config = path.join("config.json");
    if config.is_file() {
        return Ok(config);
    }
    Err(ProfileError::Validation(format!(
        "config.json not found in {}",
        path.display()
    )))
}

fn infer_model_type(config: &serde_json::Value, repo_id: &str) -> String {
    let model_type = config
        .get("model_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    let archs: Vec<String> = config
        .get("architectures")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                .collect()
        })
        .unwrap_or_default();

    let repo_lower = repo_id.to_lowercase();

    if model_type.contains("whisper")
        || archs.iter().any(|a| a.contains("whisper"))
        || repo_lower.contains("whisper")
        || model_type.contains("sensevoice")
        || model_type.contains("paraformer")
    {
        return "asr".into();
    }
    if model_type.contains("tts")
        || repo_lower.contains("tts")
        || repo_lower.contains("speech")
    {
        return "tts".into();
    }
    if model_type.contains("flux")
        || repo_lower.contains("flux")
        || repo_lower.contains("image")
        || model_type.contains("mmdit")
    {
        return "image_gen".into();
    }
    if model_type.contains("vl")
        || model_type.contains("vision")
        || config.get("vision_config").is_some()
        || archs.iter().any(|a| a.contains("vl") || a.contains("vision"))
        || repo_lower.contains("-vl-")
        || repo_lower.contains("vlm")
        || repo_lower.contains("vision")
    {
        return "multimodal".into();
    }
    if archs.iter().any(|a| a.contains("forcausallm")) || model_type.contains("llama")
    {
        return "llm".into();
    }
    if !model_type.is_empty() {
        return "llm".into();
    }
    "llm".into()
}

pub const TASK_LLM: &str = "llm";
pub const TASK_MULTIMODAL: &str = "multimodal";
pub const TASK_ASR: &str = "asr";
pub const TASK_TTS: &str = "tts";
pub const TASK_IMAGE_GEN: &str = "image_gen";
pub const TASK_VIDEO_GEN: &str = "video_gen";

pub const ALL_TASKS: &[&str] = &[
    TASK_LLM,
    TASK_MULTIMODAL,
    TASK_ASR,
    TASK_TTS,
    TASK_IMAGE_GEN,
    TASK_VIDEO_GEN,
];

pub fn task_label_ko(task: &str) -> &'static str {
    match task {
        TASK_LLM => "텍스트 생성",
        TASK_MULTIMODAL => "멀티모달(이미지+텍스트)",
        TASK_ASR => "음성→텍스트(STT)",
        TASK_TTS => "텍스트→음성(TTS)",
        TASK_IMAGE_GEN => "이미지 생성",
        TASK_VIDEO_GEN => "동영상 생성",
        _ => "알 수 없음",
    }
}

/// 리더보드용 짧은 태스크 뱃지. llm은 None(생략).
pub fn task_badge_short(task: &str) -> Option<&'static str> {
    match task {
        TASK_ASR => Some("STT"),
        TASK_TTS => Some("TTS"),
        TASK_IMAGE_GEN => Some("이미지"),
        TASK_MULTIMODAL => Some("멀티모달"),
        TASK_VIDEO_GEN => Some("동영상"),
        _ => None,
    }
}

pub fn is_valid_task(task: &str) -> bool {
    ALL_TASKS.contains(&task)
}

pub fn infer_backend(model_type: &str, repo_id: &str) -> String {
    let repo_lower = repo_id.to_lowercase();
    match model_type {
        "asr" => "mlx_whisper".into(),
        "tts" => "mlx_audio".into(),
        "image_gen" => "mflux".into(),
        "multimodal" => "mlx_vlm".into(),
        _ if repo_lower.contains("gguf") => "llama_cpp".into(),
        _ => "vllm_mlx".into(),
    }
}

fn infer_quantization(config: &serde_json::Value, repo_id: &str) -> Option<String> {
    if let Some(bits) = config
        .pointer("/quantization/bits")
        .or_else(|| config.pointer("/quantization_config/bits"))
        .and_then(|v| v.as_u64())
    {
        return Some(format!("{bits}bit"));
    }
    let repo_lower = repo_id.to_lowercase();
    if repo_lower.contains("4bit") || repo_lower.contains("-4b") {
        return Some("4bit".into());
    }
    if repo_lower.contains("8bit") {
        return Some("8bit".into());
    }
    if repo_lower.contains("bf16") || repo_lower.contains("fp16") {
        return Some("fp16".into());
    }
    if repo_lower.contains("gguf") {
        return Some("4bit".into());
    }
    None
}

fn infer_context_max(config: &serde_json::Value) -> u32 {
    config
        .get("max_position_embeddings")
        .and_then(|v| v.as_u64())
        .or_else(|| {
            config
                .get("text_config")
                .and_then(|tc| tc.get("max_position_embeddings"))
                .and_then(|v| v.as_u64())
        })
        .map(|v| v.min(u32::MAX as u64) as u32)
        .unwrap_or(4096)
}

fn infer_display_name(repo_id: &str) -> String {
    let name = repo_id.split('/').nth(1).unwrap_or(repo_id);
    name.replace('-', " ")
        .replace('_', " ")
        .split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn draft_from_config(
    config_path: &Path,
    id: &str,
    source_kind: &str,
    local_path: Option<&str>,
) -> Result<ModelProfile, ProfileError> {
    let contents = std::fs::read_to_string(config_path)?;
    let config: serde_json::Value = serde_json::from_str(&contents)?;

    let model_type = infer_model_type(&config, id);
    let backend = infer_backend(&model_type, id);
    let context_max = infer_context_max(&config);
    let context_default = context_max.min(4096).max(512);

    let (input, output) = match model_type.as_str() {
        "asr" => (vec!["audio".into()], "text".into()),
        "tts" => (vec!["text".into()], "audio".into()),
        "image_gen" => (vec!["text".into()], "image".into()),
        "multimodal" => (vec!["chat".into(), "image".into()], "text".into()),
        _ => (vec!["chat".into()], "text".into()),
    };

    let source = if source_kind == "local" {
        ProfileSource {
            kind: "local".into(),
            hf_repo: String::new(),
            hf_file: String::new(),
            local_path: local_path.unwrap_or("").into(),
        }
    } else {
        ProfileSource {
            kind: "hf".into(),
            hf_repo: id.into(),
            hf_file: String::new(),
            local_path: String::new(),
        }
    };

    Ok(ModelProfile {
        schema_version: 1,
        id: id.into(),
        display_name: infer_display_name(id),
        source,
        model_type,
        backend,
        io: ProfileIo { input, output },
        context: ProfileContext {
            min: 512,
            max: context_max,
            default: context_default,
            sweep_steps: generate_sweep_steps(context_max),
        },
        default_params: serde_json::json!({
            "max_tokens": 512,
            "temperature": 0.7,
            "top_p": 0.95
        }),
        quantization: infer_quantization(&config, id),
        load_timeout_sec: 600,
        notes: "auto-generated draft — review and edit before use".into(),
    })
}

pub fn generate_profile_hf(profiles_dir: &Path, repo_id: &str) -> Result<PathBuf, ProfileError> {
    if !is_valid_hf_repo_id(repo_id) {
        return Err(ProfileError::Validation(format!(
            "invalid HF repo id format: {repo_id}"
        )));
    }
    let config_path = find_config_json_in_cache(repo_id)?;
    let profile = draft_from_config(&config_path, repo_id, "hf", None)?;
    write_profile_draft(profiles_dir, &profile)
}

pub fn generate_profile_local(
    profiles_dir: &Path,
    local_path: &Path,
) -> Result<PathBuf, ProfileError> {
    if !local_path.is_dir() {
        return Err(ProfileError::Validation(format!(
            "local path is not a directory: {}",
            local_path.display()
        )));
    }
    let config_path = find_config_json_local(local_path)?;
    let id = local_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("local/model")
        .to_string();
    let profile = draft_from_config(
        &config_path,
        &format!("local/{id}"),
        "local",
        Some(&local_path.display().to_string()),
    )?;
    write_profile_draft(profiles_dir, &profile)
}

fn write_profile_draft(profiles_dir: &Path, profile: &ModelProfile) -> Result<PathBuf, ProfileError> {
    std::fs::create_dir_all(profiles_dir)?;
    let filename = profile_filename_from_id(&profile.id);
    let out_path = profiles_dir.join(&filename);
    if out_path.exists() {
        return Err(ProfileError::Validation(format!(
            "profile already exists: {} (덮어쓰지 않음)",
            out_path.display()
        )));
    }
    let json = serde_json::to_string_pretty(profile)?;
    std::fs::write(&out_path, json)?;
    Ok(out_path)
}

pub fn list_profiles(profiles_dir: &Path) -> Result<Vec<ProfileListRow>, ProfileError> {
    if !profiles_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut rows = Vec::new();
    for entry in std::fs::read_dir(profiles_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown.json")
            .to_string();
        let contents = std::fs::read_to_string(&path)?;
        let profile: ModelProfile = serde_json::from_str(&contents)?;
        rows.push(ProfileListRow {
            id: profile.id,
            backend: profile.backend,
            model_type: profile.model_type,
            context_default: profile.context.default,
            filename,
        });
    }
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(rows)
}

fn io_for_task(task: &str) -> (Vec<String>, String) {
    match task {
        TASK_ASR => (vec!["audio".into()], "text".into()),
        TASK_TTS => (vec!["text".into()], "audio".into()),
        TASK_IMAGE_GEN | TASK_VIDEO_GEN => (vec!["text".into()], "image".into()),
        TASK_MULTIMODAL => (vec!["chat".into(), "image".into()], "text".into()),
        _ => (vec!["chat".into()], "text".into()),
    }
}

pub fn set_profile_task(
    profiles_dir: &Path,
    profile_id: &str,
    task: &str,
    adjust_backend: bool,
) -> Result<ModelProfile, ProfileError> {
    if !is_valid_task(task) {
        return Err(ProfileError::Validation(format!(
            "unsupported task: {task}"
        )));
    }

    let mut profile = load_profile_by_id(profiles_dir, profile_id)?;
    profile.model_type = task.to_string();
    if adjust_backend {
        profile.backend = infer_backend(task, &profile.id);
    }
    let (input, output) = io_for_task(task);
    profile.io = ProfileIo { input, output };

    let filename = profile_filename_from_id(profile_id);
    let path = profiles_dir.join(&filename);
    let json = serde_json::to_string_pretty(&profile)?;
    std::fs::write(&path, json)?;
    Ok(profile)
}

pub fn model_type_from_profile_json(profile_json: &str) -> Option<String> {
    serde_json::from_str::<ModelProfile>(profile_json)
        .ok()
        .map(|p| p.model_type)
}

pub fn validate_profile_file(path: &Path) -> Result<(), Vec<ValidationIssue>> {
    let contents = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return Err(vec![ValidationIssue::Field {
                field: "file".into(),
                message: e.to_string(),
            }]);
        }
    };
    let profile: ModelProfile = match serde_json::from_str(&contents) {
        Ok(p) => p,
        Err(e) => {
            return Err(vec![ValidationIssue::Field {
                field: "json".into(),
                message: e.to_string(),
            }]);
        }
    };
    if let Err(e) = validate_profile(&profile) {
        return Err(vec![ValidationIssue::Field {
            field: "profile".into(),
            message: e.to_string(),
        }]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hf_repo_id_valid_cases() {
        for id in [
            "org/model",
            "mlx-community/Qwen3-4B-Instruct-2507-4bit",
            "user_name/model.name-v2",
        ] {
            assert!(is_valid_hf_repo_id(id), "{id} should be valid");
        }
    }

    #[test]
    fn hf_repo_id_invalid_cases() {
        for id in [
            "no-slash",
            "org/",
            "/model",
            "org/model/extra",
            "org with space/model",
        ] {
            assert!(!is_valid_hf_repo_id(id), "{id} should be invalid");
        }
    }

    #[test]
    fn hf_url_derivation() {
        assert_eq!(
            hf_url("mlx-community/Qwen3-4B", Some("hf")),
            Some("https://huggingface.co/mlx-community/Qwen3-4B".into())
        );
        assert_eq!(hf_url("local/model", Some("local")), None);
    }

    #[test]
    fn infer_model_type_heuristics() {
        let whisper = serde_json::json!({"model_type": "whisper"});
        assert_eq!(infer_model_type(&whisper, "org/model"), "asr");

        let llm = serde_json::json!({
            "architectures": ["Qwen3ForCausalLM"],
            "model_type": "qwen3"
        });
        assert_eq!(infer_model_type(&llm, "mlx-community/Qwen3-4B"), "llm");

        let vlm = serde_json::json!({"model_type": "llava"});
        assert_eq!(infer_model_type(&vlm, "org/vlm-model"), "multimodal");

        // vision_config가 있으면 이름에 단서가 없어도 멀티모달 (gemma-4 unified 케이스)
        let unified = serde_json::json!({
            "architectures": ["Gemma4UnifiedForConditionalGeneration"],
            "model_type": "gemma4_unified",
            "vision_config": {"hidden_size": 1152}
        });
        assert_eq!(
            infer_model_type(&unified, "mlx-community/gemma-4-12B-it-qat-4bit"),
            "multimodal"
        );
    }

    #[test]
    fn profile_filename_from_repo() {
        assert_eq!(
            profile_filename_from_id("mlx-community/Qwen3-4B-Instruct"),
            "mlx-community-qwen3-4b-instruct.json"
        );
    }

    #[test]
    fn set_profile_task_updates_model_type_and_io() {
        let dir = tempfile::tempdir().expect("tempdir");
        let profile = ModelProfile {
            schema_version: 1,
            id: "org/test-model".into(),
            display_name: "Test Model".into(),
            source: ProfileSource {
                kind: "hf".into(),
                hf_repo: "org/test-model".into(),
                hf_file: String::new(),
                local_path: String::new(),
            },
            model_type: TASK_LLM.into(),
            backend: "vllm_mlx".into(),
            io: ProfileIo {
                input: vec!["chat".into()],
                output: "text".into(),
            },
            context: ProfileContext {
                min: 512,
                max: 4096,
                default: 4096,
                sweep_steps: vec![4096],
            },
            default_params: serde_json::json!({}),
            quantization: None,
            load_timeout_sec: 600,
            notes: String::new(),
        };
        write_profile_draft(dir.path(), &profile).expect("write draft");

        let updated = set_profile_task(dir.path(), "org/test-model", TASK_ASR, true)
            .expect("set task");
        assert_eq!(updated.model_type, TASK_ASR);
        assert_eq!(updated.backend, "mlx_whisper");
        assert_eq!(updated.io.input, vec!["audio"]);
        assert_eq!(updated.io.output, "text");

        let reloaded = load_profile_by_id(dir.path(), "org/test-model").expect("reload");
        assert_eq!(reloaded.model_type, TASK_ASR);
    }

    #[test]
    fn generate_sweep_steps_up_to_max() {
        assert_eq!(
            generate_sweep_steps(32768),
            vec![1024, 2048, 4096, 8192, 16384, 32768]
        );
        assert_eq!(
            generate_sweep_steps(262144),
            vec![
                1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072, 262144
            ]
        );
    }

    #[test]
    fn generate_sweep_steps_non_power_of_two_max() {
        assert_eq!(
            generate_sweep_steps(200_000),
            vec![1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072, 200_000]
        );
    }

    #[test]
    fn extend_sweep_steps_migrates_legacy_profile() {
        let legacy = vec![1024, 2048, 4096, 8192, 16384, 32768];
        let extended = extend_sweep_steps(&legacy, 262144);
        assert!(extended.contains(&65536));
        assert!(extended.contains(&131072));
        assert!(extended.contains(&262144));
        assert_eq!(extended.len(), 9);
    }

    #[test]
    fn extend_sweep_steps_noop_when_complete() {
        let full = generate_sweep_steps(4096);
        assert_eq!(extend_sweep_steps(&full, 4096), full);
    }

    #[test]
    fn load_profile_extends_sweep_steps_to_context_max() {
        let root = std::env::var("AIDASH_ROOT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
            });
        let path = root.join("profiles/qwen3.6-35b-a3b-optiq-4bit.json");
        if !path.is_file() {
            return;
        }
        let profile = load_profile_file(&path).expect("load 35B profile");
        assert_eq!(profile.context.max, 262144);
        assert!(profile.context.sweep_steps.contains(&262144));
        assert!(profile.context.sweep_steps.contains(&65536));
    }

    #[test]
    fn task_label_ko_mappings() {
        assert_eq!(task_label_ko(TASK_LLM), "텍스트 생성");
        assert_eq!(task_label_ko(TASK_VIDEO_GEN), "동영상 생성");
        assert_eq!(task_badge_short(TASK_ASR), Some("STT"));
        assert_eq!(task_badge_short(TASK_LLM), None);
    }
}