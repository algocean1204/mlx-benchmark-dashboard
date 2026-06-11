//! Hugging Face hub 캐시 관리·모델 검색·다운로드 (Python 헬퍼 + HF API).

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use sysinfo::Disks;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;

use crate::auth;
use crate::python_dir;

const DISK_RESERVE_GB: u64 = 10;
const DISK_SIZE_MULTIPLIER: f64 = 1.2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheRevision {
    pub revision: String,
    pub size_bytes: u64,
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheRepoEntry {
    pub repo_id: String,
    pub size_bytes: u64,
    pub last_modified: Option<String>,
    pub revisions: Vec<CacheRevision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheScanResult {
    pub cache_dir: String,
    pub total_size_bytes: u64,
    pub repo_count: usize,
    pub repos: Vec<CacheRepoEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheDeleteResult {
    pub repo_id: String,
    pub deleted: bool,
    #[serde(default)]
    pub freed_bytes: u64,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskUsage {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub cache_dir: String,
    pub cache_total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HfSearchResult {
    pub repo_id: String,
    pub downloads: i64,
    pub likes: i64,
    pub pipeline_tag: Option<String>,
    pub installed: bool,
}

#[derive(Debug)]
pub enum HfCacheError {
    Io(std::io::Error),
    Json(serde_json::Error),
    CommandFailed { exit: i32, stderr: String },
    Api(String),
    DiskSpace { required: u64, available: u64 },
    DownloadInProgress,
    Gated,
    Unauthorized,
}

impl std::fmt::Display for HfCacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HfCacheError::Io(e) => write!(f, "io: {e}"),
            HfCacheError::Json(e) => write!(f, "json: {e}"),
            HfCacheError::CommandFailed { exit, stderr } => {
                write!(f, "command failed (exit {exit}): {stderr}")
            }
            HfCacheError::Api(msg) => write!(f, "api: {msg}"),
            HfCacheError::DiskSpace { required, available } => {
                write!(
                    f,
                    "insufficient disk: need {} bytes, have {} available",
                    required, available
                )
            }
            HfCacheError::DownloadInProgress => write!(f, "download already in progress"),
            HfCacheError::Gated => write!(f, "gated model — token required"),
            HfCacheError::Unauthorized => write!(f, "unauthorized — check HF token"),
        }
    }
}

impl std::error::Error for HfCacheError {}

fn hf_cache_script(project_root: &Path) -> PathBuf {
    project_root.join("python/tools/hf_cache.py")
}

async fn run_hf_cache_json(
    project_root: &Path,
    args: &[&str],
) -> Result<serde_json::Value, HfCacheError> {
    let script = hf_cache_script(project_root);
    let mut cmd = TokioCommand::new("uv");
    cmd.arg("run")
        .arg("python")
        .arg(&script)
        .args(args)
        .current_dir(python_dir(project_root))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = cmd.output().await.map_err(HfCacheError::Io)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(stdout.trim()) {
            return Ok(val);
        }
        return Err(HfCacheError::CommandFailed {
            exit: output.status.code().unwrap_or(-1),
            stderr: stderr.to_string(),
        });
    }

    serde_json::from_str(stdout.trim()).map_err(HfCacheError::Json)
}

pub async fn cache_scan(project_root: &Path) -> Result<CacheScanResult, HfCacheError> {
    let val = run_hf_cache_json(project_root, &["scan"]).await?;
    if let Some(err) = val.get("error").and_then(|v| v.as_str()) {
        return Err(HfCacheError::Api(err.to_string()));
    }
    serde_json::from_value(val).map_err(HfCacheError::Json)
}

pub async fn cache_delete(
    project_root: &Path,
    repo_id: &str,
) -> Result<CacheDeleteResult, HfCacheError> {
    let val = run_hf_cache_json(project_root, &["delete", "--repo", repo_id]).await?;
    serde_json::from_value(val).map_err(HfCacheError::Json)
}

pub fn disk_usage_from_scan(scan: &CacheScanResult) -> DiskUsage {
    let disks = Disks::new_with_refreshed_list();
    let mut total = 0u64;
    let mut available = 0u64;
    for disk in disks.list() {
        total = total.max(disk.total_space());
        available = available.max(disk.available_space());
    }
    DiskUsage {
        total_bytes: total,
        available_bytes: available,
        cache_dir: scan.cache_dir.clone(),
        cache_total_bytes: scan.total_size_bytes,
    }
}

pub async fn disk_usage(project_root: &Path) -> Result<DiskUsage, HfCacheError> {
    let scan = cache_scan(project_root).await?;
    Ok(disk_usage_from_scan(&scan))
}

/// 다운로드 전 디스크 여유 검사: `여유 < 모델크기×1.2 + 10GB`면 차단.
pub fn check_disk_for_download(available_bytes: u64, model_size_bytes: u64) -> Result<(), HfCacheError> {
    let reserve = DISK_RESERVE_GB * 1024 * 1024 * 1024;
    let required = (model_size_bytes as f64 * DISK_SIZE_MULTIPLIER) as u64 + reserve;
    if available_bytes < required {
        return Err(HfCacheError::DiskSpace {
            required,
            available: available_bytes,
        });
    }
    Ok(())
}

fn hf_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

fn auth_header() -> Option<String> {
    auth::resolve_token().map(|(_, t)| t)
}

pub async fn hf_search(
    query: &str,
    installed_repos: &[String],
) -> Result<Vec<HfSearchResult>, HfCacheError> {
    let client = hf_client();
    let url = format!(
        "https://huggingface.co/api/models?search={}&limit=20&sort=downloads",
        urlencoding::encode(query)
    );
    let mut req = client.get(&url);
    if let Some(token) = auth_header() {
        req = req.bearer_auth(token);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| HfCacheError::Api(e.to_string()))?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(HfCacheError::Unauthorized);
    }
    if !resp.status().is_success() {
        return Err(HfCacheError::Api(format!("HTTP {}", resp.status())));
    }

    let body: Vec<serde_json::Value> = resp
        .json()
        .await
        .map_err(|e| HfCacheError::Api(e.to_string()))?;

    let installed_set: std::collections::HashSet<&str> =
        installed_repos.iter().map(|s| s.as_str()).collect();

    let mut results = Vec::new();
    for item in body {
        let repo_id = item
            .get("id")
            .or_else(|| item.get("modelId"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if repo_id.is_empty() {
            continue;
        }
        results.push(HfSearchResult {
            repo_id: repo_id.clone(),
            downloads: item.get("downloads").and_then(|v| v.as_i64()).unwrap_or(0),
            likes: item.get("likes").and_then(|v| v.as_i64()).unwrap_or(0),
            pipeline_tag: item
                .get("pipeline_tag")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            installed: installed_set.contains(repo_id.as_str()),
        });
    }
    Ok(results)
}

pub async fn hf_model_size(repo_id: &str) -> Result<u64, HfCacheError> {
    let client = hf_client();
    let url = format!(
        "https://huggingface.co/api/models/{}?blobs=true",
        urlencoding::encode(repo_id)
    );
    let mut req = client.get(&url);
    if let Some(token) = auth_header() {
        req = req.bearer_auth(token);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| HfCacheError::Api(e.to_string()))?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(HfCacheError::Unauthorized);
    }
    if resp.status() == reqwest::StatusCode::FORBIDDEN {
        return Err(HfCacheError::Gated);
    }
    if !resp.status().is_success() {
        return Err(HfCacheError::Api(format!("HTTP {}", resp.status())));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| HfCacheError::Api(e.to_string()))?;

    let mut total = 0u64;
    if let Some(siblings) = body.get("siblings").and_then(|v| v.as_array()) {
        for sib in siblings {
            if let Some(size) = sib.get("size").and_then(|v| v.as_u64()) {
                total = total.saturating_add(size);
            }
        }
    }
    Ok(total)
}

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub line: String,
    pub percent: Option<f64>,
    pub done: bool,
    pub success: bool,
}

fn parse_download_percent(line: &str) -> Option<f64> {
    // hf download progress: "Downloaded X/Y" or percentage patterns
    if let Some(idx) = line.find('%') {
        let before = &line[..idx];
        if let Some(num) = before
            .rsplit(|c: char| !c.is_ascii_digit() && c != '.')
            .next()
        {
            if let Ok(p) = num.parse::<f64>() {
                return Some(p);
            }
        }
    }
    if line.contains('/') {
        let parts: Vec<&str> = line.split_whitespace().collect();
        for part in parts {
            if part.contains('/') {
                let nums: Vec<&str> = part.split('/').collect();
                if nums.len() == 2 {
                    if let (Ok(a), Ok(b)) = (nums[0].parse::<f64>(), nums[1].parse::<f64>()) {
                        if b > 0.0 {
                            return Some((a / b) * 100.0);
                        }
                    }
                }
            }
        }
    }
    None
}

pub async fn hf_download<F>(
    project_root: &Path,
    repo_id: &str,
    mut on_progress: F,
    cancel: tokio::sync::watch::Receiver<bool>,
) -> Result<(), HfCacheError>
where
    F: FnMut(DownloadProgress),
{
    let mut cmd = TokioCommand::new("uv");
    cmd.arg("run")
        .arg("hf")
        .arg("download")
        .arg(repo_id)
        .current_dir(python_dir(project_root))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some((_, token)) = auth::resolve_token() {
        cmd.env("HF_TOKEN", token);
    }

    let mut child = cmd.spawn().map_err(HfCacheError::Io)?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let mut stdout_lines = if let Some(out) = stdout {
        BufReader::new(out).lines()
    } else {
        return Err(HfCacheError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "no stdout",
        )));
    };

    let mut stderr_lines = if let Some(err) = stderr {
        Some(BufReader::new(err).lines())
    } else {
        None
    };

    loop {
        if *cancel.borrow() {
            let _ = child.kill().await;
            return Err(HfCacheError::Api("download cancelled".into()));
        }

        tokio::select! {
            line = stdout_lines.next_line() => {
                match line {
                    Ok(Some(l)) => {
                        let pct = parse_download_percent(&l);
                        on_progress(DownloadProgress {
                            line: l,
                            percent: pct,
                            done: false,
                            success: true,
                        });
                    }
                    Ok(None) => break,
                    Err(e) => return Err(HfCacheError::Io(e)),
                }
            }
            line = async {
                if let Some(ref mut reader) = stderr_lines {
                    reader.next_line().await
                } else {
                    std::future::pending().await
                }
            } => {
                if let Ok(Some(l)) = line {
                    let lower = l.to_lowercase();
                    let pct = parse_download_percent(&l);
                    on_progress(DownloadProgress {
                        line: l.clone(),
                        percent: pct,
                        done: false,
                        success: true,
                    });
                    if lower.contains("gated") {
                        let _ = child.kill().await;
                        return Err(HfCacheError::Gated);
                    }
                    if lower.contains("401") || lower.contains("unauthorized") {
                        let _ = child.kill().await;
                        return Err(HfCacheError::Unauthorized);
                    }
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
        }

        if let Ok(Some(status)) = child.try_wait() {
            let success = status.success();
            on_progress(DownloadProgress {
                line: format!("exit code: {}", status.code().unwrap_or(-1)),
                percent: if success { Some(100.0) } else { None },
                done: true,
                success,
            });
            if success {
                return Ok(());
            }
            return Err(HfCacheError::CommandFailed {
                exit: status.code().unwrap_or(-1),
                stderr: "hf download failed".into(),
            });
        }
    }

    let status = child.wait().await.map_err(HfCacheError::Io)?;
    let success = status.success();
    on_progress(DownloadProgress {
        line: format!("exit code: {}", status.code().unwrap_or(-1)),
        percent: if success { Some(100.0) } else { None },
        done: true,
        success,
    });
    if success {
        Ok(())
    } else {
        Err(HfCacheError::CommandFailed {
            exit: status.code().unwrap_or(-1),
            stderr: "hf download failed".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disk_check_blocks_when_insufficient() {
        let model_size = 5 * 1024 * 1024 * 1024; // 5 GB
        let available = 10 * 1024 * 1024 * 1024; // 10 GB — need 5*1.2+10 = 16 GB
        let err = check_disk_for_download(available, model_size).unwrap_err();
        assert!(matches!(err, HfCacheError::DiskSpace { .. }));
    }

    #[test]
    fn disk_check_allows_when_sufficient() {
        let model_size = 1 * 1024 * 1024; // 1 MB
        let available = 100 * 1024 * 1024 * 1024; // 100 GB
        assert!(check_disk_for_download(available, model_size).is_ok());
    }

    #[test]
    fn parse_cache_scan_json() {
        let json = r#"{
            "cache_dir": "/tmp/hf",
            "total_size_bytes": 1000,
            "repo_count": 1,
            "repos": [{
                "repo_id": "org/model",
                "size_bytes": 1000,
                "last_modified": "2026-01-01T00:00:00",
                "revisions": []
            }]
        }"#;
        let scan: CacheScanResult = serde_json::from_str(json).unwrap();
        assert_eq!(scan.repo_count, 1);
        assert_eq!(scan.repos[0].repo_id, "org/model");
    }

    #[test]
    fn parse_delete_result_json() {
        let json = r#"{"repo_id":"org/m","deleted":true,"freed_bytes":500}"#;
        let result: CacheDeleteResult = serde_json::from_str(json).unwrap();
        assert!(result.deleted);
        assert_eq!(result.freed_bytes, 500);
    }

    #[test]
    fn parse_download_percent_from_line() {
        assert_eq!(parse_download_percent("  45% downloaded"), Some(45.0));
    }
}