//! 외부 도구 경로 해석 (uv 등)

use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use crate::app_support_dir;

/// uv를 찾지 못했을 때 사용자에게 표시할 안내 문구
pub const UV_NOT_FOUND_MSG: &str =
    "uv를 찾을 수 없습니다 — '환경 점검' 탭에서 자동 설정을 실행하세요";

/// 번들 배포 모드에서 doctor fix_action에 사용하는 마커
pub const BOOTSTRAP_FIX_ACTION: &str = "자동 설정 실행";

/// `AIDASH_UV` → PATH → 알려진 후보 경로 순으로 uv 절대경로를 해석한다.
pub fn resolve_uv() -> Option<PathBuf> {
    if let Ok(raw) = std::env::var("AIDASH_UV") {
        let path = PathBuf::from(raw);
        if is_uv_executable(&path) {
            return Some(path);
        }
    }

    if let Some(path) = find_in_path("uv") {
        return Some(path);
    }

    let home = home_dir();
    let mut candidates: Vec<PathBuf> = vec![
        home.join(".local/bin/uv"),
        PathBuf::from("/opt/homebrew/bin/uv"),
        PathBuf::from("/usr/local/bin/uv"),
        home.join(".cargo/bin/uv"),
    ];
    if let Some(support) = app_support_dir() {
        candidates.push(support.join("bin/uv"));
    }

    candidates.into_iter().find(|p| is_uv_executable(p))
}

/// 번들 앱에서 배포 모드인지 판별한다 (개발 트리 `python/adapters` 없음 + 번들 리소스 존재).
pub fn is_bundle_deploy_mode() -> bool {
    if let Ok(root) = std::env::var("AIDASH_ROOT") {
        if !root.is_empty() {
            let path = PathBuf::from(&root);
            if path.join("python").join("adapters").is_dir() {
                return false;
            }
        }
    }
    crate::macos_bundle_python_dir().is_some()
}

pub fn app_support_uv_bin_dir() -> Option<PathBuf> {
    app_support_dir().map(|d| d.join("bin"))
}

pub fn app_support_uv_path() -> Option<PathBuf> {
    app_support_uv_bin_dir().map(|d| d.join("uv"))
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"))
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var("PATH").ok()?;
    for dir in path_var.split(':').filter(|d| !d.is_empty()) {
        let candidate = PathBuf::from(dir).join(name);
        if is_uv_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_uv_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    StdCommand::new(path)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn resolve_uv_respects_aidash_uv_override() {
        let _guard = env_lock();
        let tmp = tempfile::tempdir().expect("tempdir");
        let fake_uv = tmp.path().join("uv");
        {
            let mut f = std::fs::File::create(&fake_uv).expect("create");
            writeln!(f, "#!/bin/sh").expect("write");
            writeln!(f, "echo uv 9.9.9").expect("write");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake_uv, std::fs::Permissions::from_mode(0o755))
                .expect("chmod");
        }

        let previous = std::env::var("AIDASH_UV").ok();
        std::env::set_var("AIDASH_UV", &fake_uv);
        let resolved = resolve_uv().expect("should resolve");
        assert_eq!(resolved, fake_uv);

        match previous {
            Some(v) => std::env::set_var("AIDASH_UV", v),
            None => std::env::remove_var("AIDASH_UV"),
        }
    }

    #[test]
    fn resolve_uv_finds_local_bin_when_path_blocked() {
        let _guard = env_lock();
        let home = std::env::var("HOME").expect("HOME");
        let local_uv = PathBuf::from(&home).join(".local/bin/uv");
        if !is_uv_executable(&local_uv) {
            eprintln!("skip: ~/.local/bin/uv not present");
            return;
        }

        let previous_path = std::env::var("PATH").ok();
        std::env::remove_var("AIDASH_UV");
        std::env::set_var("PATH", "/usr/bin:/bin");

        let resolved = resolve_uv().expect("should find ~/.local/bin/uv");
        assert_eq!(resolved, local_uv.canonicalize().unwrap_or(local_uv));

        match previous_path {
            Some(v) => std::env::set_var("PATH", v),
            None => std::env::remove_var("PATH"),
        }
    }

    #[test]
    fn resolve_uv_finds_app_support_candidate() {
        let _guard = env_lock();
        let tmp = tempfile::tempdir().expect("tempdir");
        let support = tmp
            .path()
            .join("Library")
            .join("Application Support")
            .join("AI_Dashboard");
        let bin_dir = support.join("bin");
        std::fs::create_dir_all(&bin_dir).expect("mkdir");
        let fake_uv = bin_dir.join("uv");
        {
            let mut f = std::fs::File::create(&fake_uv).expect("create");
            writeln!(f, "#!/bin/sh").expect("write");
            writeln!(f, "echo uv 0.1.0").expect("write");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake_uv, std::fs::Permissions::from_mode(0o755))
                .expect("chmod");
        }

        let prev_home = std::env::var("HOME").ok();
        let prev_path = std::env::var("PATH").ok();
        let prev_aidash_uv = std::env::var("AIDASH_UV").ok();

        std::env::set_var("HOME", tmp.path());
        std::env::remove_var("AIDASH_UV");
        std::env::set_var("PATH", "/usr/bin:/bin");

        let resolved = resolve_uv().expect("app support uv");
        assert_eq!(resolved, fake_uv);

        match prev_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        match prev_path {
            Some(v) => std::env::set_var("PATH", v),
            None => std::env::remove_var("PATH"),
        }
        match prev_aidash_uv {
            Some(v) => std::env::set_var("AIDASH_UV", v),
            None => std::env::remove_var("AIDASH_UV"),
        }
    }
}