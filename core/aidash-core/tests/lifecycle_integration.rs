use std::path::PathBuf;
use std::time::Duration;

use aidash_core::events::CoreEvent;
use aidash_core::lifecycle::{
    wait_model_ready, Command, LifecycleHandle, LifecycleState, StartParams,
};
use aidash_core::profile::ModelProfile;
use aidash_core::pyproc::{pick_free_port, ChildSpec};
use reqwest::Client;
use tokio::time::{sleep, timeout};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn fake_server_path() -> PathBuf {
    fixture_root().join("tests").join("fixtures").join("fake_server.py")
}

fn test_profile() -> ModelProfile {
    ModelProfile {
        schema_version: 1,
        id: "test/fake".into(),
        display_name: "Fake".into(),
        source: aidash_core::profile::ProfileSource {
            kind: "hf".into(),
            hf_repo: "test/model".into(),
            hf_file: String::new(),
            local_path: String::new(),
        },
        model_type: "llm".into(),
        backend: "vllm_mlx".into(),
        io: aidash_core::profile::ProfileIo {
            input: vec!["chat".into()],
            output: "text".into(),
        },
        context: aidash_core::profile::ProfileContext {
            min: 512,
            max: 8192,
            default: 1024,
            sweep_steps: vec![],
        },
        default_params: serde_json::json!({}),
        quantization: Some("4bit".into()),
        load_timeout_sec: 30,
        notes: String::new(),
        draft_model: None,
        generation_kind: aidash_core::profile::GENERATION_KIND_AUTOREGRESSIVE.into(),
        base_model: None,
    }
}

fn fake_child_spec(port: u16, load_delay_sec: &str) -> ChildSpec {
    ChildSpec {
        program: "python3".into(),
        args: vec![
            fake_server_path().display().to_string(),
            "--port".into(),
            port.to_string(),
            "--load-delay-sec".into(),
            load_delay_sec.into(),
        ],
        envs: vec![],
    }
}

#[tokio::test]
async fn normal_lifecycle_cycle() {
    let port = pick_free_port().expect("free port");
    let profile = test_profile();
    let handle = LifecycleHandle::spawn();

    let start = StartParams {
        profile,
        context: 1024,
        mem_limit_gb: None,
        port: Some(port),
        python_dir: fixture_root().join("python"),
        child_spec: Some(fake_child_spec(port, "0.3")),
    };

    handle
        .command_tx
        .send(Command::Start(start))
        .await
        .expect("start send");

    handle.wait_for_state(LifecycleState::Ready).await;

    // Verify we reached key states
    assert_eq!(*handle.state_rx.borrow(), LifecycleState::Ready);

    handle
        .command_tx
        .send(Command::Stop)
        .await
        .expect("stop send");
    handle.wait_for_state(LifecycleState::Idle).await;

    assert_eq!(*handle.state_rx.borrow(), LifecycleState::Idle);

    // No python fake_server still running on our port
    let client = Client::new();
    let resp = client
        .get(format!("http://127.0.0.1:{port}/health"))
        .send()
        .await;
    assert!(resp.is_err(), "server should be stopped");

}

#[tokio::test]
async fn watchdog_kill_on_memory_limit() {
    let port = pick_free_port().expect("free port");
    let profile = test_profile();
    let handle = LifecycleHandle::spawn();
    let mut rx = handle.event_tx.subscribe();

    let start = StartParams {
        profile,
        context: 1024,
        mem_limit_gb: Some(0.05),
        port: Some(port),
        python_dir: fixture_root().join("python"),
        child_spec: Some(fake_child_spec(port, "0.2")),
    };

    handle
        .command_tx
        .send(Command::Start(start))
        .await
        .expect("start send");

    handle.wait_for_state(LifecycleState::Ready).await;

    let client = Client::new();
    let _ = client
        .get(format!("http://127.0.0.1:{port}/alloc?mb=80"))
        .send()
        .await
        .expect("alloc request");

    let mut saw_kill = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while tokio::time::Instant::now() < deadline {
        if let Ok(Ok(event)) = timeout(Duration::from_millis(200), rx.recv()).await {
            if matches!(event, CoreEvent::WatchdogKill) {
                saw_kill = true;
            }
        }
        if *handle.state_rx.borrow() == LifecycleState::Idle {
            break;
        }
    }

    handle.wait_for_state(LifecycleState::Idle).await;
    assert!(saw_kill, "expected WatchdogKill event");
    assert_eq!(*handle.state_rx.borrow(), LifecycleState::Idle);

    let resp = client
        .get(format!("http://127.0.0.1:{port}/health"))
        .send()
        .await;
    assert!(resp.is_err(), "server should be dead after watchdog kill");
}

#[tokio::test]
async fn spawning_failure_returns_to_idle() {
    let port = pick_free_port().expect("free port");
    let profile = test_profile();
    let handle = LifecycleHandle::spawn();

    let start = StartParams {
        profile,
        context: 1024,
        mem_limit_gb: None,
        port: Some(port),
        python_dir: fixture_root().join("python"),
        child_spec: Some(ChildSpec {
            program: "/nonexistent/aidash-fake-program".into(),
            args: vec![],
            envs: vec![],
        }),
    };

    handle
        .command_tx
        .send(Command::Start(start))
        .await
        .expect("start send");

    handle.wait_for_state(LifecycleState::Idle).await;
    assert_eq!(*handle.state_rx.borrow(), LifecycleState::Idle);
}

#[tokio::test]
async fn normal_cycle_state_transitions() {
    let port = pick_free_port().expect("free port");
    let profile = test_profile();
    let handle = LifecycleHandle::spawn();
    let mut rx = handle.event_tx.subscribe();

    let start = StartParams {
        profile,
        context: 1024,
        mem_limit_gb: None,
        port: Some(port),
        python_dir: fixture_root().join("python"),
        child_spec: Some(fake_child_spec(port, "0.2")),
    };

    handle
        .command_tx
        .send(Command::Start(start))
        .await
        .expect("start");

    let mut transitions = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while tokio::time::Instant::now() < deadline {
        while let Ok(ev) = rx.try_recv() {
            if let CoreEvent::StateChanged { from, to } = ev {
                transitions.push((from, to));
            }
        }
        if *handle.state_rx.borrow() == LifecycleState::Ready {
            break;
        }
        sleep(Duration::from_millis(50)).await;
    }

    assert!(
        transitions.contains(&(LifecycleState::Idle, LifecycleState::Spawning)),
        "missing Idle->Spawning: {transitions:?}"
    );
    assert!(
        transitions.iter().any(|(_, to)| *to == LifecycleState::Loading),
        "missing ->Loading: {transitions:?}"
    );
    assert!(
        transitions.iter().any(|(_, to)| *to == LifecycleState::Ready),
        "missing ->Ready: {transitions:?}"
    );

    handle.command_tx.send(Command::Stop).await.expect("stop");
    handle.wait_for_state(LifecycleState::Idle).await;

    assert!(
        transitions.iter().any(|(_, to)| *to == LifecycleState::Stopping)
            || *handle.state_rx.borrow() == LifecycleState::Idle
    );
}

#[tokio::test]
async fn wait_model_ready_succeeds_after_delayed_health() {
    let port = pick_free_port().expect("free port");
    let profile = test_profile();
    let handle = LifecycleHandle::spawn();

    let start = StartParams {
        profile,
        context: 1024,
        mem_limit_gb: None,
        port: Some(port),
        python_dir: fixture_root().join("python"),
        child_spec: Some(fake_child_spec(port, "1.0")),
    };

    handle
        .command_tx
        .send(Command::Start(start))
        .await
        .expect("start send");

    let mut state_rx = handle.state_rx.clone();
    let result = wait_model_ready(
        &mut state_rx,
        port,
        Duration::from_secs(5),
    )
    .await;

    assert!(result.is_ok(), "expected ready: {result:?}");

    handle.command_tx.send(Command::Stop).await.expect("stop");
    handle.wait_for_state(LifecycleState::Idle).await;
}

#[tokio::test]
async fn wait_model_ready_times_out_when_never_ready() {
    let port = pick_free_port().expect("free port");
    let mut profile = test_profile();
    profile.load_timeout_sec = 60;
    let handle = LifecycleHandle::spawn();

    let start = StartParams {
        profile,
        context: 1024,
        mem_limit_gb: None,
        port: Some(port),
        python_dir: fixture_root().join("python"),
        child_spec: Some(fake_child_spec(port, "120")),
    };

    handle
        .command_tx
        .send(Command::Start(start))
        .await
        .expect("start send");

    let mut state_rx = handle.state_rx.clone();
    let result = wait_model_ready(
        &mut state_rx,
        port,
        Duration::from_secs(1),
    )
    .await;

    assert!(result.is_err(), "expected timeout error");
    let err = result.err().expect("error message");
    assert!(
        err.contains("모델 로드 시간 초과"),
        "unexpected error: {err}"
    );

    let _ = handle.command_tx.send(Command::Abort {
        reason: "test cleanup".into(),
    }).await;
    handle.wait_for_state(LifecycleState::Idle).await;
}

#[tokio::test]
async fn port_available_via_watch_channel_even_when_checked_immediately_after_start() {
    // FRB의 serve_wait_ready()가 예전엔 Command::Start 전송 직후 port_rx를
    // 딱 한 번만 borrow해서, lifecycle 태스크가 아직 포트를 할당하기 전이면
    // (흔한 스케줄링 타이밍) "서버 포트가 아직 준비되지 않았습니다"로 즉시
    // 실패했다 — 채팅 시작이 랜덤하게 실패하던 원인. changed()로 실제 대기
    // 하도록 고친 뒤, 이 흐름 전체(포트 대기 → wait_model_ready)가 정상
    // 동작함을 확인한다.
    let port = pick_free_port().expect("free port");
    let profile = test_profile();
    let handle = LifecycleHandle::spawn();

    let start = StartParams {
        profile,
        context: 1024,
        mem_limit_gb: None,
        port: Some(port),
        python_dir: fixture_root().join("python"),
        child_spec: Some(fake_child_spec(port, "0.3")),
    };

    handle
        .command_tx
        .send(Command::Start(start))
        .await
        .expect("start send");

    let mut port_rx = handle.port_rx.clone();
    let got_port = timeout(Duration::from_secs(5), async {
        loop {
            if let Some(p) = *port_rx.borrow() {
                return p;
            }
            port_rx.changed().await.expect("port channel closed");
        }
    })
    .await
    .expect("port should become available without an explicit start delay");
    assert_eq!(got_port, port);

    let mut state_rx = handle.state_rx.clone();
    wait_model_ready(&mut state_rx, got_port, Duration::from_secs(5))
        .await
        .expect("model should become ready");

    handle
        .command_tx
        .send(Command::Stop)
        .await
        .expect("stop send");
    handle.wait_for_state(LifecycleState::Idle).await;
}

#[tokio::test]
async fn stop_during_loading_terminates_process_instead_of_leaking_it() {
    // 예전 버그: handle_stop()이 Ready/Busy 상태에서만 실제로 프로세스를
    // 종료했다 — Spawning/Loading 단계(모델이 아직 준비되기 전)에 Stop을
    // 받으면 조용히 무시되어, serve_wait_ready()가 포트 레이스로 실패한 뒤
    // 재시도하는 실제 GUI 시나리오에서 python 서버가 좀비로 남았다.
    let port = pick_free_port().expect("free port");
    let profile = test_profile();
    let handle = LifecycleHandle::spawn();

    let start = StartParams {
        profile,
        context: 1024,
        mem_limit_gb: None,
        port: Some(port),
        python_dir: fixture_root().join("python"),
        child_spec: Some(fake_child_spec(port, "5")),
    };
    handle
        .command_tx
        .send(Command::Start(start))
        .await
        .expect("start send");

    // Ready에 도달하기 전(Spawning 또는 Loading)에 Stop을 걸어야 버그가 재현된다.
    let mut state_rx = handle.state_rx.clone();
    timeout(Duration::from_secs(5), async {
        loop {
            let state = *state_rx.borrow();
            if matches!(state, LifecycleState::Spawning | LifecycleState::Loading) {
                return;
            }
            state_rx.changed().await.expect("state channel closed");
        }
    })
    .await
    .expect("should reach Spawning/Loading before Ready");
    assert_ne!(*handle.state_rx.borrow(), LifecycleState::Ready);

    handle
        .command_tx
        .send(Command::Stop)
        .await
        .expect("stop send");

    timeout(
        Duration::from_secs(5),
        handle.wait_for_state(LifecycleState::Idle),
    )
    .await
    .expect("stop during loading should still reach Idle, not hang forever");

    let client = Client::new();
    let resp = client
        .get(format!("http://127.0.0.1:{port}/health"))
        .send()
        .await;
    assert!(resp.is_err(), "server should be terminated, not left as a zombie");
}