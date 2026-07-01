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
    /// HF repo id of the paired speculative drafter (MTP assistant), if any.
    #[serde(default)]
    pub draft_model: Option<String>,
    /// `"autoregressive"` (default) or `"diffusion"` for block/masked diffusion LMs.
    #[serde(default = "default_generation_kind")]
    pub generation_kind: String,
    /// HF repo id of the base model this profile's LoRA adapter is trained on, if any.
    #[serde(default)]
    pub base_model: Option<String>,
}

fn default_generation_kind() -> String {
    GENERATION_KIND_AUTOREGRESSIVE.into()
}

pub const GENERATION_KIND_AUTOREGRESSIVE: &str = "autoregressive";
pub const GENERATION_KIND_DIFFUSION: &str = "diffusion";

pub fn is_diffusion_kind(generation_kind: &str) -> bool {
    generation_kind == GENERATION_KIND_DIFFUSION
}

pub fn generation_kind_from_profile_json(profile_json: &str) -> String {
    serde_json::from_str::<ModelProfile>(profile_json)
        .map(|p| p.generation_kind)
        .unwrap_or_else(|_| GENERATION_KIND_AUTOREGRESSIVE.into())
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

/// `config.json` 없이 `adapter_config.json`(PEFT LoRA)만 있는 캐시 항목을 찾는다.
pub fn find_adapter_config_json_in_cache(repo_id: &str) -> Result<PathBuf, ProfileError> {
    let dir_name = format!("models--{}", repo_id.replace('/', "--"));
    let model_dir = hf_cache_dir().join(dir_name);
    let snapshots = model_dir.join("snapshots");
    if !snapshots.is_dir() {
        return Err(ProfileError::Validation(format!(
            "모델이 캐시에 없음: {repo_id}"
        )));
    }
    for entry in std::fs::read_dir(&snapshots)? {
        let entry = entry?;
        let adapter_config = entry.path().join("adapter_config.json");
        if adapter_config.is_file() {
            return Ok(adapter_config);
        }
    }
    Err(ProfileError::Validation(format!(
        "adapter_config.json 없음: {repo_id}"
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

fn infer_generation_kind(config: &serde_json::Value) -> String {
    let model_type = config
        .get("model_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    if model_type.contains("diffusion") {
        return GENERATION_KIND_DIFFUSION.into();
    }
    let archs: Vec<String> = config
        .get("architectures")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                .collect()
        })
        .unwrap_or_default();
    if archs.iter().any(|a| a.contains("diffusion") || a.contains("blockdiffusion")) {
        return GENERATION_KIND_DIFFUSION.into();
    }
    GENERATION_KIND_AUTOREGRESSIVE.into()
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
pub const TASK_DRAFTER: &str = "drafter";

pub const DRAFTER_STANDALONE_ERROR: &str =
    "이 모델은 단독 실행용이 아닙니다 — 메인 모델의 보조로 사용됩니다";

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
        TASK_DRAFTER => "보조(drafter) 모델",
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

pub fn is_drafter_profile(profile: &ModelProfile) -> bool {
    profile.model_type == TASK_DRAFTER
}

pub fn ensure_runnable_profile(profile: &ModelProfile) -> Result<(), ProfileError> {
    if is_drafter_profile(profile) {
        return Err(ProfileError::Validation(DRAFTER_STANDALONE_ERROR.into()));
    }
    if let Some(ref base) = profile.base_model {
        if !repo_cached(base) {
            return Err(ProfileError::Validation(format!(
                "베이스 모델이 캐시에 없습니다: {base}. 먼저 다운로드하세요."
            )));
        }
    }
    Ok(())
}

const DRAFTER_MODEL_TYPES: &[&str] = &[
    "gemma4_assistant",
    "gemma4_unified_assistant",
    "qwen3_5_mtp",
    "deepseek_v4_mtp",
    "eagle3",
];

fn is_drafter_config(config: &serde_json::Value, repo_id: &str) -> bool {
    let repo_lower = repo_id.to_lowercase();
    if repo_lower.contains("-assistant-") {
        return true;
    }

    let model_type = config
        .get("model_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();
    if DRAFTER_MODEL_TYPES
        .iter()
        .any(|t| model_type.contains(t))
    {
        return true;
    }
    if model_type.contains("mtp") && model_type.contains("assistant") {
        return true;
    }

    let archs: Vec<String> = config
        .get("architectures")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                .collect()
        })
        .unwrap_or_default();
    if archs.iter().any(|a| {
        a.contains("assistant") || a.contains("mtp") || a.contains("drafter")
    }) {
        return true;
    }

    config
        .pointer("/speculative_config/draft_kind")
        .and_then(|v| v.as_str())
        .is_some()
        || config.get("draft").is_some()
}

/// Profile clone for adapter spawn; strips draft pairing when bench disables speculative mode.
pub fn profile_for_spawn(profile: &ModelProfile, use_draft: bool) -> ModelProfile {
    let mut p = profile.clone();
    if !use_draft {
        p.draft_model = None;
    }
    p
}

fn repo_cached(repo_id: &str) -> bool {
    find_config_json_in_cache(repo_id).is_ok()
}

/// Suggest a cached assistant drafter for a main model (e.g. gemma `-qat-assistant-` variant).
pub fn find_matching_assistant_drafter(main_repo_id: &str) -> Option<String> {
    if !is_valid_hf_repo_id(main_repo_id) {
        return None;
    }
    let (org, name) = main_repo_id.split_once('/')?;
    if name.contains("-assistant-") {
        return None;
    }

    if let Some(idx) = name.rfind("-qat-") {
        let prefix = &name[..idx + 5];
        let suffix = &name[idx + 5..];
        let candidate = format!("{org}/{prefix}assistant-{suffix}");
        if repo_cached(&candidate) {
            return Some(candidate);
        }
    }

    let cache_root = hf_cache_dir();
    let org_prefix = format!("models--{}--", org.replace('/', "--"));
    let name_lower = name.to_lowercase();
    let Ok(entries) = std::fs::read_dir(&cache_root) else {
        return None;
    };
    let mut matches = Vec::new();
    for entry in entries.flatten() {
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        if !dir_name.starts_with(&org_prefix) {
            continue;
        }
        let assistant_name = dir_name
            .strip_prefix(&org_prefix)
            .unwrap_or("")
            .replace("--", "-");
        let assistant_lower = assistant_name.to_lowercase();
        if !assistant_lower.contains("-assistant-") {
            continue;
        }
        if !assistant_lower.starts_with(&name_lower) {
            continue;
        }
        let repo_id = format!("{org}/{assistant_name}");
        if repo_cached(&repo_id) {
            matches.push(repo_id);
        }
    }
    matches.sort();
    matches.into_iter().next()
}

pub fn infer_backend(model_type: &str, repo_id: &str, is_mlx: bool) -> String {
    let repo_lower = repo_id.to_lowercase();
    match model_type {
        "asr" => "mlx_whisper".into(),
        "tts" => "mlx_audio".into(),
        "image_gen" => "mflux".into(),
        "multimodal" => "mlx_vlm".into(),
        _ if repo_lower.contains("gguf") => "llama_cpp".into(),
        _ if !is_mlx => "transformers".into(),
        _ => "vllm_mlx".into(),
    }
}

/// MLX 커뮤니티 변환본(양자화 export)인지 판별한다. 아니면 일반 PyTorch/safetensors
/// 체크포인트로 보고 `transformers`(MPS) 백엔드로 라우팅한다.
fn is_mlx_checkpoint(config: &serde_json::Value, repo_id: &str) -> bool {
    if config.get("quantization").is_some() {
        return true;
    }
    let repo_lower = repo_id.to_lowercase();
    repo_lower.contains("mlx-community") || repo_lower.contains("-mlx-") || repo_lower.ends_with("-mlx")
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

fn read_max_position_embeddings(config: &serde_json::Value) -> Option<u64> {
    config
        .get("max_position_embeddings")
        .and_then(|v| v.as_u64())
        .or_else(|| {
            config
                .pointer("/text_config/max_position_embeddings")
                .and_then(|v| v.as_u64())
        })
}

fn read_rope_scaling(config: &serde_json::Value) -> Option<&serde_json::Value> {
    config
        .get("rope_scaling")
        .or_else(|| config.pointer("/text_config/rope_scaling"))
}

fn rope_scaling_type(rope: &serde_json::Value) -> Option<&str> {
    rope.get("rope_type")
        .or_else(|| rope.get("type"))
        .and_then(|v| v.as_str())
}

fn is_known_rope_scaling_type(rope_type: &str) -> bool {
    matches!(
        rope_type.to_ascii_lowercase().as_str(),
        "yarn" | "linear" | "dynamic" | "longrope"
    )
}

fn clamp_u32_from_f64(v: f64) -> u32 {
    if !v.is_finite() || v <= 0.0 {
        return 0;
    }
    v.min(u32::MAX as f64) as u32
}

/// rope_scaling 규칙을 반영한 유효 최대 컨텍스트와 미해석 타입 메모.
pub fn infer_context_max_with_notes(config: &serde_json::Value) -> (u32, Option<String>) {
    let base = read_max_position_embeddings(config)
        .map(|v| v.min(u32::MAX as u64) as u32)
        .unwrap_or(4096);

    let Some(rope) = read_rope_scaling(config) else {
        return (base, None);
    };

    let original = rope
        .get("original_max_position_embeddings")
        .and_then(|v| v.as_u64())
        .map(|v| v.min(u32::MAX as u64) as u32);

    // Case 1: max가 original보다 크면 이미 확장된 값 — 그대로 사용
    if let Some(orig) = original {
        if base > orig {
            return (base, None);
        }
    }

    let factor = rope.get("factor").and_then(|v| v.as_f64()).unwrap_or(1.0);
    if factor <= 1.0 {
        return (base, None);
    }

    let rope_type = rope_scaling_type(rope).unwrap_or("");
    if !is_known_rope_scaling_type(rope_type) {
        let note = if rope_type.is_empty() {
            "rope_scaling 미해석: (type 없음)".into()
        } else {
            format!("rope_scaling 미해석: {rope_type}")
        };
        return (base, Some(note));
    }

    // Case 2: factor 적용 — original 없거나 max == original
    if original.is_none() || base == original.unwrap_or(0) {
        let scaled = clamp_u32_from_f64(base as f64 * factor);
        if scaled > 0 {
            return (scaled, None);
        }
    }

    (base, None)
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

    let underlying_type = infer_model_type(&config, id);
    let is_drafter = is_drafter_config(&config, id);
    let model_type = if is_drafter {
        TASK_DRAFTER.into()
    } else {
        underlying_type.clone()
    };
    let backend = infer_backend(
        if is_drafter {
            underlying_type.as_str()
        } else {
            model_type.as_str()
        },
        id,
        is_mlx_checkpoint(&config, id),
    );
    let (context_max, rope_note) = infer_context_max_with_notes(&config);
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

    let generation_kind = infer_generation_kind(&config);
    let mut default_params = serde_json::json!({
        "max_tokens": 512,
        "temperature": 0.7,
        "top_p": 0.95
    });
    if generation_kind == GENERATION_KIND_DIFFUSION {
        if let Some(gc) = config.get("generation_config").and_then(|v| v.as_object()) {
            if let Some(steps) = gc.get("max_denoising_steps") {
                default_params["max_denoising_steps"] = steps.clone();
            }
        }
    }

    let mut profile = ModelProfile {
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
        default_params,
        quantization: infer_quantization(&config, id),
        load_timeout_sec: 600,
        notes: {
            let mut notes =
                "auto-generated draft — review and edit before use".to_string();
            if let Some(note) = rope_note {
                notes.push_str("; ");
                notes.push_str(&note);
            }
            notes
        },
        draft_model: None,
        generation_kind,
        base_model: None,
    };

    if !is_drafter {
        profile.draft_model = find_matching_assistant_drafter(id);
    }

    Ok(profile)
}

/// `config.json`이 없고 `adapter_config.json`(PEFT LoRA)만 있는 저장소용 프로파일 초안.
/// 베이스 모델은 `transformers`(MPS) 백엔드로 로드 후 LoRA 가중치를 합쳐 추론한다.
pub fn draft_from_lora_adapter(
    adapter_config_path: &Path,
    id: &str,
) -> Result<ModelProfile, ProfileError> {
    let contents = std::fs::read_to_string(adapter_config_path)?;
    let adapter_config: serde_json::Value = serde_json::from_str(&contents)?;

    let base_model = adapter_config
        .get("base_model_name_or_path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ProfileError::Validation(
                "adapter_config.json에 base_model_name_or_path가 없습니다".into(),
            )
        })?
        .to_string();

    let task_type = adapter_config
        .get("task_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if task_type != "CAUSAL_LM" {
        return Err(ProfileError::Validation(format!(
            "지원하지 않는 LoRA task_type: {task_type} (CAUSAL_LM만 지원)"
        )));
    }

    let (context_max, rope_note) = find_config_json_in_cache(&base_model)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
        .map(|c| infer_context_max_with_notes(&c))
        .unwrap_or((4096, None));
    let context_default = context_max.min(4096).max(512);

    let mut notes = format!("auto-generated draft — LoRA adapter, base: {base_model}");
    if let Some(r) = adapter_config.get("r").and_then(|v| v.as_u64()) {
        notes.push_str(&format!("; r={r}"));
    }
    if let Some(note) = rope_note {
        notes.push_str("; ");
        notes.push_str(&note);
    }

    Ok(ModelProfile {
        schema_version: 1,
        id: id.into(),
        display_name: format!("{} (LoRA)", infer_display_name(id)),
        source: ProfileSource {
            kind: "hf".into(),
            hf_repo: id.into(),
            hf_file: String::new(),
            local_path: String::new(),
        },
        model_type: TASK_LLM.into(),
        backend: "transformers".into(),
        io: ProfileIo {
            input: vec!["chat".into()],
            output: "text".into(),
        },
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
        quantization: None,
        load_timeout_sec: 600,
        notes,
        draft_model: None,
        generation_kind: GENERATION_KIND_AUTOREGRESSIVE.into(),
        base_model: Some(base_model),
    })
}

pub fn generate_profile_hf(profiles_dir: &Path, repo_id: &str) -> Result<PathBuf, ProfileError> {
    if !is_valid_hf_repo_id(repo_id) {
        return Err(ProfileError::Validation(format!(
            "invalid HF repo id format: {repo_id}"
        )));
    }
    match find_config_json_in_cache(repo_id) {
        Ok(config_path) => {
            let profile = draft_from_config(&config_path, repo_id, "hf", None)?;
            write_profile_draft(profiles_dir, &profile)
        }
        Err(config_err) => match find_adapter_config_json_in_cache(repo_id) {
            Ok(adapter_path) => {
                let profile = draft_from_lora_adapter(&adapter_path, repo_id)?;
                write_profile_draft(profiles_dir, &profile)
            }
            Err(_) => Err(config_err),
        },
    }
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

pub fn set_profile_draft_model(
    profiles_dir: &Path,
    profile_id: &str,
    draft_model: Option<String>,
) -> Result<ModelProfile, ProfileError> {
    if let Some(ref draft_id) = draft_model {
        if !is_valid_hf_repo_id(draft_id) {
            return Err(ProfileError::Validation(format!(
                "invalid drafter HF repo id: {draft_id}"
            )));
        }
        let draft_profile = load_profile_by_id(profiles_dir, draft_id)?;
        if !is_drafter_profile(&draft_profile) {
            return Err(ProfileError::Validation(format!(
                "selected model is not a drafter profile: {draft_id}"
            )));
        }
    }

    let mut profile = load_profile_by_id(profiles_dir, profile_id)?;
    if is_drafter_profile(&profile) {
        return Err(ProfileError::Validation(
            "drafter profiles cannot link another drafter".into(),
        ));
    }
    profile.draft_model = draft_model;

    let filename = profile_filename_from_id(profile_id);
    let path = profiles_dir.join(&filename);
    let json = serde_json::to_string_pretty(&profile)?;
    std::fs::write(&path, json)?;
    Ok(profile)
}

pub fn list_drafter_profile_ids(profiles_dir: &Path) -> Result<Vec<String>, ProfileError> {
    Ok(list_profiles(profiles_dir)?
        .into_iter()
        .filter(|row| row.model_type == TASK_DRAFTER)
        .map(|row| row.id)
        .collect())
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
        // 재조회 없이 현재 backend로 mlx 여부를 근사(non-transformers는 mlx로 취급).
        let is_mlx = profile.backend != "transformers";
        profile.backend = infer_backend(task, &profile.id, is_mlx);
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
    fn infer_generation_kind_detects_diffusion() {
        let by_model_type = serde_json::json!({"model_type": "diffusion_gemma"});
        assert_eq!(
            infer_generation_kind(&by_model_type),
            GENERATION_KIND_DIFFUSION
        );

        let by_arch = serde_json::json!({
            "model_type": "gemma",
            "architectures": ["DiffusionGemmaForConditionalGeneration"]
        });
        assert_eq!(infer_generation_kind(&by_arch), GENERATION_KIND_DIFFUSION);

        let ar = serde_json::json!({
            "architectures": ["Qwen3ForCausalLM"],
            "model_type": "qwen3"
        });
        assert_eq!(infer_generation_kind(&ar), GENERATION_KIND_AUTOREGRESSIVE);
    }

    #[test]
    fn legacy_profile_without_generation_kind_deserializes() {
        let json = r#"{
            "schema_version": 1,
            "id": "org/model",
            "display_name": "Legacy",
            "source": { "kind": "hf", "hf_repo": "org/model" },
            "model_type": "llm",
            "backend": "vllm_mlx",
            "io": { "input": ["chat"], "output": "text" },
            "context": { "min": 512, "max": 4096, "default": 4096 },
            "default_params": {},
            "quantization": null,
            "load_timeout_sec": 600
        }"#;
        let profile: ModelProfile = serde_json::from_str(json).expect("parse legacy profile");
        assert_eq!(profile.generation_kind, GENERATION_KIND_AUTOREGRESSIVE);
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
    fn infer_backend_routes_non_mlx_llm_to_transformers() {
        // 일반 PyTorch/safetensors 체크포인트 (quantization 키 없음, mlx-community 아님)
        assert_eq!(
            infer_backend("llm", "kakaocorp/kanana-nano-2.1b-instruct", false),
            "transformers"
        );
        // MLX 변환본은 기존과 동일하게 vllm_mlx
        assert_eq!(
            infer_backend("llm", "mlx-community/Qwen3-4B-Instruct-2507-4bit", true),
            "vllm_mlx"
        );
        // gguf는 mlx 여부와 무관하게 llama_cpp
        assert_eq!(infer_backend("llm", "org/model-GGUF", false), "llama_cpp");
    }

    #[test]
    fn is_mlx_checkpoint_detection() {
        let mlx_quantized = serde_json::json!({"quantization": {"bits": 4}});
        assert!(is_mlx_checkpoint(&mlx_quantized, "org/repo"));

        let by_name = serde_json::json!({});
        assert!(is_mlx_checkpoint(&by_name, "mlx-community/Qwen3-4B"));

        let regular = serde_json::json!({"model_type": "llama"});
        assert!(!is_mlx_checkpoint(&regular, "kakaocorp/kanana-nano-2.1b-instruct"));
    }

    #[test]
    fn draft_from_lora_adapter_captures_base_model() {
        let dir = tempfile::tempdir().expect("tempdir");
        let adapter_config_path = dir.path().join("adapter_config.json");
        std::fs::write(
            &adapter_config_path,
            serde_json::json!({
                "base_model_name_or_path": "kakaocorp/kanana-nano-2.1b-instruct",
                "peft_type": "LORA",
                "task_type": "CAUSAL_LM",
                "r": 128
            })
            .to_string(),
        )
        .expect("write adapter_config.json");

        let profile = draft_from_lora_adapter(&adapter_config_path, "test-org/test-lora-adapter")
            .expect("draft from lora");

        assert_eq!(
            profile.base_model.as_deref(),
            Some("kakaocorp/kanana-nano-2.1b-instruct")
        );
        assert_eq!(profile.backend, "transformers");
        assert_eq!(profile.model_type, TASK_LLM);
        assert!(profile.notes.contains("r=128"));
        // 베이스 모델이 로컬 캐시에 있으면 그 config에서, 없으면 기본값(4096)에서 컨텍스트를 얻는다 —
        // 머신마다 캐시 상태가 다르므로 정확한 값이 아니라 유효 범위만 검증한다.
        assert!(profile.context.max >= 512);
    }

    #[test]
    fn draft_from_lora_adapter_rejects_non_causal_lm() {
        let dir = tempfile::tempdir().expect("tempdir");
        let adapter_config_path = dir.path().join("adapter_config.json");
        std::fs::write(
            &adapter_config_path,
            serde_json::json!({
                "base_model_name_or_path": "org/base",
                "task_type": "SEQ_CLS"
            })
            .to_string(),
        )
        .expect("write adapter_config.json");

        let result = draft_from_lora_adapter(&adapter_config_path, "org/adapter");
        assert!(result.is_err());
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
            draft_model: None,
            generation_kind: GENERATION_KIND_AUTOREGRESSIVE.into(),
            base_model: None,
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
        assert_eq!(
            generate_sweep_steps(1_048_576),
            vec![
                1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072, 262144, 524288,
                1_048_576
            ]
        );
    }

    #[test]
    fn infer_context_max_scaled_in_place_qwen_style() {
        let cfg = serde_json::json!({ "max_position_embeddings": 262144 });
        let (max, note) = infer_context_max_with_notes(&cfg);
        assert_eq!(max, 262144);
        assert!(note.is_none());
    }

    #[test]
    fn infer_context_max_rope_factor_style() {
        let cfg = serde_json::json!({
            "max_position_embeddings": 32768,
            "rope_scaling": { "type": "yarn", "factor": 4.0 }
        });
        let (max, note) = infer_context_max_with_notes(&cfg);
        assert_eq!(max, 131072);
        assert!(note.is_none());
    }

    #[test]
    fn infer_context_max_rope_original_already_scaled() {
        let cfg = serde_json::json!({
            "max_position_embeddings": 131072,
            "rope_scaling": {
                "original_max_position_embeddings": 32768,
                "type": "yarn",
                "factor": 4.0
            }
        });
        let (max, note) = infer_context_max_with_notes(&cfg);
        assert_eq!(max, 131072);
        assert!(note.is_none());
    }

    #[test]
    fn infer_context_max_unknown_rope_type_is_conservative() {
        let cfg = serde_json::json!({
            "max_position_embeddings": 32768,
            "rope_scaling": { "type": "supertrope", "factor": 8.0 }
        });
        let (max, note) = infer_context_max_with_notes(&cfg);
        assert_eq!(max, 32768);
        assert_eq!(note.as_deref(), Some("rope_scaling 미해석: supertrope"));
    }

    #[test]
    fn infer_context_max_text_config_rope_scaling() {
        let cfg = serde_json::json!({
            "text_config": {
                "max_position_embeddings": 8192,
                "rope_scaling": { "rope_type": "linear", "factor": 2.0 }
            }
        });
        let (max, note) = infer_context_max_with_notes(&cfg);
        assert_eq!(max, 16384);
        assert!(note.is_none());
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
        assert_eq!(task_label_ko(TASK_DRAFTER), "보조(drafter) 모델");
        assert_eq!(task_badge_short(TASK_ASR), Some("STT"));
        assert_eq!(task_badge_short(TASK_LLM), None);
    }

    #[test]
    fn legacy_profile_without_draft_model_deserializes() {
        let json = r#"{
            "schema_version": 1,
            "id": "org/model",
            "display_name": "Legacy",
            "source": { "kind": "hf", "hf_repo": "org/model" },
            "model_type": "llm",
            "backend": "vllm_mlx",
            "io": { "input": ["chat"], "output": "text" },
            "context": { "min": 512, "max": 4096, "default": 4096 },
            "default_params": {},
            "quantization": null,
            "load_timeout_sec": 600
        }"#;
        let profile: ModelProfile = serde_json::from_str(json).expect("parse legacy profile");
        assert!(profile.draft_model.is_none());
    }

    #[test]
    fn drafter_detection_by_repo_and_config() {
        let assistant_cfg = serde_json::json!({
            "model_type": "gemma4_assistant",
            "architectures": ["Gemma4AssistantForCausalLM"]
        });
        assert!(is_drafter_config(
            &assistant_cfg,
            "mlx-community/gemma-4-26B-A4B-it-qat-assistant-5bit"
        ));

        let main_cfg = serde_json::json!({
            "model_type": "gemma4_unified",
            "vision_config": {"hidden_size": 1152}
        });
        assert!(!is_drafter_config(
            &main_cfg,
            "mlx-community/gemma-4-26B-A4B-it-qat-5bit"
        ));
    }

    #[test]
    fn profile_for_spawn_strips_draft_when_disabled() {
        let profile = ModelProfile {
            schema_version: 1,
            id: "org/main".into(),
            display_name: "Main".into(),
            source: ProfileSource {
                kind: "hf".into(),
                hf_repo: "org/main".into(),
                hf_file: String::new(),
                local_path: String::new(),
            },
            model_type: TASK_LLM.into(),
            backend: "mlx_vlm".into(),
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
            draft_model: Some("org/assistant".into()),
            generation_kind: GENERATION_KIND_AUTOREGRESSIVE.into(),
            base_model: None,
        };
        let spawned = profile_for_spawn(&profile, false);
        assert!(spawned.draft_model.is_none());
        let spawned_on = profile_for_spawn(&profile, true);
        assert_eq!(spawned_on.draft_model.as_deref(), Some("org/assistant"));
    }

    #[test]
    fn ensure_runnable_profile_rejects_drafter() {
        let drafter = ModelProfile {
            schema_version: 1,
            id: "org/assistant".into(),
            display_name: "Assistant".into(),
            source: ProfileSource {
                kind: "hf".into(),
                hf_repo: "org/assistant".into(),
                hf_file: String::new(),
                local_path: String::new(),
            },
            model_type: TASK_DRAFTER.into(),
            backend: "mlx_vlm".into(),
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
            draft_model: None,
            generation_kind: GENERATION_KIND_AUTOREGRESSIVE.into(),
            base_model: None,
        };
        assert!(ensure_runnable_profile(&drafter).is_err());
    }

    #[test]
    fn ensure_runnable_profile_rejects_missing_base_model() {
        let lora = ModelProfile {
            schema_version: 1,
            id: "org/lora-adapter".into(),
            display_name: "LoRA".into(),
            source: ProfileSource {
                kind: "hf".into(),
                hf_repo: "org/lora-adapter".into(),
                hf_file: String::new(),
                local_path: String::new(),
            },
            model_type: TASK_LLM.into(),
            backend: "transformers".into(),
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
            draft_model: None,
            generation_kind: GENERATION_KIND_AUTOREGRESSIVE.into(),
            // 테스트 환경 캐시에 존재하지 않을 저장소 id — 실측 캐시를 건드리지 않는다.
            base_model: Some("org/definitely-not-cached-base-model-xyz".into()),
        };
        let err = ensure_runnable_profile(&lora).expect_err("missing base model must error");
        assert!(matches!(err, ProfileError::Validation(_)));
    }

    #[test]
    fn suggest_assistant_drafter_inserts_qat_assistant_segment() {
        let candidate = {
            let name = "gemma-4-26B-A4B-it-qat-5bit";
            let idx = name.rfind("-qat-").unwrap();
            let prefix = &name[..idx + 5];
            let suffix = &name[idx + 5..];
            format!("mlx-community/{prefix}assistant-{suffix}")
        };
        assert_eq!(
            candidate,
            "mlx-community/gemma-4-26B-A4B-it-qat-assistant-5bit"
        );
    }
}