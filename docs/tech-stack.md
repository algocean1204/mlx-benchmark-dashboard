# 기술 스택

AI_Dashboard — Apple Silicon 전용 로컬 AI 모델 벤치마킹 데스크탑 앱.
Rust 코어 + Python 3.12 래퍼 + Flutter macOS UI 구조다. .dmg로 비상업 재배포.

---

## 최종 확정 스택

| 영역 | 기술 | 버전 | 라이선스 | 판정 |
|---|---|---|---|---|
| 코어 시스템 | Rust | 1.87+ | MIT / Apache-2.0 | GREEN |
| 자원 모니터링 | sysinfo | 0.39.3 | MIT | GREEN |
| 비동기 런타임 | tokio | 1.52.3 | MIT | GREEN |
| HTTP 서버 (Rust) | axum | 0.8.9 | MIT | GREEN |
| 내장 DB | SQLite (Public Domain) + rusqlite 0.40.1 (MIT) | — | — | GREEN |
| UI | Flutter macOS desktop | 3.44.0 | BSD-3-Clause | GREEN |
| Rust-Flutter 브릿지 | flutter_rust_bridge | 2.12.0 | MIT | GREEN |
| 차트 | fl_chart | 1.2.0 | MIT | GREEN |
| 창 관리 | window_manager | 0.5.1 | MIT | GREEN |
| Python 래퍼 | Python 3.12 (사용자 고정) + uv 0.11.19 | — | PSF / MIT · Apache-2.0 | GREEN |
| 래핑 서버 | FastAPI | 0.136.3 | MIT | GREEN |
| 모델 다운로드 | huggingface_hub | 1.18.0 | Apache-2.0 | GREEN |
| MLX 추론 (기본) | vllm-mlx | 0.3.0 | Apache-2.0 | GREEN |
| MLX 추론 (대안) | mlx-lm (mlx_lm.server) | 0.31.3 | MIT | GREEN |
| MLX 프레임워크 | mlx | 0.31.2 | MIT | GREEN |
| 멀티모달 | mlx-vlm | 0.1.27 | MIT | GREEN |
| ASR | mlx-whisper | 0.4.3 | MIT | GREEN |
| TTS | mlx-audio | 0.4.4 | MIT | GREEN |
| 이미지 생성 | mflux | 0.18.0 | MIT | GREEN |
| CPU 모드 (추천) | llama-cpp-python 0.3.28 / llama.cpp llama-server | — | MIT | GREEN |
| CPU 모드 (대안) | HF transformers | 5.10.2 | Apache-2.0 | GREEN |

RED 항목 없음. 전 항목 GREEN.

---

## CPU 모드 추천 근거

llama.cpp (MIT)를 기본 CPU 모드로 추천함.

- .dmg 동봉이 가장 단순하고 CPU 벤치마킹 사실상 표준임
- transformers는 의존성이 무겁고 CPU 추론 속도가 느려 벤치마킹 목적에 부적합함

transformers는 필요 시 대안으로만 채택함.

---

## 주요 메모

**vllm-mlx 실체**
vLLM과 별개의 독립 PyPI 프로젝트다. vLLM에서 영감을 받아 연속 배치·페이지드 KV 캐시를 Apple Silicon MLX 위에 구현한 패키지이며, PyPI에서 직접 확인됨. vLLM 본체와 혼동하지 않도록 주의.

**버전 정책**
항상 최신 안정 버전을 사용함. 로컬 Flutter 설치본이 3.38.2이면 3.44.0으로 업그레이드해야 함. package.json 및 requirements.txt에 정확한 버전을 핀함(^ · ~ 표기 금지).

**코드 작성 도구**
Grok Build CLI (Composer 2.5)로 생성한 코드를 사용 중. 재배포 조건은 xAI ToS 확인이 필요한 상태로, license-report.md 리스크 항목을 참조.
