//! HF 토큰 관리: macOS Keychain 저장·조회·삭제, 외부 소스 감지·가져오기, whoami 검증

use std::fmt;
use std::path::PathBuf;

use serde::Serialize;

pub const KEYRING_SERVICE: &str = "ai-dashboard";
pub const KEYRING_USER: &str = "hf-token";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenSource {
    Keychain,
    EnvHfToken,
    EnvHuggingFaceHubToken,
    HfCliFile,
}

impl TokenSource {
    pub fn label(self) -> &'static str {
        match self {
            TokenSource::Keychain => "Keychain (ai-dashboard)",
            TokenSource::EnvHfToken => "env HF_TOKEN",
            TokenSource::EnvHuggingFaceHubToken => "env HUGGING_FACE_HUB_TOKEN",
            TokenSource::HfCliFile => "~/.cache/huggingface/token",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenSourceStatus {
    pub source: TokenSource,
    pub present: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthStatus {
    pub sources: Vec<TokenSourceStatus>,
    pub active_source: Option<TokenSource>,
    pub masked_token: Option<String>,
    pub whoami_user: String,
}

#[derive(Debug)]
pub enum AuthError {
    Keyring(String),
    Io(std::io::Error),
    InvalidToken(String),
    WhoamiFailed(String),
    NoTokenFound,
    EmptyInput,
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::Keyring(msg) => write!(f, "keyring error: {msg}"),
            AuthError::Io(e) => write!(f, "io error: {e}"),
            AuthError::InvalidToken(msg) => write!(f, "{msg}"),
            AuthError::WhoamiFailed(msg) => write!(f, "whoami failed: {msg}"),
            AuthError::NoTokenFound => write!(f, "no token found in hf-cli file"),
            AuthError::EmptyInput => write!(f, "empty token input"),
        }
    }
}

impl std::error::Error for AuthError {}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"))
}

pub fn hf_cli_token_path() -> PathBuf {
    home_dir().join(".cache/huggingface/token")
}

/// 앞 3자 + `****` + 끝 4자. 짧은 토큰은 전부 마스킹.
pub fn mask_token(token: &str) -> String {
    let trimmed = token.trim();
    if trimmed.len() <= 7 {
        return "****".to_string();
    }
    let start = &trimmed[..3];
    let end = &trimmed[trimmed.len() - 4..];
    format!("{start}****{end}")
}

pub fn keychain_has_token() -> bool {
    keychain_get().is_ok()
}

pub fn keychain_get() -> Result<String, AuthError> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| AuthError::Keyring(e.to_string()))?;
    entry
        .get_password()
        .map_err(|e| AuthError::Keyring(e.to_string()))
}

pub fn keychain_set(token: &str) -> Result<(), AuthError> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| AuthError::Keyring(e.to_string()))?;
    entry
        .set_password(token)
        .map_err(|e| AuthError::Keyring(e.to_string()))
}

pub fn keychain_clear() -> Result<(), AuthError> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| AuthError::Keyring(e.to_string()))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AuthError::Keyring(e.to_string())),
    }
}

pub fn env_hf_token_present() -> bool {
    std::env::var("HF_TOKEN")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

pub fn env_huggingface_hub_token_present() -> bool {
    std::env::var("HUGGING_FACE_HUB_TOKEN")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

pub fn hf_cli_file_present() -> bool {
    let path = hf_cli_token_path();
    path.is_file() && std::fs::read_to_string(&path).map(|s| !s.trim().is_empty()).unwrap_or(false)
}

fn read_hf_cli_token() -> Option<String> {
    let path = hf_cli_token_path();
    let contents = std::fs::read_to_string(&path).ok()?;
    let token = contents.trim().to_string();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

/// Keychain → env(`HF_TOKEN`, `HUGGING_FACE_HUB_TOKEN`) → hf-cli 토큰 파일.
pub fn resolve_token() -> Option<(TokenSource, String)> {
    if let Ok(token) = keychain_get() {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return Some((TokenSource::Keychain, trimmed.to_string()));
        }
    }

    if let Ok(token) = std::env::var("HF_TOKEN") {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return Some((TokenSource::EnvHfToken, trimmed.to_string()));
        }
    }

    if let Ok(token) = std::env::var("HUGGING_FACE_HUB_TOKEN") {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return Some((TokenSource::EnvHuggingFaceHubToken, trimmed.to_string()));
        }
    }

    read_hf_cli_token().map(|token| (TokenSource::HfCliFile, token))
}

pub async fn verify_whoami(token: &str) -> Result<String, AuthError> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://huggingface.co/api/whoami-v2")
        .bearer_auth(token.trim())
        .send()
        .await
        .map_err(|e| AuthError::WhoamiFailed(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(AuthError::InvalidToken(format!(
            "HTTP {}",
            resp.status().as_u16()
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AuthError::WhoamiFailed(e.to_string()))?;

    body.get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| AuthError::WhoamiFailed("missing name field".into()))
}

pub async fn build_auth_status() -> AuthStatus {
    let sources = vec![
        TokenSourceStatus {
            source: TokenSource::Keychain,
            present: keychain_has_token(),
        },
        TokenSourceStatus {
            source: TokenSource::EnvHfToken,
            present: env_hf_token_present(),
        },
        TokenSourceStatus {
            source: TokenSource::EnvHuggingFaceHubToken,
            present: env_huggingface_hub_token_present(),
        },
        TokenSourceStatus {
            source: TokenSource::HfCliFile,
            present: hf_cli_file_present(),
        },
    ];

    let active = resolve_token();
    let masked_token = active.as_ref().map(|(_, t)| mask_token(t));
    let active_source = active.as_ref().map(|(s, _)| *s);

    let whoami_user = match active {
        Some((_, token)) => match verify_whoami(&token).await {
            Ok(name) => name,
            Err(AuthError::WhoamiFailed(_)) => "검증 불가(오프라인)".into(),
            Err(_) => "검증 실패".into(),
        },
        None => "없음".into(),
    };

    AuthStatus {
        sources,
        active_source,
        masked_token,
        whoami_user,
    }
}

pub async fn set_token_from_stdin() -> Result<String, AuthError> {
    let token = rpassword::prompt_password("HF token: ").map_err(AuthError::Io)?;
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err(AuthError::EmptyInput);
    }

    let username = verify_whoami(&token).await?;
    keychain_set(&token)?;
    Ok(username)
}

pub async fn import_from_hf_cli() -> Result<String, AuthError> {
    let token = read_hf_cli_token().ok_or(AuthError::NoTokenFound)?;
    let username = verify_whoami(&token).await?;
    keychain_set(&token)?;
    Ok(username)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_token_standard() {
        assert_eq!(mask_token("hf_abcdefghijklmnop"), "hf_****mnop");
    }

    #[test]
    fn mask_token_short() {
        assert_eq!(mask_token("short"), "****");
    }

    #[test]
    fn mask_token_exact_boundary() {
        assert_eq!(mask_token("1234567"), "****");
        assert_eq!(mask_token("12345678"), "123****5678");
    }
}