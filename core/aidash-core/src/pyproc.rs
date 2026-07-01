//! Python subprocess 스폰·프로세스 그룹 관리·SIGTERM/SIGKILL·stdout/stderr 캡처
//!
//! IN: 백엔드 종류, 프로파일, 컨텍스트 크기, 포트
//! OUT: `ChildHandle`(pid, pgid, port), 프로세스 이벤트

use std::io;
use std::path::Path;
use std::process::Stdio;

use nix::sys::signal::{kill, Signal};
use nix::unistd::setsid;
use nix::unistd::Pid;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

use crate::auth;
use crate::profile::ModelProfile;
use crate::tools::{self, UV_NOT_FOUND_MSG};

#[derive(Debug, Clone)]
pub struct ChildSpec {
    pub program: String,
    pub args: Vec<String>,
    pub envs: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct ChildHandle {
    pub pid: u32,
    pub pgid: i32,
    pub port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogStream {
    Stdout,
    Stderr,
}

#[derive(Debug)]
pub enum PyprocEvent {
    Log {
        stream: LogStream,
        level: String,
        message: String,
    },
    Exited { code: Option<i32> },
}

pub struct SpawnedChild {
    pub handle: ChildHandle,
    pub event_rx: mpsc::Receiver<PyprocEvent>,
}

pub fn build_child_spec(
    python_dir: &Path,
    profile: &ModelProfile,
    context: u32,
    port: u16,
) -> Result<ChildSpec, String> {
    let adapter = format!("adapters.serve_{}", profile.backend);
    let profile_json = serde_json::to_string(profile).map_err(|e| e.to_string())?;

    let mut envs = Vec::new();
    if let Some((_, token)) = auth::resolve_token() {
        envs.push(("HF_TOKEN".into(), token));
    }

    let uv = tools::resolve_uv().ok_or_else(|| UV_NOT_FOUND_MSG.to_string())?;

    // LoRA 프로파일은 베이스 모델을 `--model-path`로 로드하고, 어댑터 저장소 자체를
    // `--adapter-path`로 전달해 어댑터가 베이스+LoRA를 합쳐 서빙한다.
    let model_path_arg = profile
        .base_model
        .clone()
        .unwrap_or_else(|| profile.model_path().to_string());

    let mut args = vec![
        "run".into(),
        "--project".into(),
        python_dir.display().to_string(),
        "python".into(),
        "-m".into(),
        adapter,
        "--model-path".into(),
        model_path_arg,
        "--context-size".into(),
        context.to_string(),
        "--port".into(),
        port.to_string(),
        "--profile-json".into(),
        profile_json,
    ];
    if let Some(ref draft_model) = profile.draft_model {
        args.push("--draft-model-path".into());
        args.push(draft_model.clone());
    }
    if profile.base_model.is_some() {
        args.push("--adapter-path".into());
        args.push(profile.model_path().to_string());
    }

    Ok(ChildSpec {
        program: uv.display().to_string(),
        args,
        envs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{ModelProfile, ProfileContext, ProfileIo, ProfileSource, TASK_LLM};

    fn fixture_profile(draft_model: Option<&str>) -> ModelProfile {
        fixture_profile_full(draft_model, None)
    }

    fn fixture_profile_full(draft_model: Option<&str>, base_model: Option<&str>) -> ModelProfile {
        ModelProfile {
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
            draft_model: draft_model.map(str::to_string),
            generation_kind: crate::profile::GENERATION_KIND_AUTOREGRESSIVE.into(),
            base_model: base_model.map(str::to_string),
        }
    }

    #[test]
    fn build_child_spec_omits_draft_arg_without_pairing() {
        let spec = build_child_spec(
            std::path::Path::new("/tmp/python"),
            &fixture_profile(None),
            4096,
            18080,
        )
        .expect("child spec");
        assert!(!spec.args.iter().any(|a| a == "--draft-model-path"));
    }

    #[test]
    fn build_child_spec_includes_draft_model_path() {
        let spec = build_child_spec(
            std::path::Path::new("/tmp/python"),
            &fixture_profile(Some("org/assistant")),
            4096,
            18080,
        )
        .expect("child spec");
        let draft_idx = spec
            .args
            .iter()
            .position(|a| a == "--draft-model-path")
            .expect("draft arg");
        assert_eq!(spec.args.get(draft_idx + 1).map(String::as_str), Some("org/assistant"));
    }

    #[test]
    fn build_child_spec_lora_uses_base_model_path_and_adapter_arg() {
        let spec = build_child_spec(
            std::path::Path::new("/tmp/python"),
            &fixture_profile_full(None, Some("org/base")),
            4096,
            18080,
        )
        .expect("child spec");
        let model_idx = spec
            .args
            .iter()
            .position(|a| a == "--model-path")
            .expect("model-path arg");
        assert_eq!(spec.args.get(model_idx + 1).map(String::as_str), Some("org/base"));

        let adapter_idx = spec
            .args
            .iter()
            .position(|a| a == "--adapter-path")
            .expect("adapter-path arg");
        assert_eq!(spec.args.get(adapter_idx + 1).map(String::as_str), Some("org/main"));
    }
}

pub fn spawn_child(spec: ChildSpec, port: u16) -> io::Result<SpawnedChild> {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for (key, value) in spec.envs {
        command.env(key, value);
    }

    unsafe {
        command.pre_exec(|| {
            setsid().map_err(|e| io::Error::other(e))?;
            Ok(())
        });
    }

    let mut child = command.spawn()?;
    let pid = child
        .id()
        .ok_or_else(|| io::Error::other("spawned child has no pid"))?;
    let pgid = pid as i32;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("stdout not captured"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("stderr not captured"))?;

    let (event_tx, event_rx) = mpsc::channel(256);

    spawn_log_reader(stdout, LogStream::Stdout, event_tx.clone());
    spawn_log_reader(stderr, LogStream::Stderr, event_tx.clone());
    spawn_wait_task(child, event_tx);

    Ok(SpawnedChild {
        handle: ChildHandle { pid, pgid, port },
        event_rx,
    })
}

fn is_tqdm_noise(line: &str) -> bool {
    line.contains("%|") || line.contains('\r')
}

fn parse_log_line(stream: LogStream, line: &str) -> Option<(String, String)> {
    if is_tqdm_noise(line) {
        return None;
    }

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
        if let Some(level) = value.get("level").and_then(|v| v.as_str()) {
            let message = value
                .get("message")
                .and_then(|v| v.as_str())
                .or_else(|| value.get("event").and_then(|v| v.as_str()))
                .unwrap_or(line)
                .to_string();
            return Some((level.to_string(), message));
        }
        if value.get("event").is_some() {
            return Some(("info".into(), line.to_string()));
        }
    }

    let level = match stream {
        LogStream::Stdout => "info",
        LogStream::Stderr => "info",
    };
    Some((level.into(), line.to_string()))
}

fn spawn_log_reader<R>(reader: R, stream: LogStream, event_tx: mpsc::Sender<PyprocEvent>)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let Some((level, message)) = parse_log_line(stream, &line) else {
                continue;
            };
            if event_tx
                .send(PyprocEvent::Log {
                    stream,
                    level,
                    message,
                })
                .await
                .is_err()
            {
                break;
            }
        }
    });
}

fn spawn_wait_task(mut child: Child, event_tx: mpsc::Sender<PyprocEvent>) {
    tokio::spawn(async move {
        let code = match child.wait().await {
            Ok(status) => status.code(),
            Err(_) => None,
        };
        let _ = event_tx.send(PyprocEvent::Exited { code }).await;
    });
}

pub async fn terminate_graceful(pgid: i32) {
    let _ = signal_process_group(pgid, Signal::SIGTERM);
    sleep(Duration::from_secs(3)).await;
    let _ = signal_process_group(pgid, Signal::SIGKILL);
}

pub async fn terminate_abort(pgid: i32) {
    let _ = signal_process_group(pgid, Signal::SIGKILL);
}

pub fn signal_process_group(pgid: i32, sig: Signal) -> Result<(), String> {
    kill(Pid::from_raw(-pgid), sig).map_err(|e| e.to_string())
}

pub fn is_process_alive(pid: u32) -> bool {
    let mut system = sysinfo::System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    system
        .process(sysinfo::Pid::from_u32(pid))
        .is_some()
}

pub fn pick_free_port() -> io::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    Ok(port)
}

pub fn is_port_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

pub async fn reap_process_group(pgid: i32, root_pid: u32, port: u16) {
    let _ = signal_process_group(pgid, Signal::SIGKILL);
    for _ in 0..20 {
        if !is_process_alive(root_pid) && is_port_free(port) {
            return;
        }
        sleep(Duration::from_millis(100)).await;
    }
}

pub async fn wait_for_child_exit(
    mut event_rx: mpsc::Receiver<PyprocEvent>,
) -> Option<i32> {
    while let Some(event) = event_rx.recv().await {
        if let PyprocEvent::Exited { code } = event {
            return code;
        }
    }
    None
}