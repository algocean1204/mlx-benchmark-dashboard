# 라이선스 검토 보고서

AI_Dashboard Phase 0 산출물. .dmg 비상업 재배포 기준으로 검토함.
검토 기준일: 2026-06-10.

---

## 판정 요약

| 구분 | GREEN | YELLOW | RED |
|---|---|---|---|
| 프레임워크 / 라이브러리 | 전 항목 (21건) | 0건 | 0건 |
| 모델 라이선스 | 2건 | 2건 | 0건 |
| 생성 코드 (Grok Build CLI) | — | 1건 (미확인) | 0건 |

RED 없음. YELLOW 3건은 조치 사항 이행 시 문제없음.

---

## 프레임워크 / 라이브러리 판정

전 항목 GREEN. 세부 내용은 tech-stack.md 스택 표 참조.

---

## 모델 라이선스

앱에 모델을 동봉하지 않고 사용자가 직접 다운로드하는 구조이므로, 앱 자체에 모델 라이선스가 직접 적용되지는 않음. 그러나 앱 내에 모델별 라이선스 안내 UI를 포함하는 것이 best practice다.

| 모델 | 라이선스 | 주요 조건 | 판정 |
|---|---|---|---|
| Llama 3.x (mlx-community/Llama-3.2-\*) | Llama Community License | 700M MAU 이하 허용 / "Built with Llama" 표기 필요 | YELLOW |
| Qwen3 | Apache-2.0 | 제한 없음 | GREEN |
| Gemma 3 | Gemma Terms of Use | 라이선스 동봉 필요 | YELLOW |
| Mistral | Apache-2.0 | 제한 없음 | GREEN |

Llama 3.x: 700M MAU 이하 비상업 배포는 허용 범위 내이나 "Built with Llama" 표기가 의무임.
Gemma 3: 라이선스 파일을 앱 또는 배포본에 동봉해야 함.

---

## Grok Build CLI 생성 코드

| 항목 | 내용 |
|---|---|
| 도구 | Grok Build CLI (Composer 2.5) |
| 판정 | YELLOW (미확인) |
| 사유 | xAI 공식 ToS 페이지(x.ai/legal 등) 403 / 연결 거부로 직접 확인 불가. GitHub(xai-org/grok-build) 404. |
| 업계 표준 | 생성 코드 소유권은 사용자에게 귀속되는 것이 일반적이므로 문제없을 가능성이 높으나, 미확인 상태를 정직하게 표기함. |
| 권장 조치 | xAI 계정 로그인 후 ToS의 output 소유권 조항을 직접 확인할 것. |

---

## NOTICE / 고지 의무 (.dmg 재배포 시)

| 라이선스 유형 | 해당 항목 | 의무 내용 |
|---|---|---|
| Apache-2.0 | vllm-mlx, huggingface_hub, transformers (선택 시) | NOTICE 파일 동봉 필수 |
| BSD-3-Clause | Flutter | 저작권 고지 필수 — "Copyright 2014 The Flutter Authors" |
| MIT (다수) | tokio, axum, rusqlite, flutter_rust_bridge, fl_chart, window_manager, FastAPI, mlx, mlx-lm, mlx-vlm, mlx-whisper, mlx-audio, mflux, llama-cpp-python, uv 등 | 앱 내 "오픈소스 라이선스" 화면 또는 LICENSE.txt 동봉으로 충족 |
| Public Domain | SQLite | 고지 의무 없음 |

---

## 핵심 리스크 3건

**1. Grok Build CLI 생성 코드 재배포 조건 미확인**
xAI ToS를 직접 열람할 수 없어 생성 코드의 재배포 허용 여부가 공식 확인되지 않은 상태임. 사용자가 xAI 계정으로 로그인 후 ToS output 소유권 조항을 직접 확인해야 함. (YELLOW)

**2. Flutter 버전 업그레이드 필요**
현재 로컬 설치본 3.38.2를 최신 stable 3.44.0으로 업그레이드해야 함. 버전 정책상 최신 안정 버전 사용이 의무임.

**3. 모델 라이선스 안내 UI**
앱 자체에 모델 라이선스가 직접 적용되지는 않으나, Llama "Built with Llama" 표기 의무 및 Gemma 라이선스 동봉 요건을 충족하려면 앱 내 모델별 라이선스 안내 화면을 두는 것이 best practice다.
