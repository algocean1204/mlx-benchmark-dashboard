//! 런타임 Python 환경 부트스트랩: uv 설치, 번들→App Support 동기화, venv+의존성

use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;

use crate::tools::{self, app_support_uv_bin_dir, app_support_uv_path};
use crate::{macos_bundle_python_dir, python_dir};

const BUNDLED_VERSION_FILE: &str = ".bundled_version";

#[derive(Debug, Clone)]
pub enum BootstrapEvent {
    StepStart { step: String, message: String },
    StepDone {
        step: String,
        success: bool,
        message: String,
    },
    Log { line: String },
}

/// 멱등 부트스트랩 파이프라인을 실행한다.
pub async fn env_bootstrap<F>(project_root: &Path, mut on_event: F) -> Result<(), String>
where
    F: FnMut(BootstrapEvent),
{
    ensure_uv(&mut on_event).await?;
    sync_python_sources(project_root, &mut on_event)?;
    ensure_venv_and_deps(project_root, &mut on_event).await?;
    Ok(())
}

async fn ensure_uv<F>(on_event: &mut F) -> Result<(), String>
where
    F: FnMut(BootstrapEvent),
{
    const STEP: &str = "uv";
    if let Some(path) = tools::resolve_uv() {
        on_event(BootstrapEvent::StepStart {
            step: STEP.into(),
            message: format!("uv 확인됨: {}", path.display()),
        });
        on_event(BootstrapEvent::StepDone {
            step: STEP.into(),
            success: true,
            message: "이미 설치되어 있음".into(),
        });
        return Ok(());
    }

    on_event(BootstrapEvent::StepStart {
        step: STEP.into(),
        message: "uv 자동 설치 시작".into(),
    });

    let bin_dir = app_support_uv_bin_dir()
        .ok_or_else(|| "Application Support 경로를 확인할 수 없습니다".to_string())?;
    std::fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;

    let install_dir = bin_dir.display().to_string();
    let script = "curl -LsSf https://astral.sh/uv/install.sh | sh";
    let mut cmd = TokioCommand::new("sh");
    cmd.arg("-c")
        .arg(script)
        .env("UV_INSTALL_DIR", &install_dir)
        .env("UV_NO_MODIFY_PATH", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("uv 설치 스크립트 실행 실패: {e}"))?;
    stream_child_lines(&mut child, on_event).await?;

    let status = child.wait().await.map_err(|e| e.to_string())?;
    if !status.success() {
        on_event(BootstrapEvent::StepDone {
            step: STEP.into(),
            success: false,
            message: format!("uv 설치 실패 (exit {})", status.code().unwrap_or(-1)),
        });
        return Err("uv 자동 설치에 실패했습니다".into());
    }

    let uv_path = app_support_uv_path().ok_or_else(|| "uv 경로 확인 실패".to_string())?;
    if !uv_path.is_file() {
        on_event(BootstrapEvent::StepDone {
            step: STEP.into(),
            success: false,
            message: format!("설치 후 uv 없음: {}", uv_path.display()),
        });
        return Err("uv 설치 후 실행 파일을 찾을 수 없습니다".into());
    }

    on_event(BootstrapEvent::StepDone {
        step: STEP.into(),
        success: true,
        message: format!("uv 설치 완료: {}", uv_path.display()),
    });
    Ok(())
}

fn sync_python_sources<F>(project_root: &Path, on_event: &mut F) -> Result<(), String>
where
    F: FnMut(BootstrapEvent),
{
    const STEP: &str = "python_sync";
    let target = python_dir(project_root);

    if !tools::is_bundle_deploy_mode() {
        on_event(BootstrapEvent::StepStart {
            step: STEP.into(),
            message: "개발 모드 — 번들 동기화 생략".into(),
        });
        on_event(BootstrapEvent::StepDone {
            step: STEP.into(),
            success: true,
            message: "개발 모드".into(),
        });
        return Ok(());
    }

    let Some(source) = macos_bundle_python_dir() else {
        return Err("번들 python 리소스를 찾을 수 없습니다".into());
    };

    if python_sync_up_to_date(&source, &target)? {
        on_event(BootstrapEvent::StepStart {
            step: STEP.into(),
            message: "python 소스가 최신 상태".into(),
        });
        on_event(BootstrapEvent::StepDone {
            step: STEP.into(),
            success: true,
            message: "동기화 생략".into(),
        });
        return Ok(());
    }

    on_event(BootstrapEvent::StepStart {
        step: STEP.into(),
        message: format!("{} → {}", source.display(), target.display()),
    });

    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    for name in ["adapters", "tools"] {
        let src = source.join(name);
        if !src.is_dir() {
            return Err(format!("번들에 {name}/ 디렉터리가 없습니다"));
        }
        copy_dir_recursive(&src, &target.join(name))?;
    }
    for name in ["pyproject.toml", "uv.lock"] {
        let src = source.join(name);
        if !src.is_file() {
            return Err(format!("번들에 {name}이 없습니다"));
        }
        std::fs::copy(&src, target.join(name)).map_err(|e| e.to_string())?;
    }

    if let Some(version) = bundle_short_version() {
        std::fs::write(target.join(BUNDLED_VERSION_FILE), version).map_err(|e| e.to_string())?;
    }

    on_event(BootstrapEvent::StepDone {
        step: STEP.into(),
        success: true,
        message: "python 소스 동기화 완료".into(),
    });
    Ok(())
}

async fn ensure_venv_and_deps<F>(project_root: &Path, on_event: &mut F) -> Result<(), String>
where
    F: FnMut(BootstrapEvent),
{
    const STEP: &str = "venv_sync";
    let py_dir = python_dir(project_root);
    let venv_python = py_dir.join(".venv/bin/python");

    if venv_python.is_file() {
        on_event(BootstrapEvent::StepStart {
            step: STEP.into(),
            message: "가상환경이 이미 구성됨".into(),
        });
        on_event(BootstrapEvent::StepDone {
            step: STEP.into(),
            success: true,
            message: format!("{}", venv_python.display()),
        });
        return Ok(());
    }

    let uv = tools::resolve_uv().ok_or_else(|| tools::UV_NOT_FOUND_MSG.to_string())?;

    on_event(BootstrapEvent::StepStart {
        step: STEP.into(),
        message: "Python 3.12 및 의존성 설치 (uv sync)".into(),
    });

    let mut install_py = TokioCommand::new(&uv);
    install_py
        .args(["python", "install", "3.12"])
        .current_dir(&py_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut py_child = install_py
        .spawn()
        .map_err(|e| format!("uv python install 실행 실패: {e}"))?;
    stream_child_lines(&mut py_child, on_event).await?;
    let py_status = py_child.wait().await.map_err(|e| e.to_string())?;
    if !py_status.success() {
        on_event(BootstrapEvent::StepDone {
            step: STEP.into(),
            success: false,
            message: "Python 3.12 설치 실패".into(),
        });
        return Err("Python 3.12 설치에 실패했습니다".into());
    }

    let mut sync = TokioCommand::new(&uv);
    sync.arg("sync")
        .current_dir(&py_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut sync_child = sync
        .spawn()
        .map_err(|e| format!("uv sync 실행 실패: {e}"))?;
    stream_child_lines(&mut sync_child, on_event).await?;
    let sync_status = sync_child.wait().await.map_err(|e| e.to_string())?;
    if !sync_status.success() {
        on_event(BootstrapEvent::StepDone {
            step: STEP.into(),
            success: false,
            message: "uv sync 실패".into(),
        });
        return Err("의존성 설치(uv sync)에 실패했습니다".into());
    }

    if !venv_python.is_file() {
        on_event(BootstrapEvent::StepDone {
            step: STEP.into(),
            success: false,
            message: ".venv/bin/python 없음".into(),
        });
        return Err("가상환경 생성 후 python 실행 파일을 찾을 수 없습니다".into());
    }

    on_event(BootstrapEvent::StepDone {
        step: STEP.into(),
        success: true,
        message: "가상환경 및 의존성 설치 완료".into(),
    });
    Ok(())
}

async fn stream_child_lines<F>(
    child: &mut tokio::process::Child,
    on_event: &mut F,
) -> Result<(), String>
where
    F: FnMut(BootstrapEvent),
{
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    if let Some(out) = stdout {
        let mut lines = BufReader::new(out).lines();
        while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
            on_event(BootstrapEvent::Log { line });
        }
    }
    if let Some(err) = stderr {
        let mut lines = BufReader::new(err).lines();
        while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
            on_event(BootstrapEvent::Log { line });
        }
    }
    Ok(())
}

/// 번들 버전 마커와 대상 디렉터리 상태로 동기화 필요 여부를 판단한다.
pub fn python_sync_up_to_date(source: &Path, target: &Path) -> Result<bool, String> {
    if !target.join("adapters").is_dir() || !target.join("pyproject.toml").is_file() {
        return Ok(false);
    }
    let Some(current) = bundle_short_version() else {
        return Ok(false);
    };
    let marker = target.join(BUNDLED_VERSION_FILE);
    if !marker.is_file() {
        return Ok(false);
    }
    let stored = std::fs::read_to_string(&marker).map_err(|e| e.to_string())?;
    if stored.trim() != current.trim() {
        return Ok(false);
    }
    // 소스에 tools/가 있으면 대상에도 있어야 한다.
    if source.join("tools").is_dir() && !target.join("tools").is_dir() {
        return Ok(false);
    }
    Ok(true)
}

pub fn bundle_short_version() -> Option<String> {
    let plist = macos_bundle_info_plist()?;
    let output = std::process::Command::new("/usr/libexec/PlistBuddy")
        .args([
            "-c",
            "Print CFBundleShortVersionString",
            plist.to_str()?,
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if version.is_empty() {
        None
    } else {
        Some(version)
    }
}

fn macos_bundle_info_plist() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let macos_dir = exe.parent()?;
    if macos_dir.file_name()?.to_str()? != "MacOS" {
        return None;
    }
    let contents = macos_dir.parent()?;
    let plist = contents.join("Info.plist");
    if plist.is_file() {
        Some(plist)
    } else {
        None
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        let dest_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), &dest_path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_sync_up_to_date_requires_marker_and_adapters() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = tmp.path().join("bundle");
        let target = tmp.path().join("target");
        std::fs::create_dir_all(source.join("tools")).expect("tools");
        std::fs::create_dir_all(target.join("adapters")).expect("adapters");
        std::fs::write(target.join("pyproject.toml"), "[project]\n").expect("toml");

        assert!(!python_sync_up_to_date(&source, &target).expect("check"));

        std::fs::write(target.join(BUNDLED_VERSION_FILE), "1.0.0").expect("marker");
        // bundle_short_version() is None in test binary — still false without matching runtime version
        assert!(!python_sync_up_to_date(&source, &target).expect("check2"));
    }

    #[test]
    fn copy_dir_recursive_copies_nested_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let src = tmp.path().join("src");
        let dst = tmp.path().join("dst");
        std::fs::create_dir_all(src.join("adapters")).expect("mkdir");
        std::fs::write(src.join("pyproject.toml"), "x").expect("write");
        std::fs::write(src.join("adapters/a.py"), "print(1)").expect("write");

        copy_dir_recursive(&src, &dst).expect("copy");
        assert!(dst.join("adapters/a.py").is_file());
        assert!(dst.join("pyproject.toml").is_file());
    }

    #[test]
    fn bundled_version_marker_roundtrip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target = tmp.path().join("python");
        std::fs::create_dir_all(&target).expect("mkdir");
        std::fs::write(target.join(BUNDLED_VERSION_FILE), "2.5.0").expect("write");
        let stored = std::fs::read_to_string(target.join(BUNDLED_VERSION_FILE)).expect("read");
        assert_eq!(stored.trim(), "2.5.0");
    }
}