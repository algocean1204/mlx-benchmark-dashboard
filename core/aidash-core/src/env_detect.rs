//! 환경 감지: 시스템·도구·백엔드 패키지·HF 캐시 모델·토큰 소스 스캔

use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use serde::Serialize;
use sysinfo::{Disks, System};

use crate::auth;
use crate::profile;
use crate::sys_memory;
use crate::tools::{self, BOOTSTRAP_FIX_ACTION};
use crate::{profiles_dir, python_dir};

const DISK_WARN_BYTES: u64 = 20 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorStatus {
    Ok,
    Warn,
    Missing,
    Info,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorItem {
    pub category: String,
    pub name: String,
    pub status: DoctorStatus,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_action: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    pub items: Vec<DoctorItem>,
}

struct BackendCheck {
    name: &'static str,
    module: &'static str,
    extra: &'static str,
}

const BACKENDS: &[BackendCheck] = &[
    BackendCheck {
        name: "vllm-mlx",
        module: "vllm_mlx",
        extra: "vllm",
    },
    BackendCheck {
        name: "mlx-lm",
        module: "mlx_lm",
        extra: "mlx-lm",
    },
    BackendCheck {
        name: "mlx-vlm",
        module: "mlx_vlm",
        extra: "vlm",
    },
    BackendCheck {
        name: "llama-cpp-python",
        module: "llama_cpp",
        extra: "cpu",
    },
    BackendCheck {
        name: "transformers",
        module: "transformers",
        extra: "cpu",
    },
    BackendCheck {
        name: "mflux",
        module: "mflux",
        extra: "imagegen",
    },
    BackendCheck {
        name: "mlx-whisper",
        module: "mlx_whisper",
        extra: "whisper",
    },
    BackendCheck {
        name: "mlx-audio",
        module: "mlx_audio",
        extra: "audio",
    },
];

/// `models--org--name` → `org/name`
pub fn cache_dir_to_repo_id(dir_name: &str) -> Option<String> {
    let rest = dir_name.strip_prefix("models--")?;
    let (org, model) = rest.split_once("--")?;
    Some(format!("{org}/{model}"))
}

pub fn format_bytes(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    if bytes >= GB as u64 {
        format!("{:.1} GB", bytes as f64 / GB)
    } else {
        format!("{:.1} MB", bytes as f64 / MB)
    }
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let entries = match std::fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return 0,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            total = total.saturating_add(dir_size(&p));
        } else if let Ok(meta) = entry.metadata() {
            total = total.saturating_add(meta.len());
        }
    }
    total
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"))
}

fn run_command_version(program: &str, args: &[&str]) -> Option<String> {
    let output = StdCommand::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?.trim();
    if line.is_empty() {
        None
    } else {
        Some(line.to_string())
    }
}

fn parse_python_version(version_line: &str) -> Option<(u32, u32)> {
    let rest = version_line.strip_prefix("Python ")?;
    let major_minor = rest.split_whitespace().next()?;
    let mut parts = major_minor.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    Some((major, minor))
}

fn scan_system(items: &mut Vec<DoctorItem>) {
    let mut system = System::new_all();
    system.refresh_all();

    let os_version = System::long_os_version()
        .or_else(System::os_version)
        .unwrap_or_else(|| "unknown".into());
    items.push(DoctorItem {
        category: "system".into(),
        name: "macOS version".into(),
        status: DoctorStatus::Info,
        detail: os_version,
        fix_action: None,
    });

    let is_apple_silicon = std::env::consts::ARCH == "aarch64";
    items.push(DoctorItem {
        category: "system".into(),
        name: "Apple Silicon".into(),
        status: if is_apple_silicon {
            DoctorStatus::Ok
        } else {
            DoctorStatus::Missing
        },
        detail: if is_apple_silicon {
            format!("aarch64 ({})", std::env::consts::ARCH)
        } else {
            format!("{} (vllm-mlx unavailable)", std::env::consts::ARCH)
        },
        fix_action: if is_apple_silicon {
            None
        } else {
            Some("Use CPU backend: uv sync --extra cpu".into())
        },
    });

    let mem = sys_memory::system_memory();
    let total_gb = mem.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let used_gb = mem.used_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let avail_gb = mem.available_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let free_pct = sys_memory::system_free_percent();
    items.push(DoctorItem {
        category: "system".into(),
        name: "RAM".into(),
        status: DoctorStatus::Info,
        detail: format!(
            "{total_gb:.1} GB total · {used_gb:.1} GB used · {avail_gb:.1} GB free ({free_pct:.0}%)"
        ),
        fix_action: None,
    });

    let disks = Disks::new_with_refreshed_list();
    let mut max_available = 0u64;
    for disk in disks.list() {
        max_available = max_available.max(disk.available_space());
    }
    let disk_status = if max_available < DISK_WARN_BYTES {
        DoctorStatus::Warn
    } else {
        DoctorStatus::Ok
    };
    items.push(DoctorItem {
        category: "system".into(),
        name: "disk free".into(),
        status: disk_status,
        detail: format!("{} available", format_bytes(max_available)),
        fix_action: if disk_status == DoctorStatus::Warn {
            Some("Free at least 20 GB of disk space".into())
        } else {
            None
        },
    });
}

fn bundle_fix_or_dev(dev_action: &str) -> String {
    if tools::is_bundle_deploy_mode() {
        BOOTSTRAP_FIX_ACTION.into()
    } else {
        dev_action.into()
    }
}

fn scan_tools(items: &mut Vec<DoctorItem>, python_dir: &Path) {
    let uv_path = tools::resolve_uv();
    match uv_path.as_ref().and_then(|p| run_command_version(p.to_str()?, &["--version"])) {
        Some(version) => {
            let detail = if let Some(path) = uv_path {
                format!("{version} ({})", path.display())
            } else {
                version
            };
            items.push(DoctorItem {
                category: "tools".into(),
                name: "uv".into(),
                status: DoctorStatus::Ok,
                detail,
                fix_action: None,
            });
        }
        None => {
            items.push(DoctorItem {
                category: "tools".into(),
                name: "uv".into(),
                status: DoctorStatus::Missing,
                detail: "not found".into(),
                fix_action: Some(bundle_fix_or_dev("brew install uv")),
            });
        }
    }

    let venv_python = python_dir.join(".venv/bin/python");
    if venv_python.is_file() {
        if let Some(version) = run_command_version(venv_python.to_str().unwrap_or("python3"), &["--version"]) {
            let (major, minor) = parse_python_version(&version).unwrap_or((0, 0));
            let ok = major > 3 || (major == 3 && minor >= 12);
            items.push(DoctorItem {
                category: "tools".into(),
                name: "python3".into(),
                status: if ok {
                    DoctorStatus::Ok
                } else {
                    DoctorStatus::Missing
                },
                detail: format!("{version} (venv)"),
                fix_action: if ok {
                    None
                } else {
                    Some(bundle_fix_or_dev("cd python && uv sync"))
                },
            });
        }
    } else {
        items.push(DoctorItem {
            category: "tools".into(),
            name: "python3".into(),
            status: DoctorStatus::Missing,
            detail: "venv not configured".into(),
            fix_action: Some(bundle_fix_or_dev("cd python && uv sync")),
        });
    }

    if let Some(version) = run_command_version("python3", &["--version"]) {
        items.push(DoctorItem {
            category: "tools".into(),
            name: "python3 (system)".into(),
            status: DoctorStatus::Info,
            detail: version,
            fix_action: None,
        });
    } else {
        items.push(DoctorItem {
            category: "tools".into(),
            name: "python3 (system)".into(),
            status: DoctorStatus::Info,
            detail: "not found".into(),
            fix_action: None,
        });
    }
}

fn check_backend_package(venv_python: &Path, module: &str) -> Option<String> {
    let script = format!(
        "import {module}; print(getattr({module},'__version__','?'))"
    );
    let output = StdCommand::new(venv_python)
        .args(["-c", &script])
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

fn scan_backends(items: &mut Vec<DoctorItem>, python_dir: &Path) {
    let venv = python_dir.join(".venv");
    let venv_python = venv.join("bin/python");

    if !venv_python.is_file() {
        let fix = bundle_fix_or_dev("cd python && uv sync");
        for backend in BACKENDS {
            items.push(DoctorItem {
                category: "backend".into(),
                name: backend.name.into(),
                status: DoctorStatus::Missing,
                detail: "venv not found".into(),
                fix_action: Some(fix.clone()),
            });
        }
        return;
    }

    for backend in BACKENDS {
        match check_backend_package(&venv_python, backend.module) {
            Some(version) => {
                items.push(DoctorItem {
                    category: "backend".into(),
                    name: backend.name.into(),
                    status: DoctorStatus::Ok,
                    detail: format!("v{version}"),
                    fix_action: None,
                });
            }
            None => {
                items.push(DoctorItem {
                    category: "backend".into(),
                    name: backend.name.into(),
                    status: DoctorStatus::Missing,
                    detail: "not installed".into(),
                    fix_action: Some(if tools::is_bundle_deploy_mode() {
                    BOOTSTRAP_FIX_ACTION.into()
                } else {
                    format!("cd python && uv sync --extra {}", backend.extra)
                }),
                });
            }
        }
    }
}

fn scan_external_installs(items: &mut Vec<DoctorItem>) {
    let pipx_venvs = home_dir().join(".local/pipx/venvs");
    let names: Vec<String> = if pipx_venvs.is_dir() {
        std::fs::read_dir(&pipx_venvs)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .collect()
    } else {
        vec![]
    };

    let detail = if names.is_empty() {
        "none detected".into()
    } else {
        names.join(", ")
    };

    items.push(DoctorItem {
        category: "external".into(),
        name: "pipx venvs".into(),
        status: DoctorStatus::Info,
        detail,
        fix_action: None,
    });
}

fn load_profile_repo_ids(profiles_dir: &Path) -> Vec<String> {
    let mut ids = Vec::new();
    if !profiles_dir.is_dir() {
        return ids;
    }
    if let Ok(entries) = std::fs::read_dir(profiles_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(p) = profile::load_profile_file(&path) {
                ids.push(p.id.clone());
                if !p.source.hf_repo.is_empty() {
                    ids.push(p.source.hf_repo.clone());
                }
            }
        }
    }
    ids
}

fn scan_models(items: &mut Vec<DoctorItem>, profiles_dir: &Path) {
    let hub = home_dir().join(".cache/huggingface/hub");
    let profile_ids = load_profile_repo_ids(profiles_dir);

    if !hub.is_dir() {
        items.push(DoctorItem {
            category: "model".into(),
            name: "HF cache".into(),
            status: DoctorStatus::Info,
            detail: "cache directory not found".into(),
            fix_action: None,
        });
        return;
    }

    let mut found_any = false;
    if let Ok(entries) = std::fs::read_dir(&hub) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.starts_with("models--") {
                continue;
            }
            let Some(repo_id) = cache_dir_to_repo_id(&name_str) else {
                continue;
            };
            found_any = true;
            let size = dir_size(&entry.path());
            let has_profile = profile_ids.iter().any(|id| id == &repo_id);
            let (status, fix) = if has_profile {
                (DoctorStatus::Ok, None)
            } else {
                (
                    DoctorStatus::Warn,
                    Some(format!(
                        "aidash profile generate --hf {repo_id}"
                    )),
                )
            };
            let profile_note = if has_profile {
                "profile exists"
            } else {
                "no profile — can generate"
            };
            items.push(DoctorItem {
                category: "model".into(),
                name: repo_id,
                status,
                detail: format!("{} ({profile_note})", format_bytes(size)),
                fix_action: fix,
            });
        }
    }

    if !found_any {
        items.push(DoctorItem {
            category: "model".into(),
            name: "HF cache".into(),
            status: DoctorStatus::Info,
            detail: "no cached models".into(),
            fix_action: None,
        });
    }
}

fn scan_token(items: &mut Vec<DoctorItem>) {
    let keychain = auth::keychain_has_token();
    let env_hf = auth::env_hf_token_present();
    let env_hub = auth::env_huggingface_hub_token_present();
    let hf_file = auth::hf_cli_file_present();

    items.push(DoctorItem {
        category: "token".into(),
        name: "Keychain".into(),
        status: if keychain {
            DoctorStatus::Ok
        } else {
            DoctorStatus::Info
        },
        detail: if keychain { "present" } else { "not set" }.into(),
        fix_action: None,
    });
    items.push(DoctorItem {
        category: "token".into(),
        name: "HF_TOKEN env".into(),
        status: if env_hf {
            DoctorStatus::Ok
        } else {
            DoctorStatus::Info
        },
        detail: if env_hf { "present" } else { "not set" }.into(),
        fix_action: None,
    });
    items.push(DoctorItem {
        category: "token".into(),
        name: "HUGGING_FACE_HUB_TOKEN env".into(),
        status: if env_hub {
            DoctorStatus::Ok
        } else {
            DoctorStatus::Info
        },
        detail: if env_hub { "present" } else { "not set" }.into(),
        fix_action: None,
    });
    items.push(DoctorItem {
        category: "token".into(),
        name: "hf-cli token file".into(),
        status: if hf_file {
            DoctorStatus::Ok
        } else {
            DoctorStatus::Info
        },
        detail: if hf_file { "present" } else { "not set" }.into(),
        fix_action: None,
    });

    let any = keychain || env_hf || env_hub || hf_file;
    let masked = auth::resolve_token()
        .map(|(_, t)| auth::mask_token(&t))
        .unwrap_or_else(|| "none".into());

    items.push(DoctorItem {
        category: "token".into(),
        name: "active token".into(),
        status: if any {
            DoctorStatus::Ok
        } else {
            DoctorStatus::Warn
        },
        detail: format!("masked={masked}"),
        fix_action: if any {
            None
        } else {
            Some("aidash auth set or aidash auth import (public models only without token)".into())
        },
    });
}

pub fn scan_environment(project_root: &Path) -> DoctorReport {
    let py_dir = python_dir(project_root);
    let prof_dir = profiles_dir(project_root);

    let mut items = Vec::new();
    scan_system(&mut items);
    scan_tools(&mut items, &py_dir);
    scan_backends(&mut items, &py_dir);
    scan_external_installs(&mut items);
    scan_models(&mut items, &prof_dir);
    scan_token(&mut items);

    DoctorReport { items }
}

pub async fn run_doctor(project_root: PathBuf) -> DoctorReport {
    tokio::task::spawn_blocking(move || scan_environment(&project_root))
        .await
        .unwrap_or_else(|e| panic!("doctor scan panicked: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_dir_to_repo_id_standard() {
        assert_eq!(
            cache_dir_to_repo_id("models--mlx-community--Qwen3-30B-A3B-4bit"),
            Some("mlx-community/Qwen3-30B-A3B-4bit".into())
        );
    }

    #[test]
    fn cache_dir_to_repo_id_invalid() {
        assert_eq!(cache_dir_to_repo_id("snapshots--foo"), None);
        assert_eq!(cache_dir_to_repo_id("models--onlyorg"), None);
    }
}