import 'package:app/app_shell.dart';
import 'package:app/screens/bench_screen.dart';
import 'package:app/screens/chat_screen.dart';
import 'package:app/screens/compare_screen.dart';
import 'package:app/screens/dashboard_screen.dart';
import 'package:app/screens/model_detail_screen.dart';
import 'package:app/screens/model_manage_screen.dart';
import 'package:app/screens/onboarding_screen.dart';
import 'dart:typed_data';

import 'package:app/services/aidash_api.dart';
import 'package:app/src/rust/api.dart';
import 'package:app/widgets/metric_label.dart';
import 'package:app/services/config_service.dart';
import 'package:app/theme/app_theme.dart';
import 'package:app/widgets/doctor_badge.dart';
import 'package:app/widgets/draft_badge.dart';
import 'package:app/widgets/tier_badge.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:provider/provider.dart';

Widget _wrap(Widget child, {AidashApi? api}) {
  return MultiProvider(
    providers: [
      Provider<AidashApi>.value(value: api ?? MockAidashApi()),
      ChangeNotifierProvider(
        create: (_) => ConfigService()..load(),
      ),
    ],
    child: MaterialApp(
      theme: AppTheme.light(),
      home: Scaffold(body: child),
    ),
  );
}

void main() {
  testWidgets('대시보드 리더보드에 모델과 TPS 등급이 표시된다', (tester) async {
    await tester.pumpWidget(_wrap(const DashboardScreen()));
    await tester.pumpAndSettle();

    expect(find.text('대시보드'), findsWidgets);
    expect(find.text('Qwen2.5 7B 4bit'), findsWidgets);
    expect(find.textContaining('이상적'), findsWidgets);
  });

  testWidgets('온보딩 화면에 doctor 요약 카드가 표시된다', (tester) async {
    await tester.pumpWidget(
      _wrap(OnboardingScreen(onComplete: () {})),
    );
    await tester.pumpAndSettle();

    expect(find.text('AI Dashboard 환경 점검'), findsOneWidget);
    expect(find.text('준비됨'), findsWidgets);
    expect(find.text('조치 필요'), findsWidgets);
    expect(find.textContaining('vllm-mlx'), findsOneWidget);
  });

  testWidgets('AppShell 네비게이션 레일에 8개 목적지가 있다', (tester) async {
    await tester.pumpWidget(_wrap(const AppShell()));
    await tester.pumpAndSettle();

    expect(find.text('대시보드'), findsWidgets);
    expect(find.text('모델'), findsOneWidget);
    expect(find.text('비교'), findsOneWidget);
    expect(find.text('벤치'), findsOneWidget);
    expect(find.text('채팅'), findsOneWidget);
    expect(find.text('설정'), findsOneWidget);
    expect(find.text('환경 점검'), findsOneWidget);
    expect(find.text('모델 관리'), findsOneWidget);
  });

  testWidgets('TierBadge가 Rust 등급 라벨을 표시한다', (tester) async {
    await tester.pumpWidget(
      _wrap(const TierBadge(decodeTps: 52.3)),
    );
    await tester.pumpAndSettle();

    expect(find.textContaining('이상적'), findsOneWidget);
    expect(find.text('🟢'), findsOneWidget);
  });

  testWidgets('삭제 확인 다이얼로그가 표시되고 취소할 수 있다', (tester) async {
    final run = MockAidashApi().runRows.first;

    await tester.pumpWidget(
      MaterialApp(
        theme: AppTheme.light(),
        home: Scaffold(
          body: Builder(
            builder: (context) => FilledButton(
              onPressed: () async {
                await showDialog<bool>(
                  context: context,
                  builder: (ctx) => AlertDialog(
                    title: const Text('런 삭제'),
                    content: Text(
                      '런 #${run.runId} (${run.displayName})을(를) 삭제할까요?',
                    ),
                    actions: [
                      TextButton(
                        onPressed: () => Navigator.pop(ctx, false),
                        child: const Text('취소'),
                      ),
                      FilledButton(
                        onPressed: () => Navigator.pop(ctx, true),
                        child: const Text('삭제'),
                      ),
                    ],
                  ),
                );
              },
              child: const Text('삭제 열기'),
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.text('삭제 열기'));
    await tester.pumpAndSettle();

    expect(find.text('런 삭제'), findsOneWidget);
    expect(find.text('취소'), findsOneWidget);

    await tester.tap(find.text('취소'));
    await tester.pumpAndSettle();

    expect(find.text('런 삭제'), findsNothing);
  });

  testWidgets('비교 화면에서 모델 1개 선택 시 컨텍스트별 비교가 표시된다', (tester) async {
    await tester.pumpWidget(_wrap(const CompareScreen()));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Qwen2.5 7B 4bit'));
    await tester.pumpAndSettle();

    expect(
      find.text('같은 모델의 컨텍스트별 측정 비교입니다.'),
      findsOneWidget,
    );
    expect(find.text('4K'), findsWidgets);
    expect(find.text('8K'), findsWidgets);
  });

  testWidgets('비교 화면에서 모델 2개 선택 시 자동으로 비교 결과가 표시된다', (tester) async {
    await tester.pumpWidget(_wrap(const CompareScreen()));
    await tester.pumpAndSettle();

    expect(
      find.text('저장된 측정 결과 기반 비교입니다 (새 측정은 \'벤치\' 탭)'),
      findsOneWidget,
    );
    expect(find.text('비교 실행'), findsNothing);

    await tester.tap(find.text('Qwen2.5 7B 4bit'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Llama 3.1 8B 4bit'));
    await tester.pumpAndSettle();

    expect(find.textContaining('TPS(토큰속도)'), findsWidgets);
  });

  testWidgets('모델 관리 화면이 디스크 카드와 설치 목록을 렌더한다', (tester) async {
    await tester.pumpWidget(_wrap(const ModelManageScreen()));
    await tester.pumpAndSettle();

    expect(find.text('모델 관리'), findsOneWidget);
    expect(find.text('디스크 · 캐시 현황'), findsOneWidget);
    expect(find.textContaining('mlx-community/Qwen2.5-7B-Instruct-4bit'), findsOneWidget);
  });

  testWidgets('모델 관리 삭제 확인 다이얼로그가 표시된다', (tester) async {
    await tester.pumpWidget(_wrap(const ModelManageScreen()));
    await tester.pumpAndSettle();

    await tester.tap(find.byIcon(Icons.delete_outline));
    await tester.pumpAndSettle();

    expect(find.text('모델 캐시 삭제'), findsOneWidget);
    expect(find.textContaining('측정 기록(DB)은 보존됩니다'), findsOneWidget);
    expect(find.text('취소'), findsOneWidget);

    await tester.tap(find.text('취소'));
    await tester.pumpAndSettle();
    expect(find.text('모델 캐시 삭제'), findsNothing);
  });

  testWidgets('벤치 스윕 모드에서 단계 체크박스와 대형 컨텍스트 경고가 표시된다', (tester) async {
    final api = MockAidashApi(
      profiles: [
        FrbProfileRow(
          id: 'mlx-community/Qwen3.6-35B-A3B-OptiQ-4bit',
          backend: 'vllm_mlx',
          modelType: 'llm',
          contextDefault: 4096,
          contextMin: 1024,
          contextMax: 262144,
          sweepSteps: Uint32List.fromList(<int>[
            1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072, 262144,
          ]),
          filename: 'mlx-community-qwen3.6-35b-a3b-optiq-4bit.json',
          isMultimodal: false,
          draftModel: null,
          isDrafter: false,
        ),
      ],
    );
    await tester.pumpWidget(_wrap(const BenchScreen(), api: api));
    await tester.pumpAndSettle();

    await tester.tap(find.text('스윕'));
    await tester.pumpAndSettle();

    expect(find.text('스윕 단계'), findsOneWidget);
    expect(find.text('64K'), findsWidgets);
    expect(
      find.textContaining('대형 컨텍스트'),
      findsOneWidget,
    );
  });

  testWidgets('벤치 화면이 ASR 프로파일에서 오디오 입력 UI로 전환된다', (tester) async {
    final api = MockAidashApi(
      profiles: [
        FrbProfileRow(
          id: 'org/whisper-test',
          backend: 'mlx_whisper',
          modelType: 'asr',
          contextDefault: 4096,
          contextMin: 512,
          contextMax: 4096,
          sweepSteps: Uint32List.fromList(<int>[4096]),
          filename: 'org-whisper-test.json',
          isMultimodal: false,
          draftModel: null,
          isDrafter: false,
        ),
      ],
    );
    await tester.pumpWidget(_wrap(const BenchScreen(), api: api));
    await tester.pumpAndSettle();

    expect(find.text('파일 선택'), findsOneWidget);
    expect(find.text('컨텍스트'), findsNothing);
  });

  testWidgets('MetricLabel 툴팁 아이콘이 표시되고 자세히 다이얼로그가 열린다', (tester) async {
    await tester.pumpWidget(
      _wrap(const MetricLabel(term: 'TPS')),
    );
    await tester.pumpAndSettle();

    expect(find.byIcon(Icons.info_outline), findsOneWidget);
    await tester.tap(find.byIcon(Icons.info_outline));
    await tester.pumpAndSettle();
    expect(find.text('TPS 등급 기준'), findsOneWidget);
  });

  testWidgets('채팅 화면이 ASR 모델 선택 시 입력 비활성 안내를 표시한다', (tester) async {
    final api = MockAidashApi(
      profiles: [
        FrbProfileRow(
          id: 'org/whisper-test',
          backend: 'mlx_whisper',
          modelType: 'asr',
          contextDefault: 4096,
          contextMin: 512,
          contextMax: 4096,
          sweepSteps: Uint32List.fromList(<int>[4096]),
          filename: 'org-whisper-test.json',
          isMultimodal: false,
          draftModel: null,
          isDrafter: false,
        ),
      ],
    );
    await tester.pumpWidget(_wrap(const ChatScreen(), api: api));
    await tester.pumpAndSettle();

    expect(
      find.textContaining('이 모델은 채팅형이 아닙니다'),
      findsOneWidget,
    );
    expect(find.text('채팅 불가 모델'), findsOneWidget);
  });

  testWidgets('채팅 세션 패널과 컨텍스트 게이지가 표시된다', (tester) async {
    await tester.pumpWidget(_wrap(const ChatScreen()));
    await tester.pumpAndSettle();

    expect(find.text('새 대화'), findsOneWidget);
    expect(find.textContaining('컨텍스트 사용'), findsOneWidget);
    expect(find.text('4K'), findsWidgets);
  });

  testWidgets('압축 안내 칩이 대화에 표시된다', (tester) async {
    await tester.pumpWidget(
      _wrap(
        const Center(
          child: Chip(
            avatar: Icon(Icons.compress, size: 16),
            label: Text('이전 대화가 요약·압축되었습니다 — 토큰 절약'),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(
      find.text('이전 대화가 요약·압축되었습니다 — 토큰 절약'),
      findsOneWidget,
    );
  });

  testWidgets('모델 상세 성능 평가 섹션이 표시된다', (tester) async {
    await tester.pumpWidget(
      _wrap(const ModelDetailScreen()),
    );
    await tester.pumpAndSettle();

    expect(find.text('성능 평가'), findsOneWidget);
    expect(find.text('평가 실행'), findsOneWidget);
    expect(find.text('4K'), findsWidgets);
    expect(find.text('이전 평가'), findsOneWidget);
    expect(find.textContaining('72점'), findsOneWidget);
  });

  testWidgets('DraftBadge가 draft 가속 라벨을 표시한다', (tester) async {
    await tester.pumpWidget(_wrap(const DraftBadge()));
    await tester.pumpAndSettle();

    expect(find.text('draft 가속'), findsOneWidget);
  });

  testWidgets('벤치 화면에 speculative 토글이 표시된다', (tester) async {
    await tester.pumpWidget(_wrap(const BenchScreen()));
    await tester.pumpAndSettle();

    expect(
      find.text('보조 모델 가속(speculative) 사용'),
      findsOneWidget,
    );
    expect(find.byType(SwitchListTile), findsOneWidget);
  });

  testWidgets('모델 상세에 보조 모델(drafter) 드롭다운이 표시된다', (tester) async {
    await tester.pumpWidget(_wrap(const ModelDetailScreen()));
    await tester.pumpAndSettle();

    expect(find.text('보조 모델(drafter)'), findsOneWidget);
    expect(find.text('없음'), findsWidgets);
  });

  testWidgets('모델 관리 다운로드 완료 시 프로파일 자동 생성 스낵바가 표시된다', (tester) async {
    await tester.pumpWidget(_wrap(const ModelManageScreen()));
    await tester.pumpAndSettle();

    await tester.enterText(find.byType(TextField), 'gpt2');
    await tester.tap(find.text('검색'));
    await tester.pumpAndSettle();

    await tester.tap(find.text('hf-internal-testing/tiny-random-gpt2'));
    await tester.pumpAndSettle();

    final installButton = find.text('설치');
    await tester.ensureVisible(installButton);
    await tester.pumpAndSettle();
    await tester.tap(installButton);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));
    await tester.pumpAndSettle();

    expect(
      find.textContaining('프로파일 자동 생성됨 — 최대 컨텍스트 1M 감지'),
      findsOneWidget,
    );
  });

  testWidgets('1M 프로파일 모델 상세에 512K·1M 평가 칩이 표시된다', (tester) async {
    final api = MockAidashApi(
      profiles: [
        FrbProfileRow(
          id: 'mlx-community/Qwen2.5-7B-Instruct-4bit',
          backend: 'vllm_mlx',
          modelType: 'llm',
          contextDefault: 4096,
          contextMin: 1024,
          contextMax: 1048576,
          sweepSteps: Uint32List.fromList(<int>[
            1024,
            2048,
            4096,
            8192,
            16384,
            32768,
            65536,
            131072,
            262144,
            524288,
            1048576,
          ]),
          filename: 'mlx-community-Qwen2.5-7B-Instruct-4bit.json',
          isMultimodal: false,
          draftModel: null,
          isDrafter: false,
        ),
      ],
    );
    await tester.pumpWidget(
      _wrap(const ModelDetailScreen(), api: api),
    );
    await tester.pumpAndSettle();

    expect(find.text('512K'), findsWidgets);
    expect(find.text('1M'), findsWidgets);
  });

  testWidgets('DoctorBadge가 3가지 상태를 올바르게 표시한다', (tester) async {
    await tester.pumpWidget(
      _wrap(
        const Column(
          children: [
            DoctorBadge(status: 'ok'),
            DoctorBadge(status: 'warn'),
            DoctorBadge(status: 'missing'),
          ],
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('준비됨'), findsOneWidget);
    expect(find.text('조치 필요'), findsOneWidget);
    expect(find.text('설치 가능'), findsOneWidget);
  });
}