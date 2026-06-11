import 'package:app/services/aidash_api.dart';
import 'package:app/src/rust/api.dart';
import 'package:app/theme/app_theme.dart';
import 'package:app/utils/formatters.dart';
import 'package:app/widgets/error_card.dart';
import 'package:app/widgets/metric_label.dart';
import 'package:app/widgets/tier_badge.dart';
import 'package:fl_chart/fl_chart.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

class CompareScreen extends StatefulWidget {
  const CompareScreen({super.key});

  @override
  State<CompareScreen> createState() => _CompareScreenState();
}

class _ContextCompareRow {
  final int contextSize;
  final double? decodeTps;
  final double ttftAvgMs;
  final int peakRamBytes;
  final String? measuredAt;

  const _ContextCompareRow({
    required this.contextSize,
    required this.decodeTps,
    required this.ttftAvgMs,
    required this.peakRamBytes,
    required this.measuredAt,
  });
}

class _CompareScreenState extends State<CompareScreen> {
  List<FrbOverviewRow> _models = [];
  final Set<String> _selected = {};
  int _context = 4096;
  List<FrbCompareRow> _rows = [];
  List<_ContextCompareRow> _contextRows = [];
  String? _contextCompareModel;
  bool _loading = false;
  String? _error;

  static const _contextOptions = [2048, 4096, 8192, 16384, 32768];

  bool get _isContextCompareMode => _selected.length == 1;

  @override
  void initState() {
    super.initState();
    _loadModels();
  }

  Future<void> _loadModels() async {
    final api = context.read<AidashApi>();
    try {
      final rows = api.statsOverview();
      if (!mounted) return;
      setState(() {
        _models = rows;
        _error = null;
      });
      _refreshCompare();
    } catch (e) {
      if (!mounted) return;
      setState(() => _error = e.toString());
    }
  }

  Future<void> _refreshCompare() async {
    if (_selected.isEmpty) {
      setState(() {
        _rows = [];
        _contextRows = [];
        _contextCompareModel = null;
        _loading = false;
        _error = null;
      });
      return;
    }

    if (_isContextCompareMode) {
      final modelId = _selected.first;
      setState(() {
        _loading = true;
        _error = null;
        _rows = [];
      });
      final api = context.read<AidashApi>();
      try {
        final stats = api.statsModel(id: modelId);
        final runs = api.listRuns(model: modelId);
        final latestByContext = <int, FrbRunListRow>{};
        for (final run in runs) {
          if (run.contextSize == null || run.status != 'completed') continue;
          final ctx = platformIntToInt(run.contextSize!);
          final existing = latestByContext[ctx];
          if (existing == null ||
              platformIntToInt(run.runId) > platformIntToInt(existing.runId)) {
            latestByContext[ctx] = run;
          }
        }
        final contextRows = stats.byContext.map((row) {
          final ctx = platformIntToInt(row.contextSize);
          final latest = latestByContext[ctx];
          return _ContextCompareRow(
            contextSize: ctx,
            decodeTps: row.decodeTpsAvg,
            ttftAvgMs: row.ttftAvgMs,
            peakRamBytes: platformIntToInt(row.peakPhysAvgBytes),
            measuredAt: latest?.endedAt,
          );
        }).toList()
          ..sort((a, b) => a.contextSize.compareTo(b.contextSize));
        if (!mounted) return;
        setState(() {
          _contextRows = contextRows;
          _contextCompareModel = stats.displayName;
          _loading = false;
        });
      } catch (e) {
        if (!mounted) return;
        setState(() {
          _loading = false;
          _error = e.toString();
        });
      }
      return;
    }

    if (_selected.length < 2) {
      setState(() {
        _rows = [];
        _contextRows = [];
        _contextCompareModel = null;
        _loading = false;
        _error = null;
      });
      return;
    }

    setState(() {
      _loading = true;
      _error = null;
      _contextRows = [];
      _contextCompareModel = null;
    });
    final api = context.read<AidashApi>();
    try {
      final rows = api.compare(models: _selected.toList(), ctx: _context);
      if (!mounted) return;
      setState(() {
        _rows = rows;
        _loading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _loading = false;
        _error = e.toString();
      });
    }
  }

  void _toggleModel(String id, bool selected) {
    setState(() {
      if (selected) {
        _selected.add(id);
      } else {
        _selected.remove(id);
      }
    });
    _refreshCompare();
  }

  void _setContext(int ctx) {
    setState(() => _context = ctx);
    _refreshCompare();
  }

  @override
  Widget build(BuildContext context) {
    final api = context.read<AidashApi>();
    final bands = tierBands(api);
    final theme = Theme.of(context);

    return ListView(
      padding: const EdgeInsets.all(24),
      children: [
        Text(
          '모델 비교',
          style: theme.textTheme.headlineSmall?.copyWith(
            fontWeight: FontWeight.bold,
          ),
        ),
        const SizedBox(height: 8),
        Text(
          '저장된 측정 결과 기반 비교입니다 (새 측정은 \'벤치\' 탭)',
          style: theme.textTheme.bodyMedium?.copyWith(
            color: AppTheme.inkMuted,
          ),
        ),
        const SizedBox(height: 16),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: _contextOptions
              .map(
                (c) => ChoiceChip(
                  label: Text('$c'),
                  selected: _context == c,
                  onSelected: _isContextCompareMode ? null : (_) => _setContext(c),
                ),
              )
              .toList(),
        ),
        const SizedBox(height: 16),
        Card(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Wrap(
              spacing: 8,
              runSpacing: 8,
              children: _models
                  .map(
                    (m) => FilterChip(
                      label: Text(m.displayName),
                      selected: _selected.contains(m.profileId),
                      onSelected: (v) => _toggleModel(m.profileId, v),
                    ),
                  )
                  .toList(),
            ),
          ),
        ),
        if (_loading) ...[
          const SizedBox(height: 24),
          const Center(child: CircularProgressIndicator()),
        ],
        if (_error != null) ...[
          const SizedBox(height: 16),
          ErrorCard(message: _error!, onRetry: _refreshCompare),
        ],
        if (_isContextCompareMode && !_loading) ...[
          const SizedBox(height: 16),
          Text(
            '같은 모델의 컨텍스트별 측정 비교입니다.',
            style: theme.textTheme.bodySmall?.copyWith(color: AppTheme.inkMuted),
          ),
        ],
        if (_selected.isEmpty && !_loading) ...[
          const SizedBox(height: 16),
          Text(
            '모델을 선택하면 비교됩니다. 2개 이상은 모델 간, 1개는 컨텍스트별 비교입니다.',
            style: theme.textTheme.bodySmall?.copyWith(color: AppTheme.inkMuted),
          ),
        ],
        if (_contextRows.isNotEmpty) ...[
          const SizedBox(height: 24),
          if (_contextCompareModel != null)
            Text(
              _contextCompareModel!,
              style: theme.textTheme.titleMedium,
            ),
          const SizedBox(height: 8),
          Card(
            child: _ContextCompareTable(rows: _contextRows),
          ),
        ],
        if (_rows.isNotEmpty) ...[
          if (_rows.map((r) => r.modelType).toSet().length > 1) ...[
            const SizedBox(height: 16),
            Card(
              color: AppTheme.warning.withValues(alpha: 0.08),
              child: const Padding(
                padding: EdgeInsets.all(12),
                child: Text(
                  '서로 다른 태스크의 모델입니다 — 수치 직접 비교는 참고용입니다',
                ),
              ),
            ),
          ],
          const SizedBox(height: 24),
          _StatCards(rows: _rows),
          const SizedBox(height: 16),
          SizedBox(
            height: 260,
            child: Card(
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: _CompareChart(rows: _rows, bands: bands),
              ),
            ),
          ),
        ],
      ],
    );
  }
}

class _StatCards extends StatelessWidget {
  final List<FrbCompareRow> rows;

  const _StatCards({required this.rows});

  double? _bestTps() {
    final values = rows.map((r) => r.decodeTps).whereType<double>();
    if (values.isEmpty) return null;
    return values.reduce((a, b) => a > b ? a : b);
  }

  double? _bestTtft() {
    final values = rows.map((r) => r.ttftMs).whereType<double>();
    if (values.isEmpty) return null;
    return values.reduce((a, b) => a < b ? a : b);
  }

  int? _bestRam() {
    final values = [
      for (final r in rows)
        if (r.peakPhysFootprintBytes != null) r.peakPhysFootprintBytes!.toInt(),
    ];
    if (values.isEmpty) return null;
    return values.reduce((a, b) => a < b ? a : b);
  }

  static String formatMeasuredAt(String? raw) {
    if (raw == null) return '—';
    final ms = int.tryParse(raw);
    if (ms == null) return raw;
    final dt = DateTime.fromMillisecondsSinceEpoch(ms).toLocal();
    String two(int v) => v.toString().padLeft(2, '0');
    return '측정일 ${dt.year}-${two(dt.month)}-${two(dt.day)} ${two(dt.hour)}:${two(dt.minute)}';
  }

  @override
  Widget build(BuildContext context) {
    final bestTps = _bestTps();
    final bestTtft = _bestTtft();
    final bestRam = _bestRam();

    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: rows.map((r) {
        final hasData = r.decodeTps != null;
        final tpsWin = hasData && r.decodeTps == bestTps;
        final ttftWin =
            r.ttftMs != null && bestTtft != null && r.ttftMs == bestTtft;
        final ramWin = r.peakPhysFootprintBytes != null &&
            bestRam != null &&
            r.peakPhysFootprintBytes!.toInt() == bestRam;

        return Expanded(
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 6),
            child: Card(
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: hasData
                    ? Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            r.displayName,
                            style: Theme.of(context).textTheme.titleSmall,
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                          ),
                          const SizedBox(height: 8),
                          TierBadge(
                            decodeTps: r.decodeTps,
                            tier: r.tier,
                            large: true,
                          ),
                          const SizedBox(height: 12),
                          _StatLine(
                            term: 'TPS',
                            value: r.decodeTps!.toStringAsFixed(1),
                            highlight: tpsWin,
                          ),
                          _StatLine(
                            term: 'TTFT',
                            value: '${r.ttftMs?.toStringAsFixed(0) ?? '—'} ms',
                            highlight: ttftWin,
                            lowerIsBetter: true,
                          ),
                          _StatLine(
                            term: 'Peak RAM',
                            value: r.peakPhysFootprintBytes != null
                                ? formatBytesInt(
                                    r.peakPhysFootprintBytes!.toInt(),
                                  )
                                : '—',
                            highlight: ramWin,
                            lowerIsBetter: true,
                          ),
                          Padding(
                            padding: const EdgeInsets.symmetric(vertical: 2),
                            child: Text(
                              'tokens: ${r.tokensIn ?? '—'} / ${r.tokensOut ?? '—'}',
                              style: Theme.of(context).textTheme.bodySmall,
                            ),
                          ),
                          const SizedBox(height: 4),
                          Text(
                            _StatCards.formatMeasuredAt(r.measuredAt),
                            style: Theme.of(context).textTheme.labelSmall,
                          ),
                          const SizedBox(height: 4),
                          _ContextLabel(row: r),
                        ],
                      )
                    : Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            r.displayName,
                            style: Theme.of(context).textTheme.titleSmall,
                          ),
                          const SizedBox(height: 12),
                          Text(
                            '측정 기록 없음',
                            style: Theme.of(context)
                                .textTheme
                                .bodyMedium
                                ?.copyWith(color: AppTheme.inkMuted),
                          ),
                        ],
                      ),
              ),
            ),
          ),
        );
      }).toList(),
    );
  }
}

class _StatLine extends StatelessWidget {
  final String term;
  final String value;
  final bool highlight;
  final bool lowerIsBetter;

  const _StatLine({
    required this.term,
    required this.value,
    this.highlight = false,
    this.lowerIsBetter = false,
  });

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        children: [
          if (highlight)
            const Text('▲ ', style: TextStyle(color: AppTheme.primary, fontSize: 10)),
          MetricLabel(term: term),
          const SizedBox(width: 4),
          Text(
            value,
            style: Theme.of(context).textTheme.bodySmall?.copyWith(
                  fontWeight: highlight ? FontWeight.bold : FontWeight.normal,
                  color: highlight ? AppTheme.primary : AppTheme.ink,
                ),
          ),
        ],
      ),
    );
  }
}

class _ContextLabel extends StatelessWidget {
  final FrbCompareRow row;

  const _ContextLabel({required this.row});

  @override
  Widget build(BuildContext context) {
    final actual = platformIntToInt(row.contextActual);
    if (row.contextSubstituted) {
      final requested = platformIntToInt(row.contextRequested);
      return Tooltip(
        message: '요청 컨텍스트 $requested → 실제 $actual',
        child: Text(
          'ctx $actual*',
          style: Theme.of(context).textTheme.labelSmall?.copyWith(
                color: AppTheme.inkMuted,
              ),
        ),
      );
    }
    return Text(
      'ctx $actual',
      style: Theme.of(context).textTheme.labelSmall,
    );
  }
}

class _CompareChart extends StatelessWidget {
  final List<FrbCompareRow> rows;
  final List<({double min, double max, Color color, String label})> bands;

  const _CompareChart({required this.rows, required this.bands});

  @override
  Widget build(BuildContext context) {
    final dataRows = rows.where((r) => r.decodeTps != null).toList();
    if (dataRows.isEmpty) {
      return const Center(child: Text('비교할 TPS 데이터가 없습니다'));
    }

    final maxTps = dataRows
        .map((r) => r.decodeTps!)
        .fold<double>(100, (a, b) => a > b ? a : b);

    return Stack(
      children: [
        CustomPaint(
          size: Size.infinite,
          painter: _TierBandPainter(bands: bands, maxY: maxTps * 1.15),
        ),
        BarChart(
          BarChartData(
            maxY: maxTps * 1.15,
            gridData: FlGridData(
              show: true,
              drawVerticalLine: false,
              getDrawingHorizontalLine: (v) => FlLine(
                color: AppTheme.border,
                strokeWidth: 1,
              ),
            ),
            titlesData: FlTitlesData(
              bottomTitles: AxisTitles(
                sideTitles: SideTitles(
                  showTitles: true,
                  getTitlesWidget: (v, _) {
                    final i = v.toInt();
                    if (i < 0 || i >= dataRows.length) return const SizedBox();
                    return Padding(
                      padding: const EdgeInsets.only(top: 4),
                      child: Text(
                        dataRows[i].displayName.split('/').last,
                        style: const TextStyle(fontSize: 9, color: AppTheme.inkMuted),
                        overflow: TextOverflow.ellipsis,
                      ),
                    );
                  },
                ),
              ),
              leftTitles: AxisTitles(
                sideTitles: SideTitles(
                  showTitles: true,
                  reservedSize: 40,
                  interval: maxTps > 50 ? 20 : 10,
                  getTitlesWidget: (v, _) {
                    final iv = maxTps > 50 ? 20 : 10;
                    if (v % iv != 0) return const SizedBox.shrink();
                    return Text(
                      v.toInt().toString(),
                      style: const TextStyle(
                          fontSize: 10, color: AppTheme.inkMuted),
                    );
                  },
                ),
              ),
              topTitles: const AxisTitles(),
              rightTitles: const AxisTitles(),
            ),
            barGroups: dataRows.asMap().entries.map((e) {
              final tps = e.value.decodeTps ?? 0.0;
              return BarChartGroupData(
                x: e.key,
                barRods: [
                  BarChartRodData(
                    toY: tps,
                    color: AppTheme.primary,
                    width: 36,
                    borderRadius: BorderRadius.circular(4),
                  ),
                ],
              );
            }).toList(),
          ),
        ),
      ],
    );
  }
}

class _ContextCompareTable extends StatelessWidget {
  final List<_ContextCompareRow> rows;

  const _ContextCompareTable({required this.rows});

  @override
  Widget build(BuildContext context) {
    return SingleChildScrollView(
      scrollDirection: Axis.horizontal,
      child: DataTable(
        columns: const [
          DataColumn(label: Text('컨텍스트')),
          DataColumn(label: MetricLabel(term: 'TPS')),
          DataColumn(label: MetricLabel(term: 'TTFT')),
          DataColumn(label: MetricLabel(term: 'Peak RAM')),
          DataColumn(label: Text('측정일')),
        ],
        rows: rows
            .map(
              (r) => DataRow(
                cells: [
                  DataCell(Text('${r.contextSize}')),
                  DataCell(Text(r.decodeTps?.toStringAsFixed(1) ?? '—')),
                  DataCell(Text('${r.ttftAvgMs.toStringAsFixed(0)} ms')),
                  DataCell(Text(formatBytesInt(r.peakRamBytes))),
                  DataCell(Text(_StatCards.formatMeasuredAt(r.measuredAt))),
                ],
              ),
            )
            .toList(),
      ),
    );
  }
}

class _TierBandPainter extends CustomPainter {
  final List<({double min, double max, Color color, String label})> bands;
  final double maxY;

  _TierBandPainter({required this.bands, required this.maxY});

  @override
  void paint(Canvas canvas, Size size) {
    const leftPad = 40.0;
    const topPad = 8.0;
    const bottomPad = 28.0;
    final chartH = size.height - topPad - bottomPad;

    for (final band in bands) {
      final yTop = topPad + chartH * (1 - band.max / maxY);
      final yBot = topPad + chartH * (1 - band.min / maxY);
      final paint = Paint()..color = band.color;
      canvas.drawRect(
        Rect.fromLTRB(leftPad, yTop, size.width - 8, yBot),
        paint,
      );
    }
  }

  @override
  bool shouldRepaint(covariant _TierBandPainter old) =>
      old.maxY != maxY || old.bands != bands;
}