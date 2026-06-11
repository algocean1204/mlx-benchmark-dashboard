import 'dart:async';
import 'dart:io';

import 'package:app/services/aidash_api.dart';
import 'package:app/src/rust/api.dart';
import 'package:app/task_labels.dart';
import 'package:app/theme/app_theme.dart';
import 'package:app/utils/formatters.dart';
import 'package:app/utils/wav_duration.dart';
import 'package:app/widgets/draft_badge.dart';
import 'package:app/widgets/metric_label.dart';
import 'package:app/widgets/tier_badge.dart';
import 'package:file_selector/file_selector.dart';
import 'package:fl_chart/fl_chart.dart';
import 'package:flutter/material.dart';
import 'package:path/path.dart' as p;
import 'package:provider/provider.dart';

class BenchScreen extends StatefulWidget {
  const BenchScreen({super.key});

  @override
  State<BenchScreen> createState() => _BenchScreenState();
}

class _BenchScreenState extends State<BenchScreen> {
  List<FrbProfileRow> _profiles = [];
  String? _profileId;
  String _benchTask = 'llm';
  int _ctx = 4096;
  FrbBenchMode _mode = FrbBenchMode.single;
  final Set<int> _selectedSweepSteps = {};
  String _state = 'Idle';
  final List<FrbResourceSample> _samples = [];
  FrbBenchResult? _result;
  double? _audioDurationSec;
  StreamSubscription<FrbBenchEvent>? _eventSub;
  bool _running = false;
  final List<String> _logs = [];

  final _promptController = TextEditingController();
  String? _imagePath;
  String? _audioPath;
  bool _useDraft = true;

  static const _defaultTtsText = 'Hello, benchmark test.';
  static const _defaultImagePrompt =
      'A simple red circle on white background.';
  static const _defaultMultimodalPrompt = 'Describe this image briefly.';
  static const _defaultImageFixture = 'tests/fixtures/test_image.png';
  static const _defaultAudioFixture = 'tests/fixtures/test_audio.wav';
  static const _largeContextThreshold = 65536;
  static const _defaultSweepMaxSelected = 32768;

  @override
  void initState() {
    super.initState();
    _loadProfiles();
  }

  @override
  void dispose() {
    _eventSub?.cancel();
    _promptController.dispose();
    super.dispose();
  }

  void _loadProfiles() {
    final api = context.read<AidashApi>();
    final profiles =
        api.listProfiles().where((p) => !p.isDrafter).toList();
    setState(() {
      _profiles = profiles;
      if (profiles.isNotEmpty) {
        _profileId = profiles.first.id;
        _ctx = profiles.first.contextDefault;
        _benchTask = TaskLabels.benchTasksForProfile(profiles.first.modelType).first;
        _useDraft = profiles.first.draftModel != null;
        _applyTaskDefaults(api);
        _resetSweepStepSelection();
      }
    });
  }

  void _resetSweepStepSelection() {
    _selectedSweepSteps
      ..clear()
      ..addAll(
        _ctxOptions.where((s) => s <= _defaultSweepMaxSelected),
      );
  }

  bool get _hasLargeSweepSteps =>
      _ctxOptions.any((s) => s >= _largeContextThreshold);

  List<int> get _orderedSelectedSweepSteps =>
      _selectedSweepSteps.toList()..sort();

  FrbProfileRow? get _profile {
    if (_profileId == null) return null;
    return _profiles.cast<FrbProfileRow?>().firstWhere(
          (p) => p?.id == _profileId,
          orElse: () => null,
        );
  }

  List<int> get _ctxOptions {
    final p = _profile;
    if (p == null) return [4096];
    final steps = p.sweepSteps.toList();
    if (steps.isNotEmpty) return steps;
    return [p.contextDefault];
  }

  String _fixturePath(AidashApi api, String relative) {
    return p.join(api.getProjectRoot(), relative);
  }

  void _applyTaskDefaults(AidashApi api) {
    if (_benchTask == 'multimodal') {
      _promptController.text = _defaultMultimodalPrompt;
      _imagePath ??= _fixturePath(api, _defaultImageFixture);
    } else if (_benchTask == 'tts') {
      _promptController.text = _defaultTtsText;
    } else if (_benchTask == 'image_gen') {
      _promptController.text = _defaultImagePrompt;
    } else if (_benchTask == 'asr') {
      _audioPath ??= _fixturePath(api, _defaultAudioFixture);
      _refreshAudioDuration();
    } else if (_promptController.text.isEmpty) {
      _promptController.text = '벤치마크 테스트 프롬프트입니다.';
    }
  }

  void _refreshAudioDuration() {
    final path = _audioPath;
    if (path == null) {
      _audioDurationSec = null;
      return;
    }
    _audioDurationSec = wavDurationSeconds(path);
  }

  Future<void> _pickImage() async {
    const types = [
      XTypeGroup(
        label: 'images',
        extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif'],
      ),
    ];
    final file = await openFile(acceptedTypeGroups: types);
    if (file != null) {
      setState(() => _imagePath = file.path);
    }
  }

  Future<void> _pickAudio() async {
    const types = [
      XTypeGroup(
        label: 'audio',
        extensions: ['wav', 'mp3', 'm4a', 'flac'],
      ),
    ];
    final file = await openFile(acceptedTypeGroups: types);
    if (file != null) {
      setState(() {
        _audioPath = file.path;
        _refreshAudioDuration();
      });
    }
  }

  Future<void> _start() async {
    if (_profileId == null || _running) return;
    if (TaskLabels.isVideoUnsupported(_benchTask)) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('지원 예정 — 현재 측정 불가')),
      );
      return;
    }

    final api = context.read<AidashApi>();
    setState(() {
      _running = true;
      _samples.clear();
      _logs.clear();
      _result = null;
      _state = 'Starting';
    });

    _eventSub?.cancel();
    _eventSub = api.benchEvents().listen(_onEvent);

    String? prompt;
    String? imagePath;
    String? audioPath;
    String? benchTask;

    if (_benchTask == 'llm' ||
        _benchTask == 'multimodal' ||
        _benchTask == 'tts' ||
        _benchTask == 'image_gen') {
      prompt = _promptController.text.trim();
    }
    if (_benchTask == 'multimodal') {
      imagePath = _imagePath;
    }
    if (_benchTask == 'asr') {
      audioPath = _audioPath;
    }

    final profile = _profile;
    if (profile != null && _benchTask != profile.modelType) {
      benchTask = _benchTask;
    }

    if (_mode == FrbBenchMode.sweep && _selectedSweepSteps.isEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('스윕할 컨텍스트 단계를 1개 이상 선택하세요')),
      );
      setState(() => _running = false);
      return;
    }

    try {
      await api.benchStart(
        profileId: _profileId!,
        ctx: _ctx,
        mode: _mode,
        prompt: prompt,
        imagePath: imagePath,
        audioPath: audioPath,
        benchTask: benchTask,
        sweepSteps: _mode == FrbBenchMode.sweep
            ? _orderedSelectedSweepSteps
            : null,
        useDraft: profile?.draftModel != null ? _useDraft : null,
      );
    } catch (e) {
      if (mounted) {
        setState(() {
          _running = false;
          _state = 'Error';
          _logs.add('시작 실패: $e');
        });
      }
    }
  }

  void _onEvent(FrbBenchEvent event) {
    event.when(
      stateChanged: (from, to) {
        setState(() {
          _state = to;
          _logs.add('상태: $from → $to');
        });
      },
      sample: (s) {
        setState(() {
          _samples.add(s);
          if (_samples.length > 120) _samples.removeAt(0);
        });
      },
      token: (index, text) {},
      watchdogWarn: () => setState(() => _logs.add('⚠️ Watchdog 경고')),
      watchdogKill: () => setState(() => _logs.add('🔴 Watchdog 강제 종료')),
      runFinished: (runId, status, result) {
        setState(() {
          _running = false;
          _state = status;
          _result = result;
        });
      },
      log: (level, message) =>
          setState(() => _logs.add('[$level] $message')),
      progress: (message) => setState(() => _logs.add(message)),
    );
  }

  void _abort() {
    context.read<AidashApi>().benchAbort();
    setState(() {
      _running = false;
      _state = 'Aborted';
    });
  }

  Widget _buildTaskInput(AidashApi api) {
    if (TaskLabels.isVideoUnsupported(_benchTask)) {
      return const Padding(
        padding: EdgeInsets.symmetric(vertical: 12),
        child: Text(
          '지원 예정 — 현재 측정 불가',
          style: TextStyle(color: AppTheme.warning),
        ),
      );
    }

    switch (_benchTask) {
      case 'multimodal':
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            TextField(
              controller: _promptController,
              decoration: const InputDecoration(labelText: '프롬프트'),
              maxLines: 2,
              enabled: !_running,
            ),
            const SizedBox(height: 12),
            Row(
              children: [
                OutlinedButton.icon(
                  onPressed: _running ? null : _pickImage,
                  icon: const Icon(Icons.folder_open, size: 18),
                  label: const Text('파일 선택'),
                ),
                const SizedBox(width: 12),
                if (_imagePath != null)
                  Expanded(
                    child: Text(
                      p.basename(_imagePath!),
                      overflow: TextOverflow.ellipsis,
                      style: Theme.of(context).textTheme.bodySmall,
                    ),
                  ),
              ],
            ),
            if (_imagePath != null && File(_imagePath!).existsSync()) ...[
              const SizedBox(height: 8),
              ClipRRect(
                borderRadius: BorderRadius.circular(8),
                child: Image.file(
                  File(_imagePath!),
                  height: 80,
                  width: 120,
                  fit: BoxFit.cover,
                ),
              ),
            ],
          ],
        );
      case 'asr':
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                OutlinedButton.icon(
                  onPressed: _running ? null : _pickAudio,
                  icon: const Icon(Icons.folder_open, size: 18),
                  label: const Text('파일 선택'),
                ),
                const SizedBox(width: 12),
                if (_audioPath != null)
                  Expanded(
                    child: Text(
                      p.basename(_audioPath!),
                      overflow: TextOverflow.ellipsis,
                    ),
                  ),
              ],
            ),
            if (_audioDurationSec != null)
              Padding(
                padding: const EdgeInsets.only(top: 8),
                child: Text(
                  '오디오 길이: ${_audioDurationSec!.toStringAsFixed(1)}초',
                  style: Theme.of(context).textTheme.bodySmall?.copyWith(
                        color: AppTheme.inkMuted,
                      ),
                ),
              ),
          ],
        );
      case 'tts':
        return TextField(
          controller: _promptController,
          decoration: const InputDecoration(labelText: '합성할 텍스트'),
          maxLines: 3,
          enabled: !_running,
        );
      case 'image_gen':
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            TextField(
              controller: _promptController,
              decoration: const InputDecoration(labelText: '프롬프트'),
              maxLines: 2,
              enabled: !_running,
            ),
            const SizedBox(height: 8),
            Text(
              '스텝·해상도: 어댑터 기본값 사용',
              style: Theme.of(context).textTheme.bodySmall?.copyWith(
                    color: AppTheme.inkMuted,
                  ),
            ),
          ],
        );
      default:
        return TextField(
          controller: _promptController,
          decoration: const InputDecoration(labelText: '프롬프트'),
          maxLines: 3,
          enabled: !_running,
        );
    }
  }

  Widget _buildResultCard() {
    final result = _result;
    if (result == null) return const SizedBox.shrink();

    final isTokenTask = TaskLabels.usesTpsTier(_benchTask);

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                const Text('벤치 결과', style: TextStyle(fontWeight: FontWeight.bold)),
                const Spacer(),
                if (_useDraft && _profile?.draftModel != null) ...[
                  const DraftBadge(),
                  const SizedBox(width: 8),
                ],
                if (isTokenTask)
                  TierBadge(
                    decodeTps: result.decodeTps,
                    tier: result.tier,
                    compact: true,
                  ),
              ],
            ),
            const SizedBox(height: 8),
            Text('컨텍스트 ${formatContext(result.contextSize)} · ${result.status}'),
            const SizedBox(height: 12),
            if (isTokenTask) ...[
              _ResultMetric(
                label: 'TTFT',
                value: '${result.ttftMs?.toStringAsFixed(0) ?? '—'} ms',
                term: 'TTFT',
              ),
              _ResultMetric(
                label: 'decode TPS',
                value: result.decodeTps?.toStringAsFixed(1) ?? '—',
                term: 'TPS',
              ),
            ] else ...[
              _ResultMetric(
                label: '처리시간',
                value: '${result.ttftMs?.toStringAsFixed(0) ?? '—'} ms',
                term: '처리시간',
              ),
              if (_benchTask == 'asr' && _audioDurationSec != null && result.ttftMs != null)
                _ResultMetric(
                  label: 'RTF',
                  value: (result.ttftMs! / (_audioDurationSec! * 1000))
                      .toStringAsFixed(2),
                  term: 'RTF',
                ),
            ],
            _ResultMetric(
              label: 'Peak RAM',
              value: formatBytesInt(result.peakPhysFootprintBytes.toInt()),
              term: 'Peak RAM',
            ),
          ],
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final profile = _profile;
    final api = context.read<AidashApi>();
    final taskOptions = profile == null
        ? <String>[]
        : TaskLabels.benchTasksForProfile(profile.modelType);

    return ListView(
      padding: const EdgeInsets.all(24),
      children: [
        Text(
          '벤치마크',
          style: Theme.of(context).textTheme.headlineSmall?.copyWith(
                fontWeight: FontWeight.bold,
              ),
        ),
        const SizedBox(height: 16),
        Card(
          child: Padding(
            padding: const EdgeInsets.all(20),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                DropdownMenu<String>(
                  label: const Text('프로파일'),
                  initialSelection: _profileId,
                  dropdownMenuEntries: _profiles
                      .map(
                        (p) => DropdownMenuEntry(value: p.id, label: p.id),
                      )
                      .toList(),
                  onSelected: (v) {
                    if (v == null) return;
                    final p = _profiles.firstWhere((x) => x.id == v);
                    final tasks = TaskLabels.benchTasksForProfile(p.modelType);
                    setState(() {
                      _profileId = v;
                      _ctx = p.contextDefault;
                      _benchTask = tasks.first;
                      _useDraft = p.draftModel != null;
                      _applyTaskDefaults(api);
                      _resetSweepStepSelection();
                    });
                  },
                ),
                if (profile?.draftModel != null) ...[
                  const SizedBox(height: 12),
                  SwitchListTile(
                    contentPadding: EdgeInsets.zero,
                    title: const Text('보조 모델 가속(speculative) 사용'),
                    subtitle: Text(
                      profile!.draftModel!,
                      style: Theme.of(context).textTheme.bodySmall?.copyWith(
                            color: AppTheme.inkMuted,
                          ),
                    ),
                    value: _useDraft,
                    onChanged: _running
                        ? null
                        : (v) => setState(() => _useDraft = v),
                  ),
                ],
                if (taskOptions.length > 1) ...[
                  const SizedBox(height: 16),
                  Text('태스크', style: Theme.of(context).textTheme.labelLarge),
                  const SizedBox(height: 8),
                  Wrap(
                    spacing: 8,
                    children: taskOptions
                        .map(
                          (t) => ChoiceChip(
                            label: Text(TaskLabels.label(t)),
                            selected: _benchTask == t,
                            onSelected: _running
                                ? null
                                : (_) {
                                    setState(() {
                                      _benchTask = t;
                                      _applyTaskDefaults(api);
                                    });
                                  },
                          ),
                        )
                        .toList(),
                  ),
                ],
                const SizedBox(height: 16),
                _buildTaskInput(api),
                if (TaskLabels.usesContext(_benchTask)) ...[
                  const SizedBox(height: 16),
                  Row(
                    children: [
                      MetricLabel(term: '컨텍스트'),
                      const SizedBox(width: 8),
                      Text(
                        '선택',
                        style: Theme.of(context).textTheme.labelLarge,
                      ),
                    ],
                  ),
                  const SizedBox(height: 8),
                  Wrap(
                    spacing: 8,
                    children: _ctxOptions
                        .map(
                          (c) => ChoiceChip(
                            label: Text(formatContext(c)),
                            selected: _ctx == c,
                            onSelected: _running
                                ? null
                                : (_) => setState(() => _ctx = c),
                          ),
                        )
                        .toList(),
                  ),
                  const SizedBox(height: 16),
                  SegmentedButton<FrbBenchMode>(
                    segments: const [
                      ButtonSegment(
                        value: FrbBenchMode.single,
                        label: Text('단일'),
                      ),
                      ButtonSegment(
                        value: FrbBenchMode.sweep,
                        label: Text('스윕'),
                      ),
                    ],
                    selected: {_mode},
                    onSelectionChanged: _running
                        ? null
                        : (s) => setState(() {
                              _mode = s.first;
                              if (_mode == FrbBenchMode.sweep) {
                                _resetSweepStepSelection();
                              }
                            }),
                  ),
                  if (_mode == FrbBenchMode.sweep) ...[
                    const SizedBox(height: 16),
                    Text(
                      '스윕 단계',
                      style: Theme.of(context).textTheme.labelLarge,
                    ),
                    const SizedBox(height: 8),
                    Wrap(
                      spacing: 4,
                      runSpacing: 0,
                      children: _ctxOptions.map((step) {
                        return FilterChip(
                          label: Text(formatContext(step)),
                          selected: _selectedSweepSteps.contains(step),
                          onSelected: _running
                              ? null
                              : (v) => setState(() {
                                    if (v) {
                                      _selectedSweepSteps.add(step);
                                    } else {
                                      _selectedSweepSteps.remove(step);
                                    }
                                  }),
                        );
                      }).toList(),
                    ),
                    if (_hasLargeSweepSteps) ...[
                      const SizedBox(height: 8),
                      Text(
                        '대형 컨텍스트(65536+)는 메모리를 많이 사용합니다. 필요 시에만 선택하세요.',
                        style: Theme.of(context).textTheme.bodySmall?.copyWith(
                              color: AppTheme.warning,
                            ),
                      ),
                    ],
                  ],
                ],
                const SizedBox(height: 20),
                Row(
                  children: [
                    FilledButton(
                      onPressed: _running || _profileId == null ? null : _start,
                      child: const Text('시작'),
                    ),
                    const SizedBox(width: 12),
                    OutlinedButton(
                      onPressed: _running ? _abort : null,
                      child: const Text('중단'),
                    ),
                    const SizedBox(width: 24),
                    _StateChip(state: _state),
                  ],
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 16),
        SizedBox(
          height: 200,
          child: Card(
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: _samples.isEmpty
                  ? const Center(child: Text('RAM/CPU 그래프 (측정 대기)'))
                  : _ResourceLineChart(samples: _samples),
            ),
          ),
        ),
        if (_result != null) ...[
          const SizedBox(height: 16),
          _buildResultCard(),
        ],
        if (_logs.isNotEmpty) ...[
          const SizedBox(height: 16),
          Card(
            child: ExpansionTile(
              title: const Text('이벤트 로그'),
              children: _logs
                  .map(
                    (l) => Padding(
                      padding: const EdgeInsets.symmetric(
                        horizontal: 16,
                        vertical: 2,
                      ),
                      child: Align(
                        alignment: Alignment.centerLeft,
                        child: Text(
                          l,
                          style: const TextStyle(
                            fontFamily: 'monospace',
                            fontSize: 11,
                          ),
                        ),
                      ),
                    ),
                  )
                  .toList(),
            ),
          ),
        ],
        if (profile != null)
          Padding(
            padding: const EdgeInsets.only(top: 8),
            child: Text(
              '${profile.backend} · ${TaskLabels.label(profile.modelType)} · '
              '컨텍스트 ${formatContext(profile.contextMin)}–${formatContext(profile.contextMax)}',
              style: Theme.of(context).textTheme.bodySmall?.copyWith(
                    color: AppTheme.inkMuted,
                  ),
            ),
          ),
      ],
    );
  }
}

class _ResultMetric extends StatelessWidget {
  final String label;
  final String value;
  final String term;

  const _ResultMetric({
    required this.label,
    required this.value,
    required this.term,
  });

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        children: [
          MetricLabel(term: term),
          const SizedBox(width: 8),
          Text(value),
        ],
      ),
    );
  }
}

class _StateChip extends StatelessWidget {
  final String state;

  const _StateChip({required this.state});

  @override
  Widget build(BuildContext context) {
    final color = switch (state) {
      'Ready' || 'Idle' => AppTheme.success,
      'Busy' || 'Loading' || 'Spawning' => AppTheme.warning,
      'Aborted' || 'Error' => AppTheme.error,
      _ => AppTheme.inkMuted,
    };
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.15),
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: color.withValues(alpha: 0.4)),
      ),
      child: Text(state, style: TextStyle(color: color, fontSize: 12)),
    );
  }
}

class _ResourceLineChart extends StatelessWidget {
  final List<FrbResourceSample> samples;

  const _ResourceLineChart({required this.samples});

  @override
  Widget build(BuildContext context) {
    final ramSpots = samples.asMap().entries.map((e) {
      return FlSpot(
        e.key.toDouble(),
        e.value.physFootprintBytes.toDouble() / (1024 * 1024 * 1024),
      );
    }).toList();
    final cpuSpots = samples.asMap().entries.map((e) {
      return FlSpot(e.key.toDouble(), e.value.cpuPct);
    }).toList();

    final maxRam = ramSpots.isEmpty
        ? 1.0
        : ramSpots.map((s) => s.y).reduce((a, b) => a > b ? a : b) * 1.1;

    return LineChart(
      LineChartData(
        minY: 0,
        maxY: maxRam,
        gridData: const FlGridData(show: true, drawVerticalLine: false),
        titlesData: const FlTitlesData(
          leftTitles: AxisTitles(
            sideTitles: SideTitles(showTitles: true, reservedSize: 40),
          ),
          bottomTitles: AxisTitles(),
          topTitles: AxisTitles(),
          rightTitles: AxisTitles(),
        ),
        lineBarsData: [
          LineChartBarData(
            spots: ramSpots,
            color: AppTheme.primary,
            barWidth: 2,
            dotData: const FlDotData(show: false),
          ),
          LineChartBarData(
            spots: cpuSpots,
            color: AppTheme.tierSluggish,
            barWidth: 2,
            dotData: const FlDotData(show: false),
          ),
        ],
      ),
    );
  }
}