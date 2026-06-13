import 'package:app/app_shell.dart';
import 'package:app/screens/bench_screen.dart';
import 'package:app/screens/chat_screen.dart';
import 'package:app/screens/compare_screen.dart';
import 'package:app/screens/dashboard_screen.dart';
import 'package:app/screens/model_detail_screen.dart';
import 'package:app/screens/model_manage_screen.dart';
import 'package:app/screens/onboarding_screen.dart';
import 'dart:typed_data';

import 'dart:async';

import 'package:app/services/aidash_api.dart';
import 'package:app/src/rust/api.dart';
import 'package:app/widgets/metric_label.dart';
import 'package:app/services/config_service.dart';
import 'package:app/theme/app_theme.dart';
import 'package:app/widgets/doctor_badge.dart';
import 'package:app/widgets/draft_badge.dart';
import 'package:app/widgets/eval_template_widgets.dart';
import 'package:app/widgets/tier_badge.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:provider/provider.dart';

class _MultiturnCaptureMockApi extends MockAidashApi {
  final List<List<FrbChatMessage>> sentPayloads = [];
  int sendCount = 0;

  @override
  Future<void> serveWaitReady({int timeoutSec = 0}) async {}

  @override
  Stream<FrbChatStreamEvent> chatSend({
    required List<FrbChatMessage> messages,
    String? imagePath,
  }) async* {
    sendCount++;
    sentPayloads.add(List<FrbChatMessage>.from(messages));
    yield FrbChatStreamEvent(
      isDone: false,
      text: sendCount == 1 ? '첫 응답' : '두 번째 응답',
      promptTokens: 0,
      completionTokens: 0,
    );
    yield FrbChatStreamEvent(
      isDone: true,
      text: '',
      promptTokens: 40 + sendCount * 10,
      completionTokens: 8,
    );
  }
}

class _DelayedEvalMockApi extends MockAidashApi {
  @override
  Stream<FrbEvalTemplateEvent> evalTemplateRun({
    required String profileId,
    required int contextSize,
  }) async* {
    yield const FrbEvalTemplateEvent.started(
      templateId: 'ctx1k-1',
      index: 1,
      total: 3,
    );
    await Future<void>.delayed(const Duration(milliseconds: 50));
    yield FrbEvalTemplateEvent.finished(
      totalScore: 72,
      items: [
        FrbEvalTemplateItemResult(
          templateId: 'ctx1k-1',
          description: '지식 QA',
          score: 100,
          outputExcerpt: 'H2O',
          elapsedMs: BigInt.from(1200),
        ),
      ],
    );
  }
}

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
          generationKind: 'autoregressive',
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
          generationKind: 'autoregressive',
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
          generationKind: 'autoregressive',
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

  testWidgets('2턴 전송 시 payload에 전체 히스토리가 포함된다', (tester) async {
    final api = _MultiturnCaptureMockApi();
    await tester.pumpWidget(_wrap(const ChatScreen(), api: api));
    await tester.pumpAndSettle();

    final chatInput = find.byType(TextField).last;

    await tester.enterText(chatInput, '첫 질문입니다');
    await tester.tap(find.text('전송'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pumpAndSettle();

    expect(api.sendCount, 1);
    expect(api.sentPayloads.first.length, 1);
    expect(api.sentPayloads.first.first.role, 'user');
    expect(api.sentPayloads.first.first.content, '첫 질문입니다');

    await tester.enterText(chatInput, '아까 질문이 뭐였죠?');
    await tester.tap(find.text('전송'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    await tester.pumpAndSettle();

    expect(api.sendCount, 2);
    final second = api.sentPayloads[1];
    expect(second.length, 3);
    expect(second[0].role, 'user');
    expect(second[0].content, '첫 질문입니다');
    expect(second[1].role, 'assistant');
    expect(second[1].content, '첫 응답');
    expect(second[2].role, 'user');
    expect(second[2].content, '아까 질문이 뭐였죠?');
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
          generationKind: 'autoregressive',
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

  testWidgets('기록 전용 모델 상세가 통계를 표시하고 평가 실행은 비활성이다', (tester) async {
    const recordOnlyId = 'mlx-community/Qwen3-4B-Instruct-2507-4bit';
    final api = MockAidashApi(
      overviewRows: [
        ...MockAidashApi().overviewRows,
        FrbOverviewRow(
          profileId: recordOnlyId,
          displayName: 'Qwen3 4B (기록만)',
          modelType: 'llm',
          generationKind: 'autoregressive',
          decodeTps: 41.2,
          tier: const FrbTierInfo(badge: '🟢', label: '이상적', key: 'ideal'),
          ttftMs: 195,
          context: const FrbContextPick(
            requested: 4096,
            actual: 4096,
            substituted: false,
          ),
          hfUrl: 'https://huggingface.co/$recordOnlyId',
          measuredAt: '2026-06-10T12:00:00Z',
        ),
      ],
      profiles: MockAidashApi().profiles,
      modelStats: FrbModelStats(
        profileId: recordOnlyId,
        displayName: 'Qwen3 4B (기록만)',
        generationKind: 'autoregressive',
        totalRuns: 3,
        latestMeasuredAt: '2026-06-10T12:00:00Z',
        currentTier: const FrbTierInfo(badge: '🟢', label: '이상적', key: 'ideal'),
        currentDecodeTps: 41.2,
        peakPhysFootprintBytes: 4 * 1024 * 1024 * 1024,
        peakMlxActiveBytes: 3 * 1024 * 1024 * 1024,
        hfUrl: 'https://huggingface.co/$recordOnlyId',
        byContext: const [
          FrbContextStatsRow(
            contextSize: 4096,
            decodeTpsMin: 38.0,
            decodeTpsAvg: 41.2,
            decodeTpsMax: 44.0,
            ttftAvgMs: 195,
            runCount: 2,
            peakPhysFootprintBytes: 4 * 1024 * 1024 * 1024,
            peakPhysAvgBytes: 3 * 1024 * 1024 * 1024 + 256 * 1024 * 1024,
          ),
        ],
      ),
      runRows: [
        FrbRunListRow(
          runId: 201,
          profileId: recordOnlyId,
          displayName: 'Qwen3 4B (기록만)',
          generationKind: 'autoregressive',
          kind: 'bench',
          contextSize: 4096,
          status: 'completed',
          decodeTps: 41.2,
          peakPhysFootprintBytes: 4 * 1024 * 1024 * 1024,
          tier: const FrbTierInfo(badge: '🟢', label: '이상적', key: 'ideal'),
          endedAt: '1717920000000',
          useDraft: false,
        ),
      ],
    );

    await tester.pumpWidget(
      _wrap(ModelDetailScreen(modelId: recordOnlyId), api: api),
    );
    await tester.pumpAndSettle();

    expect(find.text('기록 전용 (로컬 모델 없음)'), findsOneWidget);
    expect(find.text('총 런'), findsOneWidget);
    expect(find.text('3'), findsWidgets);
    expect(find.text('평가 실행'), findsOneWidget);
    await tester.tap(find.text('평가 실행'));
    await tester.pump();
    expect(find.text('평가 중…'), findsNothing);
    expect(find.textContaining('profile'), findsNothing);
  });

  testWidgets('비교 화면 컨텍스트 칩이 측정 기록 합집합으로 생성된다', (tester) async {
    final api = MockAidashApi(
      measuredContextSizes: [2048, 4096, 8192, 16384, 32768, 262144],
    );
    await tester.pumpWidget(_wrap(const CompareScreen(), api: api));
    await tester.pumpAndSettle();

    expect(find.text('256K'), findsOneWidget);
    expect(find.text('32K'), findsOneWidget);
    expect(find.text('4K'), findsWidgets);
  });

  testWidgets('벤치 템플릿 평가 모드 전환과 템플릿 미리보기가 표시된다', (tester) async {
    await tester.pumpWidget(_wrap(const BenchScreen()));
    await tester.pumpAndSettle();

    expect(find.text('프롬프트 측정'), findsOneWidget);
    expect(find.text('템플릿 평가'), findsOneWidget);

    await tester.tap(find.text('템플릿 평가'));
    await tester.pumpAndSettle();

    expect(find.text('템플릿 미리보기'), findsOneWidget);
    expect(find.text('ctx1k-1'), findsOneWidget);
    expect(find.text('ctx1k-2'), findsOneWidget);
    expect(find.text('ctx1k-3'), findsOneWidget);
    expect(find.textContaining('▶ 평가 실행 (프롬프트 3개)'), findsOneWidget);

    await tester.tap(find.text('4K'));
    await tester.pumpAndSettle();
    expect(find.text('ctx4k-1'), findsOneWidget);
    expect(
      find.text('결과는 DB에 저장되며 모델 상세의 성능 평가 이력에 자동 반영됩니다.'),
      findsNothing,
    );
  });

  testWidgets('벤치 템플릿 평가 실행 버튼 상태가 전이된다', (tester) async {
    await tester.pumpWidget(
      _wrap(const BenchScreen(), api: _DelayedEvalMockApi()),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.text('템플릿 평가'));
    await tester.pumpAndSettle();

    final runButton = find.textContaining('▶ 평가 실행 (프롬프트 3개)');
    expect(runButton, findsOneWidget);

    await tester.ensureVisible(runButton);
    await tester.pumpAndSettle();
    await tester.tap(runButton);
    await tester.pump();
    expect(find.text('평가 중…'), findsOneWidget);
    await tester.pumpAndSettle(const Duration(milliseconds: 100));

    expect(find.text('72'), findsOneWidget);
    expect(find.textContaining('지식 QA'), findsWidgets);
    expect(find.text('완료'), findsOneWidget);
  });

  testWidgets('벤치 프롬프트 측정 버튼 라벨과 상태 뱃지가 한국어로 표시된다', (tester) async {
    await tester.pumpWidget(_wrap(const BenchScreen()));
    await tester.pumpAndSettle();

    expect(find.textContaining('▶ 측정 시작'), findsOneWidget);
    expect(find.text('대기'), findsOneWidget);
    expect(
      find.text('측정에 사용할 프롬프트 — 비우면 컨텍스트를 채우는 표준 프롬프트 사용'),
      findsOneWidget,
    );
  });

  testWidgets('ASR 프로파일 벤치에는 템플릿 평가 세그먼트가 없다', (tester) async {
    final api = MockAidashApi(
      profiles: [
        FrbProfileRow(
          id: 'org/whisper-test',
          backend: 'mlx_whisper',
          modelType: 'asr',
          generationKind: 'autoregressive',
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

    expect(find.text('템플릿 평가'), findsNothing);
    expect(find.text('프롬프트 측정'), findsNothing);
  });

  testWidgets('EvalScoreCard가 총점과 항목별 점수를 표시한다', (tester) async {
    await tester.pumpWidget(
      _wrap(
        EvalScoreCard(
          totalScore: 72,
          items: [
            FrbEvalTemplateItemResult(
              templateId: 'ctx4k-1',
              description: '지식 QA — 화학식',
              score: 100,
              outputExcerpt: 'H2O',
              elapsedMs: BigInt.from(1200),
            ),
            FrbEvalTemplateItemResult(
              templateId: 'ctx4k-3',
              description: '지시 이행 — 3색 나열',
              score: 15,
              outputExcerpt: '빨강',
              elapsedMs: BigInt.from(1500),
            ),
          ],
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('72'), findsOneWidget);
    expect(find.text('/ 100'), findsOneWidget);
    expect(find.text('100점'), findsOneWidget);
    expect(find.text('15점'), findsOneWidget);
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