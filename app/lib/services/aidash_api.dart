import 'dart:async';

import 'package:app/src/rust/api.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

/// Converts a Dart int to FRB [PlatformInt64] (plain int on macOS/io).
PlatformInt64 toPlatformInt64(int value) => value;

/// Abstract API surface matching the FRB bindings — enables widget tests with mocks.
abstract class AidashApi {
  void init({required String rootPath});

  Future<FrbDoctorReport> doctorReport();

  bool isBundleDeployMode();

  Stream<FrbBootstrapEvent> envBootstrap();

  List<FrbOverviewRow> statsOverview({int? ctx});

  FrbModelStats statsModel({required String id});

  List<FrbRunListRow> listRuns({String? model});

  FrbDeleteSummary deleteRun({required int id});

  FrbDeleteSummary deleteModel({required String id});

  List<FrbCompareRow> compare({
    required List<String> models,
    int? ctx,
  });

  List<FrbProfileRow> listProfiles();

  Future<int> benchStart({
    required String profileId,
    required int ctx,
    required FrbBenchMode mode,
    String? prompt,
    String? imagePath,
    String? audioPath,
    String? benchTask,
    List<int>? sweepSteps,
    bool? useDraft,
  });

  List<String> listDrafterProfiles();

  void profileSetDraftModel({
    required String profileId,
    String? draftModel,
  });

  void profileSetTask({
    required String profileId,
    required String task,
    required bool adjustBackend,
  });

  String profileTaskLabel({required String task});

  Stream<FrbBenchEvent> benchEvents();

  bool benchAbort();

  Future<void> serveStart({required String profileId, required int ctx});

  Future<void> serveStop();

  bool chatShouldCompress({
    required int promptTokens,
    required int contextSize,
  });

  List<FrbChatSessionRow> chatListSessions();

  int chatCreateSession({required String profileId, required String title});

  void chatDeleteSession({required int sessionId});

  List<FrbChatMessageRow> chatLoadMessages({required int sessionId});

  int chatAppendMessage({
    required int sessionId,
    required String role,
    required String content,
    int? tokenCount,
  });

  void chatUpdateSessionTitle({required int sessionId, required String title});

  Future<String> chatSummarize({required List<FrbChatMessage> messages});

  Stream<FrbChatStreamEvent> chatSend({
    required List<FrbChatMessage> messages,
    String? imagePath,
  });

  Future<FrbAuthStatus> authStatus();

  Future<String> authSet({required String token});

  Future<String> authImport();

  void authClear();

  Future<String> authVerifyToken({required String token});

  Stream<FrbResourceSample> systemResources();

  FrbTierInfo tpsTier({required double decodeTps});

  String getProjectRoot();

  void setProjectRoot({required String path});

  Stream<FrbFixProgress> runFixAction({required String command});

  String deviceLabel();

  Future<FrbCacheScanResult> cacheScan();

  Future<FrbCacheDeleteResult> cacheDelete({required String repoId});

  Future<FrbDiskUsage> diskUsage();

  Future<List<FrbHfSearchResult>> hfSearch({required String query});

  Future<int> hfModelSize({required String repoId});

  Stream<FrbDownloadProgress> hfDownloadStart({required String repoId});

  bool hfDownloadCancel();

  String profileGenerate({required String repoId});

  List<FrbEvalTemplateInfo> evalTemplateList();

  List<int> evalTemplateMeasurableContexts({required String profileId});

  List<FrbEvalTemplateHistoryEntry> evalTemplateHistory({
    required String profileId,
    int? contextSize,
  });

  Stream<FrbEvalTemplateEvent> evalTemplateRun({
    required String profileId,
    required int contextSize,
  });
}

/// In-memory mock for widget tests — no FRB / native library required.
class MockAidashApi implements AidashApi {
  bool initialized = false;
  String projectRoot = '/mock/project';
  final List<FrbOverviewRow> overviewRows;
  final FrbModelStats modelStats;
  final List<FrbRunListRow> runRows;
  final List<FrbProfileRow> profiles;
  final FrbDoctorReport report;
  final FrbAuthStatus auth;
  final List<FrbChatSessionRow> chatSessions;
  final Map<int, List<FrbChatMessageRow>> chatMessagesBySession;
  int _nextSessionId = 1;
  int _nextMessageId = 1;

  MockAidashApi({
    List<FrbOverviewRow>? overviewRows,
    FrbModelStats? modelStats,
    List<FrbRunListRow>? runRows,
    List<FrbProfileRow>? profiles,
    FrbDoctorReport? report,
    FrbAuthStatus? auth,
    List<FrbChatSessionRow>? chatSessions,
    Map<int, List<FrbChatMessageRow>>? chatMessagesBySession,
  })  : overviewRows = overviewRows ?? _defaultOverview(),
        modelStats = modelStats ?? _defaultModelStats(),
        runRows = runRows ?? _defaultRuns(),
        profiles = profiles ?? _defaultProfiles(),
        report = report ?? _defaultDoctor(),
        auth = auth ?? _defaultAuth(),
        chatSessions = chatSessions ?? [],
        chatMessagesBySession = chatMessagesBySession ?? {};

  static FrbTierInfo _tier(double tps) {
    if (tps < 10) {
      return const FrbTierInfo(badge: '🔴', label: '사용 불가', key: 'unusable');
    }
    if (tps < 40) {
      return const FrbTierInfo(badge: '🟠', label: '답답함', key: 'sluggish');
    }
    if (tps < 60) {
      return const FrbTierInfo(badge: '🟢', label: '이상적', key: 'ideal');
    }
    if (tps < 100) {
      return const FrbTierInfo(badge: '🔵', label: '빠름', key: 'fast');
    }
    return const FrbTierInfo(badge: '🟣', label: '실시간급', key: 'realtime');
  }

  static List<FrbOverviewRow> _defaultOverview() => [
        FrbOverviewRow(
          profileId: 'mlx-community/Qwen2.5-7B-Instruct-4bit',
          displayName: 'Qwen2.5 7B 4bit',
          modelType: 'llm',
          decodeTps: 52.3,
          tier: _tier(52.3),
          ttftMs: 180,
          context: const FrbContextPick(
            requested: 4096,
            actual: 4096,
            substituted: false,
          ),
          hfUrl: 'https://huggingface.co/mlx-community/Qwen2.5-7B-Instruct-4bit',
          measuredAt: '2026-06-09T12:00:00Z',
        ),
        FrbOverviewRow(
          profileId: 'mlx-community/Meta-Llama-3.1-8B-Instruct-4bit',
          displayName: 'Llama 3.1 8B 4bit',
          modelType: 'llm',
          decodeTps: 38.1,
          tier: _tier(38.1),
          ttftMs: 210,
          context: const FrbContextPick(
            requested: 4096,
            actual: 4096,
            substituted: false,
          ),
          hfUrl:
              'https://huggingface.co/mlx-community/Meta-Llama-3.1-8B-Instruct-4bit',
          measuredAt: '2026-06-08T09:30:00Z',
        ),
      ];

  static FrbModelStats _defaultModelStats() => FrbModelStats(
        profileId: 'mlx-community/Qwen2.5-7B-Instruct-4bit',
        displayName: 'Qwen2.5 7B 4bit',
        totalRuns: 12,
        latestMeasuredAt: '2026-06-09T12:00:00Z',
        currentTier: _tier(52.3),
        currentDecodeTps: 52.3,
        peakPhysFootprintBytes: 6 * 1024 * 1024 * 1024,
        peakMlxActiveBytes: 5 * 1024 * 1024 * 1024,
        hfUrl: 'https://huggingface.co/mlx-community/Qwen2.5-7B-Instruct-4bit',
        byContext: const [
          FrbContextStatsRow(
            contextSize: 4096,
            decodeTpsMin: 48.0,
            decodeTpsAvg: 52.3,
            decodeTpsMax: 55.0,
            ttftAvgMs: 180,
            runCount: 8,
            peakPhysFootprintBytes: 6 * 1024 * 1024 * 1024,
            peakPhysAvgBytes: 5 * 1024 * 1024 * 1024 + 512 * 1024 * 1024,
          ),
          FrbContextStatsRow(
            contextSize: 8192,
            decodeTpsMin: 40.0,
            decodeTpsAvg: 44.5,
            decodeTpsMax: 47.0,
            ttftAvgMs: 220,
            runCount: 4,
            peakPhysFootprintBytes: 7 * 1024 * 1024 * 1024,
            peakPhysAvgBytes: 6 * 1024 * 1024 * 1024 + 256 * 1024 * 1024,
          ),
        ],
      );

  static List<FrbRunListRow> _defaultRuns() => [
        FrbRunListRow(
          runId: 101,
          profileId: 'mlx-community/Qwen2.5-7B-Instruct-4bit',
          displayName: 'Qwen2.5 7B 4bit',
          kind: 'bench',
          contextSize: 4096,
          status: 'completed',
          decodeTps: 52.3,
          peakPhysFootprintBytes: 6 * 1024 * 1024 * 1024,
          tier: _tier(52.3),
          endedAt: '1717920000000',
          useDraft: true,
        ),
        FrbRunListRow(
          runId: 102,
          profileId: 'mlx-community/Qwen2.5-7B-Instruct-4bit',
          displayName: 'Qwen2.5 7B 4bit',
          kind: 'bench',
          contextSize: 8192,
          status: 'completed',
          decodeTps: 44.5,
          peakPhysFootprintBytes: 7 * 1024 * 1024 * 1024,
          tier: _tier(44.5),
          endedAt: '1718006400000',
          useDraft: false,
        ),
      ];

  static List<FrbProfileRow> _defaultProfiles() => [
        FrbProfileRow(
          id: 'mlx-community/Qwen2.5-7B-Instruct-4bit',
          backend: 'vllm_mlx',
          modelType: 'llm',
          contextDefault: 4096,
          contextMin: 2048,
          contextMax: 32768,
          sweepSteps: Uint32List.fromList(<int>[2048, 4096, 8192, 16384]),
          filename: 'mlx-community-Qwen2.5-7B-Instruct-4bit.json',
          isMultimodal: false,
          draftModel: 'mlx-community/gemma-assistant',
          isDrafter: false,
        ),
        FrbProfileRow(
          id: 'mlx-community/gemma-assistant',
          backend: 'mlx_vlm',
          modelType: 'drafter',
          contextDefault: 4096,
          contextMin: 512,
          contextMax: 4096,
          sweepSteps: Uint32List.fromList(<int>[4096]),
          filename: 'mlx-community-gemma-assistant.json',
          isMultimodal: true,
          draftModel: null,
          isDrafter: true,
        ),
      ];

  static FrbDoctorReport _defaultDoctor() => const FrbDoctorReport(
        items: [
          FrbDoctorItem(
            category: '시스템',
            name: 'Apple Silicon',
            status: 'ok',
            detail: 'M3 Pro 감지됨',
          ),
          FrbDoctorItem(
            category: '도구',
            name: 'uv',
            status: 'ok',
            detail: 'uv 0.5.0',
          ),
          FrbDoctorItem(
            category: '백엔드',
            name: 'vllm-mlx',
            status: 'warn',
            detail: '미설치',
            fixAction: 'uv sync --extra vllm',
          ),
        ],
      );

  static FrbAuthStatus _defaultAuth() => const FrbAuthStatus(
        sources: [
          FrbTokenSourceStatus(
            source: 'keychain',
            label: 'Keychain',
            present: false,
          ),
          FrbTokenSourceStatus(
            source: 'hf_cli',
            label: 'HF CLI 토큰',
            present: true,
          ),
        ],
        activeSource: 'hf_cli',
        maskedToken: 'hf_****abcd',
        whoamiUser: 'testuser',
        canImport: true,
      );

  @override
  void init({required String rootPath}) {
    initialized = true;
    projectRoot = rootPath;
  }

  @override
  Future<FrbDoctorReport> doctorReport() async => report;

  @override
  bool isBundleDeployMode() => false;

  @override
  Stream<FrbBootstrapEvent> envBootstrap() async* {
    yield const FrbBootstrapEvent(
      step: 'uv',
      kind: 'step_done',
      message: 'mock bootstrap complete',
      success: true,
    );
  }

  @override
  List<FrbOverviewRow> statsOverview({int? ctx}) => overviewRows;

  @override
  FrbModelStats statsModel({required String id}) => modelStats;

  @override
  List<FrbRunListRow> listRuns({String? model}) => runRows;

  @override
  FrbDeleteSummary deleteRun({required int id}) =>
      const FrbDeleteSummary(runs: 1, samples: 120, results: 1);

  @override
  FrbDeleteSummary deleteModel({required String id}) =>
      const FrbDeleteSummary(runs: 5, samples: 600, results: 5);

  @override
  List<FrbCompareRow> compare({
    required List<String> models,
    int? ctx,
  }) =>
      models
          .map(
            (m) => FrbCompareRow(
              profileId: m,
              displayName: m.split('/').last,
              modelType: 'llm',
              contextRequested: ctx ?? 4096,
              contextActual: ctx ?? 4096,
              contextSubstituted: false,
              decodeTps: m.contains('Qwen') ? 52.3 : 38.1,
              tier: _tier(m.contains('Qwen') ? 52.3 : 38.1),
              ttftMs: 180,
              peakPhysFootprintBytes: 6 * 1024 * 1024 * 1024,
              peakMlxActiveBytes: 5 * 1024 * 1024 * 1024,
              tokensIn: 128,
              tokensOut: 64,
              measuredAt: '2026-06-09T12:00:00Z',
              hfUrl: 'https://huggingface.co/$m',
              useDraft: m.contains('Qwen'),
            ),
          )
          .toList();

  @override
  List<FrbProfileRow> listProfiles() => profiles;

  @override
  List<String> listDrafterProfiles() => profiles
      .where((p) => p.isDrafter)
      .map((p) => p.id)
      .toList();

  @override
  void profileSetDraftModel({
    required String profileId,
    String? draftModel,
  }) {}

  @override
  Future<int> benchStart({
    required String profileId,
    required int ctx,
    required FrbBenchMode mode,
    String? prompt,
    String? imagePath,
    String? audioPath,
    String? benchTask,
    List<int>? sweepSteps,
    bool? useDraft,
  }) async =>
      201;

  @override
  void profileSetTask({
    required String profileId,
    required String task,
    required bool adjustBackend,
  }) {}

  @override
  String profileTaskLabel({required String task}) {
    const labels = {
      'llm': '텍스트 생성',
      'multimodal': '멀티모달(이미지+텍스트)',
      'asr': '음성→텍스트(STT)',
      'tts': '텍스트→음성(TTS)',
      'image_gen': '이미지 생성',
      'video_gen': '동영상 생성',
    };
    return labels[task] ?? task;
  }

  @override
  Stream<FrbBenchEvent> benchEvents() => const Stream.empty();

  @override
  bool benchAbort() => true;

  @override
  Future<void> serveStart({
    required String profileId,
    required int ctx,
  }) async {}

  @override
  Future<void> serveStop() async {}

  @override
  bool chatShouldCompress({
    required int promptTokens,
    required int contextSize,
  }) =>
      contextSize > 0 && promptTokens > (contextSize * 0.7).round();

  @override
  List<FrbChatSessionRow> chatListSessions() => chatSessions;

  @override
  int chatCreateSession({required String profileId, required String title}) {
    final id = _nextSessionId++;
    chatSessions.insert(
      0,
      FrbChatSessionRow(
        id: id,
        profileId: profileId,
        title: title,
        createdAt: DateTime.now().millisecondsSinceEpoch.toString(),
        updatedAt: DateTime.now().millisecondsSinceEpoch.toString(),
      ),
    );
    chatMessagesBySession[id] = [];
    return id;
  }

  @override
  void chatDeleteSession({required int sessionId}) {
    chatSessions.removeWhere((s) => s.id.toInt() == sessionId);
    chatMessagesBySession.remove(sessionId);
  }

  @override
  List<FrbChatMessageRow> chatLoadMessages({required int sessionId}) =>
      chatMessagesBySession[sessionId] ?? [];

  @override
  int chatAppendMessage({
    required int sessionId,
    required String role,
    required String content,
    int? tokenCount,
  }) {
    final id = _nextMessageId++;
    chatMessagesBySession.putIfAbsent(sessionId, () => []).add(
          FrbChatMessageRow(
            id: id,
            sessionId: sessionId,
            role: role,
            content: content,
            createdAt: DateTime.now().millisecondsSinceEpoch.toString(),
            tokenCount: tokenCount,
          ),
        );
    return id;
  }

  @override
  void chatUpdateSessionTitle({required int sessionId, required String title}) {}

  @override
  Future<String> chatSummarize({required List<FrbChatMessage> messages}) async =>
      '요약: ${messages.map((m) => m.content).join(' / ')}';

  @override
  Stream<FrbChatStreamEvent> chatSend({
    required List<FrbChatMessage> messages,
    String? imagePath,
  }) async* {
    yield const FrbChatStreamEvent(
      isDone: false,
      text: '안녕하세요! ',
      promptTokens: 0,
      completionTokens: 0,
    );
    yield const FrbChatStreamEvent(
      isDone: false,
      text: '테스트 응답입니다.',
      promptTokens: 0,
      completionTokens: 0,
    );
    yield const FrbChatStreamEvent(
      isDone: true,
      text: '',
      promptTokens: 120,
      completionTokens: 24,
    );
  }

  @override
  Future<FrbAuthStatus> authStatus() async => auth;

  @override
  Future<String> authSet({required String token}) async => 'testuser';

  @override
  Future<String> authImport() async => 'testuser';

  @override
  void authClear() {}

  @override
  Future<String> authVerifyToken({required String token}) async => 'testuser';

  @override
  Stream<FrbResourceSample> systemResources() async* {
    yield FrbResourceSample(
      ts: BigInt.from(DateTime.now().millisecondsSinceEpoch),
      physFootprintBytes: BigInt.from(4 * 1024 * 1024 * 1024),
      mlxActiveBytes: BigInt.from(3 * 1024 * 1024 * 1024),
      cpuPct: 42.5,
      sysAvailableBytes: BigInt.from(16 * 1024 * 1024 * 1024),
      totalMemoryBytes: BigInt.from(48 * 1024 * 1024 * 1024),
      powerW: 18.2,
      tempC: 55.0,
      throttled: false,
    );
  }

  @override
  FrbTierInfo tpsTier({required double decodeTps}) => _tier(decodeTps);

  @override
  String getProjectRoot() => projectRoot;

  @override
  void setProjectRoot({required String path}) {
    projectRoot = path;
  }

  @override
  Stream<FrbFixProgress> runFixAction({required String command}) async* {
    yield FrbFixProgress(
      line: 'Running $command...',
      done: false,
      success: false,
    );
    yield const FrbFixProgress(
      line: '완료',
      done: true,
      success: true,
      exitCode: 0,
    );
  }

  @override
  String deviceLabel() => 'Mock Device (M3 Pro)';

  @override
  Future<FrbCacheScanResult> cacheScan() async => FrbCacheScanResult(
        cacheDir: '~/.cache/huggingface/hub',
        totalSizeBytes: BigInt.from(42 * 1024 * 1024 * 1024),
        repoCount: BigInt.from(2),
        repos: [
          FrbCacheRepoEntry(
            repoId: 'mlx-community/Qwen2.5-7B-Instruct-4bit',
            sizeBytes: BigInt.from(4 * 1024 * 1024 * 1024),
            lastModified: '2026-06-09T12:00:00Z',
            hasProfile: true,
          ),
        ],
      );

  @override
  Future<FrbCacheDeleteResult> cacheDelete({required String repoId}) async =>
      FrbCacheDeleteResult(
        repoId: repoId,
        deleted: true,
        freedBytes: BigInt.from(1024 * 1024),
      );

  @override
  Future<FrbDiskUsage> diskUsage() async => FrbDiskUsage(
        totalBytes: BigInt.from(500 * 1024 * 1024 * 1024),
        availableBytes: BigInt.from(200 * 1024 * 1024 * 1024),
        cacheDir: '~/.cache/huggingface/hub',
        cacheTotalBytes: BigInt.from(42 * 1024 * 1024 * 1024),
      );

  @override
  Future<List<FrbHfSearchResult>> hfSearch({required String query}) async => [
        FrbHfSearchResult(
          repoId: 'hf-internal-testing/tiny-random-gpt2',
          downloads: 1000,
          likes: 5,
          pipelineTag: 'text-generation',
          installed: false,
        ),
      ];

  @override
  Future<int> hfModelSize({required String repoId}) async => 1024 * 1024;

  @override
  Stream<FrbDownloadProgress> hfDownloadStart({required String repoId}) async* {
    yield const FrbDownloadProgress(
      line: 'downloading...',
      percent: 50,
      done: false,
      success: true,
    );
    yield const FrbDownloadProgress(
      line: 'done',
      percent: 100,
      done: true,
      success: true,
    );
  }

  @override
  bool hfDownloadCancel() => true;

  @override
  String profileGenerate({required String repoId}) {
    if (!profiles.any((p) => p.id == repoId)) {
      profiles.add(
        FrbProfileRow(
          id: repoId,
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
          filename: '${repoId.replaceAll('/', '-')}.json',
          isMultimodal: false,
          draftModel: null,
          isDrafter: false,
        ),
      );
    }
    return '/mock/profiles/$repoId.json';
  }

  @override
  List<FrbEvalTemplateInfo> evalTemplateList() => const [];

  @override
  List<int> evalTemplateMeasurableContexts({required String profileId}) {
    final profile = profiles.cast<FrbProfileRow?>().firstWhere(
          (p) => p?.id == profileId,
          orElse: () => null,
        );
    if (profile != null && profile.contextMax >= 1048576) {
      return const [
        1024,
        4096,
        16384,
        32768,
        65536,
        131072,
        262144,
        524288,
        1048576,
      ];
    }
    if (profile != null && profile.contextMax >= 262144) {
      return const [
        1024,
        4096,
        16384,
        32768,
        65536,
        131072,
        262144,
      ];
    }
    return const [1024, 4096];
  }

  @override
  List<FrbEvalTemplateHistoryEntry> evalTemplateHistory({
    required String profileId,
    int? contextSize,
  }) =>
      [
        FrbEvalTemplateHistoryEntry(
          contextSize: 4096,
          totalScore: 72,
          createdAt: '1717920000000',
          items: [
            FrbEvalTemplateItemResult(
              templateId: 'ctx4k-1',
              description: '지식 QA — 화학식',
              score: 100,
              outputExcerpt: 'H2O',
              elapsedMs: BigInt.from(1200),
            ),
            FrbEvalTemplateItemResult(
              templateId: 'ctx4k-2',
              description: '추론 — 일수 계산',
              score: 100,
              outputExcerpt: '15',
              elapsedMs: BigInt.from(1100),
            ),
            FrbEvalTemplateItemResult(
              templateId: 'ctx4k-3',
              description: '지시 이행 — 3색 나열',
              score: 15,
              outputExcerpt: '빨강, 노랑, 파랑',
              elapsedMs: BigInt.from(1500),
            ),
          ],
        ),
      ];

  @override
  Stream<FrbEvalTemplateEvent> evalTemplateRun({
    required String profileId,
    required int contextSize,
  }) async* {
    yield const FrbEvalTemplateEvent.started(
      templateId: 'ctx4k-1',
      index: 1,
      total: 3,
    );
    yield FrbEvalTemplateEvent.completed(
      templateId: 'ctx4k-1',
      score: 100,
      elapsedMs: BigInt.from(1200),
    );
    yield FrbEvalTemplateEvent.finished(
      totalScore: 72,
      items: [
        FrbEvalTemplateItemResult(
          templateId: 'ctx4k-1',
          description: '지식 QA',
          score: 100,
          outputExcerpt: 'H2O',
          elapsedMs: BigInt.from(1200),
        ),
      ],
    );
  }
}