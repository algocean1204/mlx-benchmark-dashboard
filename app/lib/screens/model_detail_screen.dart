import 'package:app/services/aidash_api.dart';
import 'package:app/src/rust/api.dart';
import 'package:app/task_labels.dart';
import 'package:app/utils/formatters.dart';
import 'package:app/theme/app_theme.dart';
import 'package:app/widgets/draft_badge.dart';
import 'package:app/widgets/error_card.dart';
import 'package:app/widgets/metric_label.dart';
import 'package:app/widgets/tier_badge.dart';
import 'package:fl_chart/fl_chart.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:url_launcher/url_launcher.dart';

class ModelDetailScreen extends StatefulWidget {
  final String? modelId;

  const ModelDetailScreen({super.key, this.modelId});

  @override
  State<ModelDetailScreen> createState() => _ModelDetailScreenState();
}

class _ModelDetailScreenState extends State<ModelDetailScreen> {
  FrbModelStats? _stats;
  List<FrbRunListRow> _runs = [];
  List<FrbOverviewRow> _models = [];
  String? _selectedId;
  String? _profileTask;
  String? _linkedDraftModel;
  List<String> _drafterProfiles = [];
  bool _loading = true;
  String? _error;

  List<int> _measurableContexts = [];
  int? _selectedEvalContext;
  bool _evalRunning = false;
  int _evalProgressIndex = 0;
  int _evalProgressTotal = 3;
  int? _evalTotalScore;
  List<FrbEvalTemplateItemResult> _evalItems = [];
  List<FrbEvalTemplateHistoryEntry> _evalHistory = [];
  String? _evalError;

  @override
  void initState() {
    super.initState();
    _selectedId = widget.modelId;
    _load();
  }

  @override
  void didUpdateWidget(covariant ModelDetailScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.modelId != oldWidget.modelId && widget.modelId != null) {
      _selectedId = widget.modelId;
      _load();
    }
  }

  Future<void> _load() async {
    setState(() => _loading = true);
    final api = context.read<AidashApi>();
    try {
      final overview = api.statsOverview();
      final id = _selectedId ??
          (overview.isNotEmpty ? overview.first.profileId : null);
      if (id == null) {
        if (mounted) setState(() => _loading = false);
        return;
      }
      final stats = api.statsModel(id: id);
      final runs = api.listRuns(model: id);
      final profiles = api.listProfiles();
      final drafters = api.listDrafterProfiles();
      final profile = profiles.cast<FrbProfileRow?>().firstWhere(
            (p) => p?.id == id,
            orElse: () => null,
          );
      final measurable = api.evalTemplateMeasurableContexts(profileId: id);
      final history = api.evalTemplateHistory(profileId: id);
      if (!mounted) return;
      setState(() {
        _models = overview;
        _selectedId = id;
        _stats = stats;
        _runs = runs;
        _profileTask = profile?.modelType;
        _linkedDraftModel = profile?.draftModel;
        _drafterProfiles = drafters;
        _measurableContexts = measurable;
        _selectedEvalContext = measurable.isNotEmpty
            ? measurable.firstWhere(
                (c) => c == 4096,
                orElse: () => measurable.first,
              )
            : null;
        _evalHistory = history;
        _loading = false;
        _error = null;
        _evalError = null;
      });
    } catch (e) {
      if (mounted) {
        setState(() {
          _loading = false;
          _error = e.toString();
        });
      }
    }
  }

  Future<void> _confirmDelete(FrbRunListRow run) async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('런 삭제'),
        content: Text(
          '런 #${platformIntToInt(run.runId)} (${run.displayName})을(를) 삭제할까요?',
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
    if (ok != true || !mounted) return;

    final api = context.read<AidashApi>();
    final runId = platformIntToInt(run.runId);
    final snapshot = run;

    setState(() => _runs.removeWhere((r) => platformIntToInt(r.runId) == runId));

    var undone = false;
    Future<void>.delayed(const Duration(seconds: 5), () {
      if (!undone && mounted) {
        api.deleteRun(id: runId);
      }
    });

    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        duration: const Duration(seconds: 5),
        content: Text('런 #$runId 삭제됨'),
        action: SnackBarAction(
          label: '실행 취소',
          onPressed: () {
            undone = true;
            setState(() {
              if (!_runs.any((r) => platformIntToInt(r.runId) == runId)) {
                _runs = [..._runs, snapshot]
                  ..sort(
                    (a, b) => platformIntToInt(b.runId)
                        .compareTo(platformIntToInt(a.runId)),
                  );
              }
            });
          },
        ),
      ),
    );
  }

  Future<void> _onDraftModelChanged(String? draftId) async {
    final id = _selectedId;
    if (id == null) return;
    final normalized = draftId == '__none__' ? null : draftId;
    if (normalized == _linkedDraftModel) return;

    try {
      context.read<AidashApi>().profileSetDraftModel(
            profileId: id,
            draftModel: normalized,
          );
      if (!mounted) return;
      setState(() => _linkedDraftModel = normalized);
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            normalized == null
                ? '보조 모델 연결 해제됨'
                : '보조 모델 연결됨: $normalized',
          ),
        ),
      );
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    }
  }

  Future<void> _onTaskChanged(String? newTask) async {
    if (newTask == null || newTask == _profileTask || _selectedId == null) return;

    final adjust = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('백엔드 재조정'),
        content: Text(
          '태스크를 "${TaskLabels.label(newTask)}"(으)로 변경합니다.\n'
          'infer_backend 규칙으로 백엔드도 함께 재조정할까요?',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('아니오'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('예'),
          ),
        ],
      ),
    );
    if (adjust == null || !mounted) return;

    try {
      context.read<AidashApi>().profileSetTask(
            profileId: _selectedId!,
            task: newTask,
            adjustBackend: adjust,
          );
      setState(() => _profileTask = newTask);
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('태스크 변경됨: ${TaskLabels.label(newTask)}')),
      );
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    }
  }

  bool get _supportsEval {
    final task = _profileTask;
    return task == 'llm' || task == 'multimodal';
  }

  Future<void> _runEval() async {
    final id = _selectedId;
    final ctx = _selectedEvalContext;
    if (id == null || ctx == null || _evalRunning) return;

    if (ctx >= 65536) {
      final proceed = await showDialog<bool>(
        context: context,
        builder: (dialogCtx) => AlertDialog(
          title: const Text('대형 컨텍스트 평가'),
          content: Text(
            '${formatContext(ctx)} 컨텍스트 평가는 수 분~수십 분이 걸릴 수 있으며 '
            '메모리 사용량이 큽니다.'
            '${ctx >= 524288 ? ' 512K는 수십 분, 1M은 1시간 이상 걸릴 수 있습니다.' : ''} '
            '계속할까요?',
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(dialogCtx, false),
              child: const Text('취소'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(dialogCtx, true),
              child: const Text('실행'),
            ),
          ],
        ),
      );
      if (proceed != true || !mounted) return;
    }

    setState(() {
      _evalRunning = true;
      _evalProgressIndex = 0;
      _evalProgressTotal = 3;
      _evalTotalScore = null;
      _evalItems = [];
      _evalError = null;
    });

    final api = context.read<AidashApi>();
    try {
      await for (final event in api.evalTemplateRun(
        profileId: id,
        contextSize: ctx,
      )) {
        if (!mounted) return;
        event.when(
          started: (templateId, index, total) {
            setState(() {
              _evalProgressIndex = index;
              _evalProgressTotal = total;
            });
          },
          completed: (templateId, score, elapsedMs) {},
          finished: (totalScore, items) {
            setState(() {
              _evalTotalScore = totalScore;
              _evalItems = items;
              _evalRunning = false;
            });
          },
          log: (message) {},
        );
      }
      if (!mounted) return;
      final history = api.evalTemplateHistory(profileId: id);
      setState(() {
        _evalHistory = history;
        _evalRunning = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _evalRunning = false;
        _evalError = e.toString();
      });
    }
  }

  Future<void> _openHf(String? url) async {
    if (url == null) return;
    final uri = Uri.parse(url);
    if (await canLaunchUrl(uri)) {
      await launchUrl(uri, mode: LaunchMode.externalApplication);
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_loading) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_error != null) {
      return ListView(
        padding: const EdgeInsets.all(24),
        children: [
          ErrorCard(message: _error!, onRetry: _load),
        ],
      );
    }
    if (_stats == null) {
      return const Center(child: Text('모델을 선택하세요.'));
    }

    final stats = _stats!;
    return ListView(
      padding: const EdgeInsets.all(24),
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                '모델 상세',
                style: Theme.of(context).textTheme.headlineSmall?.copyWith(
                      fontWeight: FontWeight.bold,
                    ),
              ),
            ),
            if (_models.isNotEmpty)
              DropdownMenu<String>(
                initialSelection: _selectedId,
                dropdownMenuEntries: _models
                    .map(
                      (m) => DropdownMenuEntry(
                        value: m.profileId,
                        label: m.displayName,
                      ),
                    )
                    .toList(),
                onSelected: (v) {
                  if (v != null) {
                    setState(() => _selectedId = v);
                    _load();
                  }
                },
              ),
            if (stats.hfUrl != null) ...[
              const SizedBox(width: 8),
              FilledButton.tonalIcon(
                onPressed: () => _openHf(stats.hfUrl),
                icon: const Icon(Icons.open_in_new, size: 18),
                label: const Text('HF'),
              ),
            ],
          ],
        ),
        const SizedBox(height: 8),
        Text(
          stats.profileId,
          style: Theme.of(context).textTheme.bodySmall?.copyWith(
                color: AppTheme.inkMuted,
              ),
        ),
        if (_profileTask != null) ...[
          const SizedBox(height: 12),
          Row(
            children: [
              const Text('태스크'),
              const SizedBox(width: 16),
              DropdownMenu<String>(
                initialSelection: _profileTask,
                dropdownMenuEntries: TaskLabels.allTasks
                    .map(
                      (t) => DropdownMenuEntry(
                        value: t,
                        label: TaskLabels.label(t),
                      ),
                    )
                    .toList(),
                onSelected: _onTaskChanged,
              ),
            ],
          ),
        ],
        if (_profileTask != null && _profileTask != 'drafter') ...[
          const SizedBox(height: 12),
          Row(
            children: [
              const Text('보조 모델(drafter)'),
              const SizedBox(width: 16),
              Expanded(
                child: DropdownMenu<String>(
                  initialSelection: _linkedDraftModel ?? '__none__',
                  dropdownMenuEntries: [
                    const DropdownMenuEntry(
                      value: '__none__',
                      label: '없음',
                    ),
                    ..._drafterProfiles.map(
                      (id) => DropdownMenuEntry(value: id, label: id),
                    ),
                  ],
                  onSelected: _onDraftModelChanged,
                ),
              ),
            ],
          ),
        ],
        const SizedBox(height: 16),
        Card(
          child: Padding(
            padding: const EdgeInsets.all(20),
            child: Row(
              children: [
                _StatTile(
                  label: '총 런',
                  value: '${platformIntToInt(stats.totalRuns)}',
                ),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      const MetricLabel(term: 'TPS'),
                      const SizedBox(height: 4),
                      Row(
                        children: [
                          Text(
                            stats.currentDecodeTps?.toStringAsFixed(1) ?? '—',
                            style: Theme.of(context).textTheme.titleMedium,
                          ),
                          if (stats.currentDecodeTps != null) ...[
                            const SizedBox(width: 8),
                            TierBadge(
                              decodeTps: stats.currentDecodeTps,
                              tier: stats.currentTier,
                              compact: true,
                            ),
                          ],
                        ],
                      ),
                    ],
                  ),
                ),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      const MetricLabel(term: 'Peak RAM'),
                      const SizedBox(height: 4),
                      Text(
                        formatBytesInt(
                          platformIntToInt(stats.peakPhysFootprintBytes),
                        ),
                        style: Theme.of(context).textTheme.titleMedium,
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
        if (_supportsEval && _measurableContexts.isNotEmpty) ...[
          const SizedBox(height: 16),
          Text('성능 평가', style: Theme.of(context).textTheme.titleMedium),
          const SizedBox(height: 8),
          _EvalSection(
            contexts: _measurableContexts,
            selectedContext: _selectedEvalContext,
            running: _evalRunning,
            progressIndex: _evalProgressIndex,
            progressTotal: _evalProgressTotal,
            totalScore: _evalTotalScore,
            items: _evalItems,
            history: _evalHistory,
            error: _evalError,
            onContextSelected: (ctx) => setState(() => _selectedEvalContext = ctx),
            onRun: _runEval,
          ),
        ],
        const SizedBox(height: 16),
        Row(
          children: [
            const MetricLabel(term: 'TPS'),
            const SizedBox(width: 8),
            Text('컨텍스트별', style: Theme.of(context).textTheme.titleMedium),
          ],
        ),
        const SizedBox(height: 8),
        SizedBox(
          height: 220,
          child: Card(
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: _TpsChart(rows: stats.byContext),
            ),
          ),
        ),
        const SizedBox(height: 16),
        Row(
          children: [
            const MetricLabel(term: 'Peak RAM'),
            const SizedBox(width: 8),
            Text('컨텍스트별', style: Theme.of(context).textTheme.titleMedium),
          ],
        ),
        const SizedBox(height: 8),
        SizedBox(
          height: 220,
          child: Card(
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: _RamChart(rows: stats.byContext),
            ),
          ),
        ),
        const SizedBox(height: 16),
        Text('통계 표', style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: 8),
        Card(
          child: DataTable(
            columns: const [
              DataColumn(label: Text('컨텍스트')),
              DataColumn(label: Text('TPS min')),
              DataColumn(label: Text('TPS avg')),
              DataColumn(label: Text('TPS max')),
              DataColumn(label: Text('TTFT avg')),
              DataColumn(label: MetricLabel(term: 'Peak RAM')),
              DataColumn(label: Text('런 수')),
            ],
            rows: stats.byContext
                .map(
                  (r) => DataRow(
                    cells: [
                      DataCell(Text(formatContext(platformIntToInt(r.contextSize)))),
                      DataCell(Text(r.decodeTpsMin.toStringAsFixed(1))),
                      DataCell(Text(r.decodeTpsAvg.toStringAsFixed(1))),
                      DataCell(Text(r.decodeTpsMax.toStringAsFixed(1))),
                      DataCell(Text('${r.ttftAvgMs.toStringAsFixed(0)} ms')),
                      DataCell(
                        Text(
                          formatBytesInt(
                            platformIntToInt(r.peakPhysAvgBytes),
                          ),
                        ),
                      ),
                      DataCell(Text('${platformIntToInt(r.runCount)}')),
                    ],
                  ),
                )
                .toList(),
          ),
        ),
        const SizedBox(height: 16),
        Text('런 목록', style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: 8),
        Card(
          child: Column(
            children: _runs
                .map(
                  (run) => ListTile(
                    title: Text(
                      '#${platformIntToInt(run.runId)} · ${run.kind}',
                    ),
                    subtitle: Text(
                      '컨텍스트 ${formatContext(platformIntToInt(run.contextSize))} · ${run.status}',
                    ),
                    trailing: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if (run.useDraft == true) ...[
                          const DraftBadge(),
                          const SizedBox(width: 4),
                        ],
                        TierBadge(
                          decodeTps: run.decodeTps,
                          tier: run.tier,
                          compact: true,
                        ),
                        IconButton(
                          icon: const Icon(Icons.delete_outline),
                          onPressed: () => _confirmDelete(run),
                        ),
                      ],
                    ),
                  ),
                )
                .toList(),
          ),
        ),
      ],
    );
  }
}

class _EvalSection extends StatelessWidget {
  final List<int> contexts;
  final int? selectedContext;
  final bool running;
  final int progressIndex;
  final int progressTotal;
  final int? totalScore;
  final List<FrbEvalTemplateItemResult> items;
  final List<FrbEvalTemplateHistoryEntry> history;
  final String? error;
  final ValueChanged<int> onContextSelected;
  final VoidCallback onRun;

  const _EvalSection({
    required this.contexts,
    required this.selectedContext,
    required this.running,
    required this.progressIndex,
    required this.progressTotal,
    required this.totalScore,
    required this.items,
    required this.history,
    required this.error,
    required this.onContextSelected,
    required this.onRun,
  });

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: contexts.map((ctx) {
                final selected = ctx == selectedContext;
                return ChoiceChip(
                  label: Text(formatContext(ctx)),
                  selected: selected,
                  onSelected: running
                      ? null
                      : (_) => onContextSelected(ctx),
                );
              }).toList(),
            ),
            const SizedBox(height: 12),
            Row(
              children: [
                FilledButton.icon(
                  onPressed: running || selectedContext == null ? null : onRun,
                  icon: running
                      ? const SizedBox(
                          width: 16,
                          height: 16,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Icon(Icons.play_arrow, size: 18),
                  label: Text(running ? '평가 중…' : '평가 실행'),
                ),
                if (running) ...[
                  const SizedBox(width: 16),
                  Text(
                    '$progressIndex / $progressTotal 프롬프트',
                    style: Theme.of(context).textTheme.bodySmall?.copyWith(
                          color: AppTheme.inkMuted,
                        ),
                  ),
                ],
              ],
            ),
            if (error != null) ...[
              const SizedBox(height: 12),
              Text(
                error!,
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ],
            if (totalScore != null) ...[
              const SizedBox(height: 16),
              Row(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  Text(
                    '$totalScore',
                    style: Theme.of(context).textTheme.displaySmall?.copyWith(
                          fontWeight: FontWeight.bold,
                          color: evalScoreColor(totalScore!),
                        ),
                  ),
                  const SizedBox(width: 8),
                  Padding(
                    padding: const EdgeInsets.only(bottom: 8),
                    child: Text(
                      '/ 100',
                      style: Theme.of(context).textTheme.titleMedium?.copyWith(
                            color: AppTheme.inkMuted,
                          ),
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 8),
              ...items.map(
                (item) => Padding(
                  padding: const EdgeInsets.symmetric(vertical: 4),
                  child: Row(
                    children: [
                      Expanded(
                        child: Text(
                          '${item.description} (${item.templateId})',
                          style: Theme.of(context).textTheme.bodySmall,
                        ),
                      ),
                      Text(
                        '${item.score}점',
                        style: TextStyle(
                          fontWeight: FontWeight.w600,
                          color: evalScoreColor(item.score),
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ],
            if (history.isNotEmpty) ...[
              const SizedBox(height: 16),
              Text(
                '이전 평가',
                style: Theme.of(context).textTheme.labelLarge,
              ),
              const SizedBox(height: 8),
              ...history.take(5).map(
                    (entry) => ListTile(
                      dense: true,
                      contentPadding: EdgeInsets.zero,
                      title: Text(
                        '${formatContext(entry.contextSize)} · ${entry.totalScore}점',
                      ),
                      subtitle: Text(
                        entry.items.map((i) => '${i.description} ${i.score}').join(' · '),
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                  ),
            ],
          ],
        ),
      ),
    );
  }
}

class _StatTile extends StatelessWidget {
  final String label;
  final String value;

  const _StatTile({required this.label, required this.value});

  @override
  Widget build(BuildContext context) {
    return Expanded(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(label, style: Theme.of(context).textTheme.labelSmall),
          const SizedBox(height: 4),
          Text(value, style: Theme.of(context).textTheme.titleMedium),
        ],
      ),
    );
  }
}

class _TpsChart extends StatelessWidget {
  final List<FrbContextStatsRow> rows;

  const _TpsChart({required this.rows});

  @override
  Widget build(BuildContext context) {
    if (rows.isEmpty) return const Center(child: Text('데이터 없음'));

    final sorted = [...rows]
      ..sort(
        (a, b) => platformIntToInt(a.contextSize)
            .compareTo(platformIntToInt(b.contextSize)),
      );

    final spots = sorted.asMap().entries.map((e) {
      return FlSpot(
        platformIntToInt(e.value.contextSize).toDouble(),
        e.value.decodeTpsAvg,
      );
    }).toList();

    final maxY = sorted
            .map((r) => r.decodeTpsMax)
            .reduce((a, b) => a > b ? a : b) *
        1.2;
    final interval = maxY > 50 ? 20.0 : 10.0;

    return LineChart(
      LineChartData(
        minY: 0,
        maxY: maxY,
        borderData: FlBorderData(
          show: true,
          border: Border.all(color: AppTheme.border, width: 1),
        ),
        lineTouchData: const LineTouchData(enabled: false),
        gridData: const FlGridData(show: true, drawVerticalLine: false),
        titlesData: FlTitlesData(
          bottomTitles: AxisTitles(
            sideTitles: SideTitles(
              showTitles: true,
              interval: 1,
              getTitlesWidget: (v, meta) {
                final ctx = v.toInt();
                final has = sorted.any(
                  (r) => platformIntToInt(r.contextSize) == ctx,
                );
                if (!has) return const SizedBox();
                return Padding(
                  padding: const EdgeInsets.only(top: 4),
                  child: Text(
                    formatContext(ctx),
                    style: const TextStyle(fontSize: 10, color: AppTheme.inkMuted),
                  ),
                );
              },
            ),
          ),
          leftTitles: AxisTitles(
            sideTitles: SideTitles(
              showTitles: true,
              reservedSize: 40,
              interval: interval,
              getTitlesWidget: (v, _) {
                if (v % interval != 0) return const SizedBox.shrink();
                return Text(
                  v.toInt().toString(),
                  style:
                      const TextStyle(fontSize: 10, color: AppTheme.inkMuted),
                );
              },
            ),
          ),
          topTitles: const AxisTitles(),
          rightTitles: const AxisTitles(),
        ),
        lineBarsData: [
          LineChartBarData(
            spots: spots,
            isCurved: true,
            color: AppTheme.primary,
            barWidth: 3,
            dotData: const FlDotData(show: true),
          ),
        ],
      ),
    );
  }
}

class _RamChart extends StatelessWidget {
  final List<FrbContextStatsRow> rows;

  const _RamChart({required this.rows});

  @override
  Widget build(BuildContext context) {
    if (rows.isEmpty) return const Center(child: Text('데이터 없음'));

    final sorted = [...rows]
      ..sort(
        (a, b) => platformIntToInt(a.contextSize)
            .compareTo(platformIntToInt(b.contextSize)),
      );

    final maxY = sorted
        .map((r) => platformIntToInt(r.peakPhysFootprintBytes))
        .fold<int>(0, (a, b) => a > b ? a : b)
        .toDouble();
    final interval = maxY > 10 * 1024 * 1024 * 1024 ? maxY / 4 : maxY / 3;

    return BarChart(
      BarChartData(
        maxY: maxY * 1.15,
        borderData: FlBorderData(
          show: true,
          border: Border.all(color: AppTheme.border, width: 1),
        ),
        barTouchData: BarTouchData(
          enabled: true,
          touchTooltipData: BarTouchTooltipData(
            getTooltipColor: (_) => AppTheme.surface,
            tooltipBorder: const BorderSide(color: AppTheme.border),
            getTooltipItem: (group, groupIndex, rod, rodIndex) {
              return BarTooltipItem(
                formatBytesInt(rod.toY.toInt()),
                const TextStyle(color: AppTheme.ink, fontSize: 12),
              );
            },
          ),
        ),
        gridData: const FlGridData(show: true, drawVerticalLine: false),
        titlesData: FlTitlesData(
          bottomTitles: AxisTitles(
            sideTitles: SideTitles(
              showTitles: true,
              getTitlesWidget: (v, _) {
                final i = v.toInt();
                if (i < 0 || i >= sorted.length) return const SizedBox();
                return Padding(
                  padding: const EdgeInsets.only(top: 4),
                  child: Text(
                    formatContext(platformIntToInt(sorted[i].contextSize)),
                    style: const TextStyle(fontSize: 10, color: AppTheme.inkMuted),
                  ),
                );
              },
            ),
          ),
          leftTitles: AxisTitles(
            sideTitles: SideTitles(
              showTitles: true,
              reservedSize: 52,
              interval: interval > 0 ? interval : null,
              getTitlesWidget: (v, _) {
                if (v < 0 || v > maxY * 1.2) return const SizedBox();
                return Text(
                  formatBytesInt(v.toInt()),
                  style: const TextStyle(fontSize: 9, color: AppTheme.inkMuted),
                );
              },
            ),
          ),
          topTitles: const AxisTitles(),
          rightTitles: const AxisTitles(),
        ),
        barGroups: sorted.asMap().entries.map((e) {
          final peak = platformIntToInt(e.value.peakPhysFootprintBytes);
          return BarChartGroupData(
            x: e.key,
            barRods: [
              BarChartRodData(
                toY: peak.toDouble(),
                color: AppTheme.primary,
                width: 28,
                borderRadius: BorderRadius.circular(4),
              ),
            ],
          );
        }).toList(),
      ),
    );
  }
}