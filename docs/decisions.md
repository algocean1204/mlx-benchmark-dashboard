# AI Dashboard — Decision Record

**프로젝트**: AI_Dashboard (macOS 전용 로컬 AI 벤치마킹 데스크탑 앱)
**스캔 일시**: 2026-06-10
**총 결정점**: 26개 (BLOCKING 17 / NON-BLOCKING 9)

---

## BLOCKING DECISIONS — 구현 시작 전 반드시 결정

### [B-01] MLX 추론 서버 백엔드 실체
**맥락**: 스펙이 "vllm-mlx"를 명시했으나 이 이름의 공식 단일 프로젝트가 불분명함.
- A) **mlx-lm 서버 모드** (`mlx_lm.server` — mlx-community 허브 공식 지원, HF 허브 직접 연동) ← 추천 (mlx-community 모델과 완벽 호환, 공식 유지보수)
- B) **vLLM의 MLX 백엔드** (vLLM 0.4+ MLX backend, OpenAI 호환 API)
- C) **Ollama MLX 모드** (단순하지만 TPS·메모리 세밀 측정 어려움)
- D) **llama.cpp MLX 빌드** (Metal 가속 llama.cpp, mlx-lm 대비 생태계 작음)

### [B-02] CPU 모드 백엔드
**맥락**: "MLX가 아니거나 CPU에서 성능 보고 싶으면 CPU 모드" — 백엔드 미정.
- A) **llama.cpp / llama-cpp-python** (CPU + Metal 혼합, 가장 넓은 GGUF 지원) ← 추천 (CPU 벤치마킹 표준, Python 바인딩으로 래핑 용이)
- B) **Hugging Face transformers CPU** (PyTorch CPU, 정확하나 느림)
- C) **vLLM CPU 모드** (실험적, macOS 지원 미성숙)
- D) **A+B 동시 지원** (선택폭 최대, 구현 복잡도 2배)

### [B-03] 이미지 생성 모델 백엔드
**맥락**: "이미지 생성 모델" 지원 명시, 백엔드 미정.
- A) **mflux** (Flux 계열 MLX 포팅, M 시리즈 최적화) ← 추천 (Apple Silicon 최적화, 활발한 유지보수)
- B) **Stable Diffusion MLX 포팅** (sdxl-turbo-mlx 등)
- C) **diffusers + MPS** (HuggingFace diffusers, Metal 가속)
- D) **v1 제외 → v2 이후 추가**

### [B-04] 음성 TTS 백엔드
**맥락**: "음성(TTS)" 지원 명시, 구체적 라이브러리 미정.
- A) **mlx-audio** (MLX 기반 TTS, Apple Silicon 최적화) ← 추천 (MLX 생태계 통일성)
- B) **Kokoro MLX** (고품질 TTS, mlx-community 배포)
- C) **macOS AVSpeechSynthesizer** (내장 TTS, 성능 측정 의미 없음)
- D) **v1 제외**

### [B-05] 음성 ASR 백엔드
**맥락**: "음성(ASR)" 지원 명시, 구체적 라이브러리 미정.
- A) **mlx-whisper** (MLX Whisper, mlx-community 허브 배포) ← 추천 (MLX 생태계 통일, 실시간 TPS 측정 가능)
- B) **whisper.cpp** (Metal 가속, C++ 기반)
- C) **openai-whisper CPU** (레퍼런스 구현, 느림)
- D) **v1 제외**

### [B-06] 멀티모달/이미지 분석 백엔드
**맥락**: "멀티모달, 이미지 분석" 지원 명시.
- A) **mlx-vlm** (MLX 기반 VLM, LLaVA/Idefics 등 지원) ← 추천 (MLX 생태계, mlx-community 모델 직결)
- B) **llama.cpp 멀티모달 확장** (LLaVA GGUF 포맷)
- C) **transformers VLM CPU** (범용이나 속도 저하)

### [B-07] v1 스코프 — 지원 모달리티 범위
**맥락**: LLM/멀티모달/이미지생성/TTS/ASR 5가지 유형 중 v1 포함 범위 미결.
- A) **LLM + 멀티모달만 (2종)** — 핵심 기능 먼저, 빠른 출시 ← 추천 (B-01~B-02 백엔드 안정화 우선, 나머지는 플러그인 구조로 추후 추가)
- B) **LLM + 멀티모달 + ASR (3종)** — 음성 입력까지 포함
- C) **5종 전부 v1** — 완전한 기능, 개발 기간 최장
- D) **LLM만 (1종)** — 가장 빠른 MVP

### [B-08] Rust ↔ Flutter 통신 방식
**맥락**: "내부 메인은 Rust, Flutter 맥용 데스크탑앱" — 연동 방식 미정.
- A) **flutter_rust_bridge FFI** (pub.dev 공식 라이브러리, Dart↔Rust 타입 안전 바인딩) ← 추천 (타입 안전, 직접 메모리 공유, 지연 최소)
- B) **로컬 TCP/Unix 소켓 IPC** (Rust가 HTTP/gRPC 서버, Flutter가 클라이언트)
- C) **Platform Channel + MethodChannel** (Flutter 네이티브 채널, Rust를 dylib로 로드)

### [B-09] Rust ↔ Python 경계 정의
**맥락**: "내부 메인은 Rust, 파이썬은 주로 래핑만" — 경계 불명확.
- A) **Rust: 프로세스 관리·모니터링·DB·IPC / Python: 모든 모델 서버 실행·추론 래핑** ← 추천 (스펙 의도와 일치, Rust가 subprocess로 Python 프로세스 관리)
- B) **Rust: 모니터링 데몬만 / Python+Flutter: 나머지** (Rust 역할 최소화)
- C) **Rust: 추론 엔진 일부 직접 구현 + 모니터링** (Rust 비중 최대, 개발 복잡도 높음)

### [B-10] 메모리 자동 언로드 임계값 정책
**맥락**: "메모리가 넘거나 넘을 것 같으면 빠르게 모델 내림" — 기준 미정.
- A) **절대값 고정** (예: 물리 RAM의 85% 초과 시 언로드) — 단순, 예측 가능
- B) **비율 기반 + 사용자 조정 가능** (기본 80%, UI 슬라이더로 50~95% 조정) ← 추천 (48GB 환경 기본값과 소용량 맥 환경 모두 대응)
- C) **예측 기반** (모델 로드 전 예상 사용량 계산 후 사전 차단)
- D) **B+C 복합** (사전 차단 + 실시간 임계값)

### [B-11] macOS 통합 메모리 실측 방법
**맥라**: "실측 필수" 명시 — Apple Silicon unified memory는 일반 RSS로 측정 불충분.
- A) **`task_info` / `proc_pidinfo` (RSS)** — 프로세스 물리 메모리, Metal 공유 메모리 미포함
- B) **`vm_stat` + Metal 사용량 (`MTLDevice.currentAllocatedSize`)** ← 추천 (GPU/Neural Engine 공유 메모리까지 포착, unified memory 실체에 가장 근접)
- C) **`mach_task_basic_info` phys_footprint** (macOS 전용, compressed memory 포함)
- D) **B+C 복합** (가장 정확, 구현 복잡도 최고)

### [B-12] DB 선택
**맥락**: "간단한 DB로 테스트 모델명과 실제 데이터 저장" — 구체적 선택 미정.
- A) **SQLite (rusqlite 크레이트)** ← 추천 (단일 파일, 추가 서버 불필요, Rust 생태계 1위, 로컬 앱 표준)
- B) **DuckDB** (분석 쿼리 최적화, 차트 집계 쿼리 빠름)
- C) **sled** (순수 Rust 임베디드 KV, 스키마 없음 — 비교 차트에 부적합)

### [B-13] .dmg 서명 및 노터라이즈 방침
**맥라**: "지인에게만 .dmg 전달" — Gatekeeper 처리 방식 미결.
- A) **노터라이즈 없이 배포 + 수동 Gatekeeper 우회 안내** (xattr -cr 명령, 비용 없음) ← 추천 (지인 소규모 배포, Apple Developer 계정 비용 절감)
- B) **Apple Developer 계정으로 서명+노터라이즈** (연 $99, Gatekeeper 자동 통과)
- C) **임시: 노터라이즈 없이 배포, 추후 계정 취득 시 업그레이드**

### [B-14] 모델 다운로드 UX (HF Hub 연동 여부)
**맥라**: 모델 획득 방법 전혀 미언급.
- A) **앱 내 HF Hub 검색·다운로드 UI 내장** (모델명 검색 → 다운로드 → 자동 등록) ← 추천 (UX 완결성, mlx-community 필터 적용 가능)
- B) **로컬 경로 수동 지정만** (사용자가 이미 다운받은 경로를 입력)
- C) **A+B 동시 지원** (최고 유연성, 구현 공수 최대)

### [B-15] TPS 측정 기준 표준화
**맥라**: "토큰속도(TPS) 측정" 명시, prefill/decode 분리 여부 및 프롬프트 표준 미정.
- A) **prefill TPS + decode TPS 분리 측정** ← 추천 (모델 특성 차이를 가장 정확히 드러냄, 모델 간 비교에 필수)
- B) **end-to-end TPS만** (단순, 사용자 체감과 유사)
- C) **A+B 동시 표시** (가장 풍부하나 UI 복잡)

### [B-16] 고정 벤치마크 프롬프트 세트 포함 여부
**맥라**: "채팅·코드작성·이미지분석으로 실제 성능 테스트" — 표준 프롬프트 세트 제공 여부 미결.
- A) **앱 내장 표준 프롬프트 세트 제공** (카테고리별 5~10개 고정 프롬프트, 재현 가능) ← 추천 (모델 간 공정 비교의 전제조건)
- B) **사용자 자유 입력만** (유연하나 비교 의미 희석)
- C) **A+B 동시** (내장 프롬프트 + 자유 입력)

### [B-17] Qwen 하이브리드 캐싱 등 모델별 KV 캐시 차이 일반화 전략
**맥라**: "Qwen은 하이브리드 캐싱, llama는 아님 — 예외 두지 말고" — 구체적 구현 전략 미결.
- A) **모델 메타데이터 기반 자동 감지** (config.json의 `model_type`/`architectures` 필드 파싱 → 캐시 전략 자동 선택) ← 추천 (확장 가능, 신규 모델 추가 시 코드 변경 최소)
- B) **화이트리스트 방식** (Qwen/Llama/Mistral 등 알려진 아키텍처별 하드코딩 분기)
- C) **백엔드 위임** (mlx-lm / llama.cpp가 내부적으로 처리하도록 전달, Rust는 개입 안 함)

---

## NON-BLOCKING DECISIONS — 기본값 제안 후 진행 가능

### [N-01] 컨텍스트 길이 조정 UI 방식
**기본 제안**: 슬라이더 + 수동 입력 필드 병행 (512~128K 토큰 범위). 모델별 최대값 자동 제한.
**확정 전 진행 가능 여부**: 가능 (UI 세부는 Phase 1.5 디자인 단계에서 결정)

### [N-02] 시각화 차트 라이브러리 (Flutter)
**기본 제안**: `fl_chart` (Flutter 생태계 1위, MIT, 라인·바·레이더 차트 지원).
**확정 전 진행 가능 여부**: 가능 (Phase 2 구현 전 확정으로 충분)

### [N-03] 실시간 모니터링 폴링 주기
**기본 제안**: 500ms 폴링 (Rust 사이드카, 메모리·CPU·TPS 갱신). 임계값 근접 시 100ms로 자동 전환.
**확정 전 진행 가능 여부**: 가능 (튜닝 값, 구현 후 조정)

### [N-04] DB 스키마 핵심 테이블 구성
**기본 제안**: `models`(id, name, type, backend, path) / `sessions`(id, model_id, started_at) / `benchmarks`(id, session_id, prompt_type, tps_prefill, tps_decode, ram_peak_mb, cpu_peak_pct, duration_ms, timestamp).
**확정 전 진행 가능 여부**: 가능 (Planning Gate ERD 단계에서 확정)

### [N-05] 앱 아이콘 및 브랜드명
**기본 제안**: "AI Bench" 임시명. 아이콘은 Phase 1.5 디자인 팀 산출물.
**확정 전 진행 가능 여부**: 가능

### [N-06] Python 환경 관리 방식
**기본 제안**: `uv venv` (스펙 명시 `uv` 사용). Rust subprocess가 `uv run` 으로 Python 프로세스 실행.
**확정 전 진행 가능 여부**: 가능 (스펙에 uv 명시됨)

### [N-07] 자동 언로드 후 사용자 알림 방식
**기본 제안**: macOS 네이티브 알림(UserNotifications) + 앱 내 토스트 메시지 병행.
**확정 전 진행 가능 여부**: 가능

### [N-08] 모델 간 비교 차트 유형
**기본 제안**: 레이더 차트(종합) + 바 차트(TPS/RAM/CPU 개별) + 시계열 라인 차트(세션 내 추이). 사용자가 비교할 모델 최대 4개 선택.
**확정 전 진행 가능 여부**: 가능 (디자인 단계에서 구체화)

### [N-09] Grok Build CLI 연동 방식 (개발 워크플로우)
**기본 제안**: `grok -p` headless 모드로 Composer 2.5 사용. Claude는 코드 검증·지시 전담, 직접 코드 수정은 3줄 미만으로 제한.
**확정 전 진행 가능 여부**: 가능 (이미 스펙에 명시됨)

---

## 결정 상태 추적

| ID | 항목 | 상태 | 결정값 |
|----|------|------|--------|
| B-01 | MLX 추론 서버 백엔드 | OPEN | — |
| B-02 | CPU 모드 백엔드 | OPEN | — |
| B-03 | 이미지 생성 백엔드 | OPEN | — |
| B-04 | TTS 백엔드 | OPEN | — |
| B-05 | ASR 백엔드 | OPEN | — |
| B-06 | 멀티모달 백엔드 | OPEN | — |
| B-07 | v1 스코프 모달리티 범위 | OPEN | — |
| B-08 | Rust↔Flutter 통신 방식 | OPEN | — |
| B-09 | Rust↔Python 경계 | OPEN | — |
| B-10 | 메모리 언로드 임계값 정책 | OPEN | — |
| B-11 | 통합 메모리 실측 방법 | OPEN | — |
| B-12 | DB 선택 | OPEN | — |
| B-13 | .dmg 서명·노터라이즈 | OPEN | — |
| B-14 | HF Hub 연동 여부 | OPEN | — |
| B-15 | TPS 측정 기준 | OPEN | — |
| B-16 | 표준 프롬프트 세트 | OPEN | — |
| B-17 | KV 캐시 일반화 전략 | OPEN | — |
| N-01 | 컨텍스트 조정 UI | DEFAULT | 슬라이더+수동입력 |
| N-02 | Flutter 차트 라이브러리 | DEFAULT | fl_chart |
| N-03 | 모니터링 폴링 주기 | DEFAULT | 500ms / 임계 근접 시 100ms |
| N-04 | DB 스키마 핵심 테이블 | DEFAULT | models/sessions/benchmarks |
| N-05 | 앱명·아이콘 | DEFAULT | "AI Bench" (임시) |
| N-06 | Python 환경 관리 | DEFAULT | uv venv |
| N-07 | 자동 언로드 알림 | DEFAULT | macOS 알림 + 토스트 |
| N-08 | 비교 차트 유형 | DEFAULT | 레이더+바+라인 |
| N-09 | Grok Build 연동 | DEFAULT | grok -p headless |
