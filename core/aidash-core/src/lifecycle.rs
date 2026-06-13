//! 상태 머신 (스펙 3절). 모든 상태 전이의 단일 소유자
//!
//! IN: 명령(start/stop/abort), pyproc·watchdog 이벤트
//! OUT: 상태 전이 이벤트, 현재 상태 조회

use std::future::pending;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use reqwest::Client;
use serde::Serialize;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{sleep_until, Instant};

use crate::events::CoreEvent;
use crate::monitor::spawn_process_monitor;
use crate::profile::ModelProfile;
use crate::pyproc::{
    build_child_spec, is_port_free, pick_free_port, reap_process_group, spawn_child,
    terminate_abort, terminate_graceful, ChildSpec, PyprocEvent, SpawnedChild,
};
use crate::watchdog::{compute_limits, spawn_watchdog};

const SPAWNING_TIMEOUT_SECS: u64 = 30;
const STOPPING_GRACE_SECS: u64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    Idle,
    Spawning,
    Loading,
    Ready,
    Busy,
    Stopping,
    Killing,
    Cleanup,
}

#[derive(Debug, Clone)]
pub struct StartParams {
    pub profile: ModelProfile,
    pub context: u32,
    pub mem_limit_gb: Option<f64>,
    pub port: Option<u16>,
    pub python_dir: PathBuf,
    /// 테스트용: 지정 시 고수준 ChildSpec 빌더를 우회한다.
    pub child_spec: Option<ChildSpec>,
}

#[derive(Debug, Clone)]
pub enum Command {
    Start(StartParams),
    Stop,
    Abort { reason: String },
    BeginWork,
    EndWork,
}

pub struct LifecycleHandle {
    pub command_tx: mpsc::Sender<Command>,
    pub state_rx: watch::Receiver<LifecycleState>,
    pub port_rx: watch::Receiver<Option<u16>>,
    pub event_tx: broadcast::Sender<CoreEvent>,
}

impl LifecycleHandle {
    pub fn spawn() -> Self {
        let (command_tx, command_rx) = mpsc::channel(32);
        let (state_tx, state_rx) = watch::channel(LifecycleState::Idle);
        let (port_tx, port_rx) = watch::channel(None);
        let (event_tx, _) = broadcast::channel(256);
        let event_tx_actor = event_tx.clone();

        tokio::spawn(run_lifecycle_actor(
            command_rx,
            command_tx.clone(),
            event_tx_actor,
            state_tx,
            port_tx,
        ));

        Self {
            command_tx,
            state_rx,
            port_rx,
            event_tx,
        }
    }

    pub async fn wait_for_state(&self, target: LifecycleState) {
        let mut rx = self.state_rx.clone();
        while *rx.borrow() != target {
            if rx.changed().await.is_err() {
                break;
            }
        }
    }
}

struct ActiveChild {
    spawned: SpawnedChild,
    monitor_task: JoinHandle<()>,
    watchdog_task: JoinHandle<()>,
}

struct LifecycleContext {
    state: LifecycleState,
    state_tx: watch::Sender<LifecycleState>,
    port_tx: watch::Sender<Option<u16>>,
    event_tx: broadcast::Sender<CoreEvent>,
    command_tx: mpsc::Sender<Command>,
    active: Option<ActiveChild>,
    port: Option<u16>,
    mem_limit_gb: Option<f64>,
    load_timeout_sec: u64,
    http: Client,
    spawning_deadline: Option<Instant>,
    loading_deadline: Option<Instant>,
    stopping_deadline: Option<Instant>,
}

pub async fn run_lifecycle_actor(
    mut command_rx: mpsc::Receiver<Command>,
    command_tx: mpsc::Sender<Command>,
    event_tx: broadcast::Sender<CoreEvent>,
    state_tx: watch::Sender<LifecycleState>,
    port_tx: watch::Sender<Option<u16>>,
) {
    let mut ctx = LifecycleContext {
        state: LifecycleState::Idle,
        state_tx,
        port_tx,
        event_tx: event_tx.clone(),
        command_tx,
        active: None,
        port: None,
        mem_limit_gb: None,
        load_timeout_sec: 600,
        http: Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap_or_else(|_| Client::new()),
        spawning_deadline: None,
        loading_deadline: None,
        stopping_deadline: None,
    };

    let mut health_interval = tokio::time::interval(Duration::from_millis(200));
    health_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        let pyproc_fut: Pin<Box<dyn std::future::Future<Output = Option<PyprocEvent>> + Send>> =
            if let Some(active) = ctx.active.as_mut() {
                Box::pin(async {
                    active.spawned.event_rx.recv().await
                })
            } else {
                Box::pin(async { pending().await })
            };

        let spawning_deadline = ctx.spawning_deadline;
        let loading_deadline = ctx.loading_deadline;
        let stopping_deadline = ctx.stopping_deadline;

        tokio::select! {
            cmd = command_rx.recv() => {
                match cmd {
                    Some(Command::Start(params)) => {
                        if ctx.state == LifecycleState::Idle {
                            if let Err(()) = ctx.handle_start(params, &event_tx).await {
                                let _ = ctx.enter_cleanup().await;
                            }
                        }
                    }
                    Some(Command::Stop) => {
                        ctx.handle_stop().await;
                    }
                    Some(Command::Abort { reason: _ }) => {
                        if !matches!(ctx.state, LifecycleState::Idle | LifecycleState::Killing | LifecycleState::Cleanup) {
                            let _ = ctx.enter_killing().await;
                        }
                    }
                    Some(Command::BeginWork) => {
                        if ctx.state == LifecycleState::Ready {
                            ctx.transition(LifecycleState::Busy);
                        }
                    }
                    Some(Command::EndWork) => {
                        if ctx.state == LifecycleState::Busy {
                            ctx.transition(LifecycleState::Ready);
                        }
                    }
                    None => break,
                }
            }
            pyproc_event = pyproc_fut => {
                if let Some(event) = pyproc_event {
                    ctx.handle_pyproc_event(event).await;
                }
            }
            _ = health_interval.tick(), if matches!(ctx.state, LifecycleState::Spawning | LifecycleState::Loading) => {
                if let Some(port) = ctx.port {
                    match poll_health(&ctx.http, port).await {
                        HealthPoll::Unreachable => {}
                        HealthPoll::Responding { model_loaded, .. } => {
                            if ctx.state == LifecycleState::Spawning {
                                ctx.transition(LifecycleState::Loading);
                                ctx.spawning_deadline = None;
                                ctx.loading_deadline = Some(
                                    Instant::now() + Duration::from_secs(ctx.load_timeout_sec),
                                );
                            }
                            if ctx.state == LifecycleState::Loading && model_loaded {
                                ctx.transition(LifecycleState::Ready);
                                ctx.loading_deadline = None;
                            }
                        }
                    }
                }
            }
            _ = sleep_until(spawning_deadline.unwrap_or_else(Instant::now)), if spawning_deadline.is_some() && ctx.state == LifecycleState::Spawning => {
                let _ = ctx.enter_killing().await;
            }
            _ = sleep_until(loading_deadline.unwrap_or_else(Instant::now)), if loading_deadline.is_some() && ctx.state == LifecycleState::Loading => {
                let _ = ctx.enter_killing().await;
            }
            _ = sleep_until(stopping_deadline.unwrap_or_else(Instant::now)), if stopping_deadline.is_some() && ctx.state == LifecycleState::Stopping => {
                if let Some(active) = ctx.active.as_ref() {
                    terminate_abort(active.spawned.handle.pgid).await;
                }
                let _ = ctx.enter_cleanup().await;
            }
        }
    }
}

enum HealthPoll {
    Unreachable,
    Responding {
        model_loaded: bool,
        status: Option<String>,
    },
}

#[derive(serde::Deserialize)]
struct HealthResponse {
    #[serde(default)]
    status: Option<String>,
    model_loaded: bool,
}

async fn poll_health(client: &Client, port: u16) -> HealthPoll {
    let url = format!("http://127.0.0.1:{port}/health");
    match client.get(&url).send().await {
        Ok(resp) => {
            if let Ok(body) = resp.json::<HealthResponse>().await {
                HealthPoll::Responding {
                    model_loaded: body.model_loaded,
                    status: body.status,
                }
            } else {
                HealthPoll::Responding {
                    model_loaded: false,
                    status: None,
                }
            }
        }
        Err(_) => HealthPoll::Unreachable,
    }
}

const WAIT_READY_POLL_MS: u64 = 500;

/// `/health`를 0.5초 간격으로 폴링해 `model_loaded=true` 또는 `Ready` 상태까지 대기한다.
pub async fn wait_model_ready(
    state_rx: &mut watch::Receiver<LifecycleState>,
    port: u16,
    timeout: Duration,
) -> Result<(), String> {
    let http = Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|e| e.to_string())?;

    let deadline = Instant::now() + timeout;
    let poll_interval = Duration::from_millis(WAIT_READY_POLL_MS);
    let mut saw_active = false;

    loop {
        let state = *state_rx.borrow();
        if state == LifecycleState::Ready {
            return Ok(());
        }
        if matches!(
            state,
            LifecycleState::Spawning | LifecycleState::Loading | LifecycleState::Busy
        ) {
            saw_active = true;
        }
        if saw_active
            && matches!(
                state,
                LifecycleState::Idle | LifecycleState::Cleanup | LifecycleState::Killing
            )
        {
            return Err("모델 서버가 예기치 않게 종료되었습니다".into());
        }

        match poll_health(&http, port).await {
            HealthPoll::Responding {
                model_loaded: true,
                ..
            } => return Ok(()),
            HealthPoll::Responding {
                model_loaded: false,
                status: Some(s),
            } if s == "error" => {
                return Err("모델 로드 중 오류가 발생했습니다".into());
            }
            HealthPoll::Unreachable | HealthPoll::Responding { .. } => {}
        }

        if Instant::now() >= deadline {
            return Err(format!(
                "모델 로드 시간 초과 ({}초) — 다시 시도하거나 다른 모델을 선택하세요",
                timeout.as_secs()
            ));
        }

        tokio::select! {
            _ = tokio::time::sleep(poll_interval) => {}
            changed = state_rx.changed() => {
                if changed.is_err() {
                    return Err("모델 서버 상태 채널이 닫혔습니다".into());
                }
            }
        }
    }
}

impl LifecycleContext {
    fn transition(&mut self, to: LifecycleState) {
        let from = self.state;
        if from == to {
            return;
        }
        self.state = to;
        let _ = self.state_tx.send(to);
        let _ = self.event_tx.send(CoreEvent::StateChanged { from, to });
    }

    async fn handle_start(
        &mut self,
        params: StartParams,
        event_tx: &broadcast::Sender<CoreEvent>,
    ) -> Result<(), ()> {
        self.mem_limit_gb = params.mem_limit_gb;
        self.load_timeout_sec = params.profile.load_timeout_sec;

        let port = match params.port {
            Some(port) => port,
            None => pick_free_port().map_err(|_| ())?,
        };
        self.port = Some(port);
        let _ = self.port_tx.send(Some(port));

        self.transition(LifecycleState::Spawning);
        self.spawning_deadline =
            Some(Instant::now() + Duration::from_secs(SPAWNING_TIMEOUT_SECS));

        let spec = if let Some(spec) = params.child_spec {
            spec
        } else {
            build_child_spec(&params.python_dir, &params.profile, params.context, port)
                .map_err(|_| ())?
        };

        let spawned = spawn_child(spec, port).map_err(|_| ())?;
        let pid = spawned.handle.pid;
        let limits = compute_limits(params.mem_limit_gb);

        let monitor_task = spawn_process_monitor(pid, Some(port), event_tx.clone());
        let sample_rx = event_tx.subscribe();
        let watchdog_task = spawn_watchdog(
            sample_rx,
            event_tx.clone(),
            self.command_tx.clone(),
            limits,
        );

        self.active = Some(ActiveChild {
            spawned,
            monitor_task,
            watchdog_task,
        });

        Ok(())
    }

    async fn handle_stop(&mut self) {
        if matches!(self.state, LifecycleState::Ready | LifecycleState::Busy) {
            self.transition(LifecycleState::Stopping);
            if let Some(active) = self.active.as_ref() {
                let pgid = active.spawned.handle.pgid;
                tokio::spawn(async move {
                    terminate_graceful(pgid).await;
                });
                self.stopping_deadline =
                    Some(Instant::now() + Duration::from_secs(STOPPING_GRACE_SECS));
            } else {
                let _ = self.enter_cleanup().await;
            }
        }
    }

    async fn handle_pyproc_event(&mut self, event: PyprocEvent) {
        match event {
            PyprocEvent::Log { level, message, .. } => {
                let _ = self.event_tx.send(CoreEvent::Log { level, message });
            }
            PyprocEvent::Exited { code: _ } => {
                if matches!(
                    self.state,
                    LifecycleState::Stopping | LifecycleState::Killing
                ) {
                    let _ = self.enter_cleanup().await;
                } else if !matches!(
                    self.state,
                    LifecycleState::Cleanup | LifecycleState::Idle
                ) {
                    let _ = self.enter_cleanup().await;
                }
            }
        }
    }

    async fn enter_killing(&mut self) -> Result<(), ()> {
        self.spawning_deadline = None;
        self.loading_deadline = None;
        self.stopping_deadline = None;
        self.transition(LifecycleState::Killing);

        if let Some(active) = self.active.as_ref() {
            terminate_abort(active.spawned.handle.pgid).await;
        } else {
            let _ = self.enter_cleanup().await;
        }

        Ok(())
    }

    async fn enter_cleanup(&mut self) -> Result<(), ()> {
        self.spawning_deadline = None;
        self.loading_deadline = None;
        self.stopping_deadline = None;
        self.transition(LifecycleState::Cleanup);

        if let Some(active) = self.active.take() {
            active.monitor_task.abort();
            active.watchdog_task.abort();

            let handle = active.spawned.handle.clone();
            reap_process_group(handle.pgid, handle.pid, handle.port).await;

            if let Some(port) = self.port {
                for _ in 0..20 {
                    if is_port_free(port) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }

        self.port = None;
        let _ = self.port_tx.send(None);
        self.mem_limit_gb = None;
        self.transition(LifecycleState::Idle);
        Ok(())
    }
}