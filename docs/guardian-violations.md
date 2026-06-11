# Guardian Violation Report — AI_Dashboard

## [VIOLATION-001] Severity: P1 — TIMEOUT
- **Discovery point**: Pre-scan + Phase 0 (8min polling window 09:37–09:45)
- **Violating agent**: license-advisor
- **Violation type**: deliverable omission (timeout)
- **Violation details**: docs/tech-stack.md 와 docs/license-report.md 가 8분 폴링 윈도우 내 생성되지 않음. license-advisor 산출물 2종 전부 부재.
- **Related file**: docs/tech-stack.md (없음), docs/license-report.md (없음)
- **Original requirement**: 요구사항 8(스택 확정), .dmg 지인 배포 라이선스 판정
- **Correction order**: license-advisor 재스폰 또는 진행 상태 확인 필요. tech-stack.md는 명시 스택(Rust메인/python3.12+uv/Flutter macOS/vllm-mlx) 임의 변경 금지, license-report.md는 .dmg 지인 배포 목적 라이선스 판정 포함.
- **Status**: OPEN

## [VIOLATION-002] Severity: P2 — 요구사항 11 미반영 (GAP-2)
- **Discovery point**: Pre-scan, decisions.md 검토
- **Violating agent**: doc-pre-scanner
- **Violation type**: requirement omission
- **Violation details**: 요구사항 11("사용자 본인 환경 기준 성능·자원소모를 직관적으로 추가 확인할 기능을 리더가 분석·제안, 시각화 우대")가 decisions.md 결정점으로 명시 추출되지 않음. N-08(비교 차트)이 부분 인접하나 "리더가 추가 기능을 분석·제안하라"는 능동 액션 항목이 누락됨.
- **Related file**: docs/decisions.md
- **Original requirement**: 요구사항 11
- **Correction order**: 리더가 Phase 0.5(/office-hours, /plan-ceo-review) 또는 별도 단계에서 "사용자 환경 기준 추가 확인 기능 분석·제안"을 능동 수행해야 함. decisions.md에 해당 액션 항목 추가 권장.
- **Status**: OPEN

## [VIOLATION-003] Severity: P2 — 요구사항 10 부분 반영 (GAP-1)
- **Discovery point**: Pre-scan, decisions.md 검토
- **Violation type**: requirement partial coverage
- **Violation details**: 요구사항 10의 한 사이클 lifecycle(버튼→subprocess 서버기동→테스트→종료버튼→모델언로드+정리+서버 자동종료)이 B-09(A안 Rust subprocess 관리)에서 부분적으로만 함의됨. lifecycle 자체를 독립 결정점/스펙 항목으로 명시하지 않음.
- **Related file**: docs/decisions.md
- **Original requirement**: 요구사항 10
- **Correction order**: Phase 1 기술 스펙(spec.md) 또는 Planning Gate 모듈 설계에서 lifecycle 상태머신(기동→테스트→종료→언로드→정리→서버종료)을 명시 항목으로 다룰 것.
- **Status**: OPEN (Phase 1에서 해소 가능)

## PASS items
- 명시 스택 임의 변경 없음 확인: decisions.md N-06(uv 명시 준수), B-08/B-09(Rust 메인 의도 유지), B-13(.dmg) — 스택을 임의로 다른 언어/프레임워크로 바꾼 흔적 없음. PASS.
- 모든 백엔드 변경 후보가 "결정점(OPEN) + 추천안" 형태로 제시됨 = 사용자 결정 사항으로 표기됨. 임의 확정 없음. PASS.
- vllm-mlx 모호성(B-01)을 정확히 포착하고 MLX/mlx-community 호환을 추천 기준으로 명시. PASS.
- Qwen KV캐시 차이 예외처리 금지(요구사항 13)를 B-17로 정확히 포착, 메타데이터 자동감지 추천(예외처리 아님). PASS.
