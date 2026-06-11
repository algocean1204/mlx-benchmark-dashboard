# Guardian Requirements Log — AI_Dashboard

## User Original Requirements
1. 맥 전용 데스크탑앱: 로컬 AI 모델 성능 벤치마킹 (챗봇/코드작성/이미지분석 등)
2. 두 가지 실행 방식: (a) vllm-mlx — MLX 기반이거나 mlx-community 모델만 허용, (b) CPU 모드 — 비-MLX 모델이거나 CPU 성능 확인 목적
3. 측정 항목: 토큰속도(TPS), 컨텍스트별 실제 RAM 사용량(반드시 실측), CPU 사용량
4. 채팅·코드작성·이미지분석 등으로 실제 성능 직접 테스트·측정
5. 세밀한 컨텍스트 조정 + 쉬운 모델 변경
6. 모델 유형별 맞춤 테스트 환경: 멀티모달, 이미지 전용, 이미지 생성, 음성(TTS/ASR), LLM(텍스트·채팅 전용)
7. 직관적 UI — 누가 봐도 쉽게 기능별 구성·사용 가능
8. 아키텍처: 내부 메인 Rust, Python은 래핑 위주(python3.12 + uv), Flutter 맥 데스크탑앱, .dmg 배포(지인 전용)
9. 실시간 자원 모니터링 + 빠른 자동 제어: 메모리 초과 임박 시 Rust가 즉시 모델 언로드 (테스트용 앱이라 급정지 허용)
10. 한 사이클: 버튼 → 서버 기동(subprocess) → 테스트 → 종료 버튼 → 모델 언로드 + 정리 + 서버 자동 종료
11. 사용자 본인 환경 기준 성능·자원소모를 직관적으로 추가 확인할 기능 분석·추가 (시각화 우대) — 리더가 분석해서 제안해야 함
12. 내장 간단 DB: 모델명 + 실측 데이터 저장 → 모델 간 차트 비교·시각화
13. Qwen 하이브리드 캐싱 vs llama 등 모델별 차이를 예외 처리로 덮지 말고 실작동 + 실시간 최대한 빠른 제어로 해결
14. 코드 작성 주체: Grok Build CLI Composer 2.5 (grok -p headless). Claude는 검증·지시·미세수정만.

## Current Phase: Pre-scan + Phase 0
## Phase Goal: 요구사항 결정점 추출(decisions.md) + 기술 스택 확정(tech-stack.md) + 라이선스 판정(license-report.md)
## Active Agents: doc-pre-scanner, license-advisor

## Critical Requirements (Intervene immediately if omitted)
- [CRITICAL] RAM 사용량은 반드시 실측 (요구사항 3) — 추정/하드코딩 금지
- [CRITICAL] vllm-mlx 모드는 MLX 기반/mlx-community 모델만 허용 (요구사항 2a)
- [CRITICAL] 아키텍처 스택 임의 변경 금지: Rust 메인 + python3.12+uv 래핑 + Flutter macOS + .dmg (요구사항 8) — 변경은 사용자 결정으로만
- [CRITICAL] 모델별 차이(Qwen 캐싱 등)를 예외처리로 덮지 말 것 (요구사항 13)
- [CRITICAL] 코드 작성 주체 = Grok Build CLI Composer 2.5 / Claude는 검증·지시·미세수정만 (요구사항 14)
- [CRITICAL] 6종 모델 유형별 맞춤 테스트 환경 전부 포함 (요구사항 6)
- [CRITICAL] 메모리 초과 임박 시 Rust 즉시 언로드 (요구사항 9)

## Standard Requirements (Verify before Phase completion)
- 5종 측정/테스트 워크플로 (채팅/코드/이미지분석), 세밀 컨텍스트 조정, 쉬운 모델 변경
- 한 사이클 lifecycle (subprocess 기동→테스트→종료→언로드+정리+서버종료)
- 내장 DB + 모델 간 차트 비교·시각화
- 사용자 환경 기준 추가 확인 기능 분석·제안 (시각화 우대)
- 직관적 UI

## Pre-scan Phase 0 Verification Targets
- decisions.md: 14개 요구사항의 모호점 전부 짚었는가 (CPU모드 백엔드 미정 / 이미지생성·TTS·ASR 백엔드 미정 / v1 범위 등)
- tech-stack.md: 명시 스택 임의 변경 없는가 (변경 제안은 사용자 결정 사항으로 표기)
- license-report.md: .dmg 지인 배포 목적 라이선스 판정했는가

---

## Verification Pass 1 — Pre-scan + Phase 0 (2026-06-10 09:45)

### Deliverable status
- decisions.md: PRESENT (12.3KB, 26 decisions: 17 BLOCKING / 9 NON-BLOCKING)
- tech-stack.md: MISSING (TIMEOUT — license-advisor did not produce within 8min)
- license-report.md: MISSING (TIMEOUT — license-advisor did not produce within 8min)

### decisions.md requirement coverage map
- Req1 (벤치마킹 앱) → covered (project framing)
- Req2a (vllm-mlx, MLX/mlx-community 한정) → B-01
- Req2b (CPU 모드) → B-02
- Req3 (TPS/RAM실측/CPU) → B-11(RAM실측), B-15(TPS), N-03(CPU폴링) — covered
- Req4 (채팅/코드/이미지분석 실측) → B-16 standard prompt set — covered
- Req5 (컨텍스트 조정 + 쉬운 모델변경) → N-01, B-14 — covered
- Req6 (6종 유형) → B-03(이미지생성),B-04(TTS),B-05(ASR),B-06(멀티모달/이미지분석),LLM, B-07(v1범위) — covered
- Req7 (직관 UI) → N-01, N-05, deferred to Phase 1.5 — covered as deferred
- Req8 (Rust메인/py3.12+uv/Flutter/.dmg) → B-08,B-09,N-06,B-13 — covered
- Req9 (메모리 초과 임박 즉시 언로드) → B-10 — covered
- Req10 (lifecycle subprocess) → B-09(A: Rust subprocess) partial — see GAP-1
- Req11 (사용자 환경 추가 기능 분석·제안) → NOT explicitly addressed — see GAP-2
- Req12 (DB + 차트 비교) → B-12, N-04, N-08 — covered
- Req13 (모델별 차이 예외처리 금지) → B-17 — covered
- Req14 (Grok 주체 / Claude 검증) → N-09 — covered
