import 'package:app/services/aidash_api.dart';
import 'package:app/src/rust/api.dart';
import 'package:app/task_labels.dart';
import 'package:app/utils/formatters.dart';
import 'package:app/theme/app_theme.dart';
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
  bool _loading = true;
  String? _error;

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
      final profile = profiles.cast<FrbProfileRow?>().firstWhere(
            (p) => p?.id == id,
            orElse: () => null,
          );
      if (!mounted) return;
      setState(() {
        _models = overview;
        _selectedId = id;
        _stats = stats;
        _runs = runs;
        _profileTask = profile?.modelType;
        _loading = false;
        _error = null;
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