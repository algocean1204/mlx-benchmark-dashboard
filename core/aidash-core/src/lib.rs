pub mod api;
pub mod auth;
pub mod bench;
pub mod client;
pub mod db;
pub mod env_detect;
pub mod hf_cache;
pub mod eval;
pub mod events;
pub mod export;
pub mod lifecycle;
pub mod monitor;
pub mod sys_memory;
pub mod profile;
pub mod pyproc;
pub mod stats;
pub mod tps_tier;
pub mod watchdog;

use std::path::{Path, PathBuf};

/// 앱 지원 디렉터리 (`~/Library/Application Support/AI_Dashboard`)
pub fn app_support_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(|home| {
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("AI_Dashboard")
    })
}

/// macOS 앱 번들의 `Contents/Resources/python/` 경로를 반환한다.
pub fn macos_bundle_python_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let macos_dir = exe.parent()?;
    if macos_dir.file_name()?.to_str()? != "MacOS" {
        return None;
    }
    let contents = macos_dir.parent()?;
    if contents.file_name()?.to_str()? != "Contents" {
        return None;
    }
    let python = contents.join("Resources").join("python");
    if python.join("adapters").is_dir() {
        Some(python)
    } else {
        None
    }
}

fn is_dev_project_root(dir: &Path) -> bool {
    dir.join("profiles").is_dir() && dir.join("python").join("adapters").is_dir()
}

fn bundle_fallback_root() -> Option<PathBuf> {
    macos_bundle_python_dir()?;
    let support = app_support_dir()?;
    let profiles = support.join("profiles");
    let _ = std::fs::create_dir_all(&profiles);
    Some(support)
}

/// 프로젝트 루트를 탐색한다. `AIDASH_ROOT` → cwd 상향 탐색 → 번들 폴백 순이다.
pub fn find_project_root() -> Option<PathBuf> {
    if let Ok(override_path) = std::env::var("AIDASH_ROOT") {
        let path = PathBuf::from(&override_path);
        if path.is_dir() {
            return Some(path);
        }
    }

    if let Ok(mut dir) = std::env::current_dir() {
        for _ in 0..6 {
            if is_dev_project_root(&dir) {
                return Some(dir);
            }
            if !dir.pop() {
                break;
            }
        }
    }

    bundle_fallback_root()
}

pub fn profiles_dir(root: &Path) -> PathBuf {
    let profiles = root.join("profiles");
    if !profiles.is_dir() {
        let _ = std::fs::create_dir_all(&profiles);
    }
    profiles
}

pub fn python_dir(root: &Path) -> PathBuf {
    let dev_python = root.join("python");
    if dev_python.join("adapters").is_dir() {
        return dev_python;
    }
    if let Some(bundle_python) = macos_bundle_python_dir() {
        return bundle_python;
    }
    dev_python
}

pub fn eval_sets_dir(root: &Path) -> PathBuf {
    root.join("eval_sets")
}

/// 입력 파일 경로 해석: (1) cwd 기준 → (2) 프로젝트 루트 폴백. 둘 다 없으면 시도 경로 목록과 함께 오류.
pub fn resolve_file_path(path: &str, project_root: &Path) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(path);
    let mut tried: Vec<String> = Vec::new();

    if candidate.is_absolute() {
        tried.push(candidate.display().to_string());
        if candidate.is_file() {
            return Ok(candidate);
        }
    } else if let Ok(cwd) = std::env::current_dir() {
        let cwd_path = cwd.join(&candidate);
        tried.push(cwd_path.display().to_string());
        if cwd_path.is_file() {
            return Ok(cwd_path);
        }
        let root_path = project_root.join(&candidate);
        tried.push(root_path.display().to_string());
        if root_path.is_file() {
            return Ok(root_path);
        }
    } else {
        let root_path = project_root.join(&candidate);
        tried.push(root_path.display().to_string());
        if root_path.is_file() {
            return Ok(root_path);
        }
    }

    Err(format!("파일 없음: {}", tried.join(", ")))
}

#[cfg(test)]
mod project_root_tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn find_project_root_respects_aidash_root_override() {
        let _guard = env_lock();
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join("profiles")).expect("profiles");
        std::fs::create_dir_all(root.join("python/adapters")).expect("python");

        let previous = std::env::var("AIDASH_ROOT").ok();
        std::env::set_var("AIDASH_ROOT", root);

        let found = find_project_root().expect("AIDASH_ROOT should resolve");
        assert_eq!(found, root);

        match previous {
            Some(value) => std::env::set_var("AIDASH_ROOT", value),
            None => std::env::remove_var("AIDASH_ROOT"),
        }
    }

    #[test]
    fn python_dir_prefers_dev_tree_over_bundle() {
        let _guard = env_lock();
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        let dev_python = root.join("python");
        std::fs::create_dir_all(dev_python.join("adapters")).expect("adapters");
        assert_eq!(python_dir(root), dev_python);
    }

    #[test]
    fn profiles_dir_creates_bundle_profiles_directory() {
        let _guard = env_lock();
        let tmp = tempfile::tempdir().expect("tempdir");
        let support = tmp.path().join("AI_Dashboard");
        std::fs::create_dir_all(&support).expect("support");
        let profiles = profiles_dir(&support);
        assert!(profiles.ends_with("profiles"));
        assert!(profiles.is_dir());
    }
}

/// 기기 정보 문자열 (예: "Apple M4 Pro / 48GB")
pub fn system_device_label() -> String {
    let chip = std::process::Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Unknown CPU".into());

    let ram_gb = sys_memory::total_system_memory_bytes() as f64 / (1024.0 * 1024.0 * 1024.0);
    format!("{chip} / {ram_gb:.0}GB")
}