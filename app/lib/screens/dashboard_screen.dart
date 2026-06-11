import 'dart:async';

import 'package:app/metric_help.dart';
import 'package:app/services/aidash_api.dart';
import 'package:app/src/rust/api.dart';
import 'package:app/utils/formatters.dart';
import 'package:app/theme/app_theme.dart';
import 'package:app/task_labels.dart';
import 'package:app/widgets/error_card.dart';
import 'package:app/widgets/metric_label.dart';
import 'package:app/widgets/tier_badge.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:url_launcher/url_launcher.dart';

class DashboardScreen extends StatefulWidget {
  final ValueChanged<String>? onModelSelected;

  const DashboardScreen({super.key, this.onModelSelected});

  @override
  State<DashboardScreen> createState() => _DashboardScreenState();
}

class _DashboardScreenState extends State<DashboardScreen> {
  List<FrbOverviewRow> _leaderboard = [];
  List<FrbRunListRow> _recentRuns = [];
  FrbResourceSample? _resources;
  bool _loading = true;
  String? _error;
  StreamSubscription<FrbResourceSample>? _resourceSub;

  @override
  void initState() {
    super.initState();
    _load();
    _startResourceMonitor();
  }

  @override
  void dispose() {
    _resourceSub?.cancel();
    super.dispose();
  }

  void _startResourceMonitor() {
    final api = context.read<AidashApi>();
    _resourceSub = api.systemResources().listen((sample) {
      if (mounted) setState(() => _resources = sample);
    });
  }

  Future<void> _load() async {
    setState(() => _loading = true);
    final api = context.read<AidashApi>();
    try {
      final rows = api.statsOverview();
      final runs = api.listRuns();
      if (!mounted) return;
      setState(() {
        _leaderboard = rows;
        _recentRuns = runs.take(8).toList();
        _loading = false;
        _error = null;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _loading = false;
        _error = e.toString();
      });
    }
  }

  Future<void> _openUrl(String? url) async {
    if (url == null) return;
    final uri = Uri.parse(url);
    if (await canLaunchUrl(uri)) {
      await launchUrl(uri, mode: LaunchMode.externalApplication);
    }
  }

  @override
  Widget build(BuildContext context) {
    return RefreshIndicator(
      onRefresh: _load,
      child: _loading
          ? const Center(child: CircularProgressIndicator())
          : ListView(
              padding: const EdgeInsets.all(24),
              children: [
                Text(
                  '대시보드',
                  style: Theme.of(context).textTheme.headlineSmall?.copyWith(
                        fontWeight: FontWeight.bold,
                      ),
                ),
                const SizedBox(height: 16),
                if (_error != null) ...[
                  ErrorCard(message: _error!, onRetry: _load),
                  const SizedBox(height: 12),
                ],
                if (_resources != null) _ResourceCard(sample: _resources!),
                const SizedBox(height: 20),
                Text(
                  '모델 리더보드',
                  style: Theme.of(context).textTheme.titleMedium,
                ),
                const SizedBox(height: 8),
                Card(
                  child: _leaderboard.isEmpty
                      ? const Padding(
                          padding: EdgeInsets.all(24),
                          child: Text('측정된 모델이 없습니다.'),
                        )
                      : Column(
                          children: [
                            for (var i = 0; i < _leaderboard.length; i++)
                              _LeaderboardRow(
                                rank: i + 1,
                                row: _leaderboard[i],
                                onTap: () =>
                                    widget.onModelSelected?.call(
                                  _leaderboard[i].profileId,
                                ),
                                onOpenHf: () =>
                                    _openUrl(_leaderboard[i].hfUrl),
                              ),
                          ],
                        ),
                ),
                const SizedBox(height: 20),
                Text(
                  '최근 측정',
                  style: Theme.of(context).textTheme.titleMedium,
                ),
                const SizedBox(height: 8),
                Card(
                  child: _recentRuns.isEmpty
                      ? const Padding(
                          padding: EdgeInsets.all(24),
                          child: Text('최근 런이 없습니다.'),
                        )
                      : Column(
                          children: _recentRuns
                              .map((r) => _RecentRunRow(run: r))
                              .toList(),
                        ),
                ),
              ],
            ),
    );
  }
}

class _ResourceCard extends StatelessWidget {
  final FrbResourceSample sample;

  const _ResourceCard({required this.sample});

  @override
  Widget build(BuildContext context) {
    final used = sample.totalMemoryBytes - sample.sysAvailableBytes;
    final pct = sample.totalMemoryBytes > BigInt.zero
        ? used.toDouble() / sample.totalMemoryBytes.toDouble() * 100
        : 0.0;

    return Card(
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              '시스템 리소스',
              style: Theme.of(context).textTheme.titleMedium,
            ),
            const SizedBox(height: 12),
            Wrap(
              spacing: 20,
              runSpacing: 8,
              children: [
                _Metric(
                  label: '메모리',
                  value:
                      '사용 중 ${formatBytes(used)} / ${formatBytes(sample.totalMemoryBytes)}',
                ),
                _Metric(
                  label: '가용',
                  value: formatBytes(sample.sysAvailableBytes),
                ),
                _Metric(
                  label: 'CPU',
                  value: '${sample.cpuPct.toStringAsFixed(1)}%',
                ),
                if (sample.powerW != null)
                  _Metric(
                    label: '전력',
                    value: '${sample.powerW!.toStringAsFixed(1)} W',
                  ),
                if (sample.tempC != null)
                  _Metric(
                    label: '온도',
                    value: '${sample.tempC!.toStringAsFixed(0)} °C',
                  ),
              ],
            ),
            const SizedBox(height: 12),
            ClipRRect(
              borderRadius: BorderRadius.circular(6),
              child: LinearProgressIndicator(
                value: pct / 100,
                minHeight: 8,
                backgroundColor: AppTheme.border,
                color: AppTheme.primary,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _Metric extends StatelessWidget {
  final String label;
  final String value;

  const _Metric({required this.label, required this.value});

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(label, style: Theme.of(context).textTheme.labelSmall),
        const SizedBox(height: 4),
        Text(value, style: Theme.of(context).textTheme.titleSmall),
      ],
    );
  }
}

class _LeaderboardRow extends StatelessWidget {
  final int rank;
  final FrbOverviewRow row;
  final VoidCallback onTap;
  final VoidCallback onOpenHf;

  const _LeaderboardRow({
    required this.rank,
    required this.row,
    required this.onTap,
    required this.onOpenHf,
  });

  @override
  Widget build(BuildContext context) {
    return InkWell(
      onTap: onTap,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
        child: Row(
          children: [
            SizedBox(
              width: 28,
              child: Text(
                '#$rank',
                style: TextStyle(
                  fontWeight: FontWeight.bold,
                  color: AppTheme.inkMuted,
                ),
              ),
            ),
            Expanded(
              flex: 3,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    row.displayName,
                    style: Theme.of(context).textTheme.titleSmall,
                  ),
                  Text(
                    row.profileId,
                    style: Theme.of(context).textTheme.bodySmall?.copyWith(
                          color: AppTheme.inkMuted,
                        ),
                  ),
                ],
              ),
            ),
            if (TaskLabels.showBadge(row.modelType))
              Padding(
                padding: const EdgeInsets.only(right: 8),
                child: Chip(
                  label: Text(
                    TaskLabels.badge(row.modelType)!,
                    style: const TextStyle(fontSize: 10),
                  ),
                  visualDensity: VisualDensity.compact,
                  padding: EdgeInsets.zero,
                ),
              ),
            if (TaskLabels.usesTpsTier(row.modelType))
              Column(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  TierBadge(
                    decodeTps: row.decodeTps,
                    tier: row.tier,
                    generationKind: row.generationKind,
                    compact: true,
                  ),
                  MetricHint(term: MetricHelp.tpsTerm(row.generationKind)),
                ],
              )
            else if (row.ttftMs != null)
              Column(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  Text(
                    '${row.ttftMs!.toStringAsFixed(0)} ms',
                    style: Theme.of(context).textTheme.titleSmall,
                  ),
                  const MetricHint(term: '처리시간'),
                ],
              ),
            const SizedBox(width: 12),
            if (row.hfUrl != null)
              IconButton(
                tooltip: 'Hugging Face 열기',
                icon: const Icon(Icons.open_in_new, size: 18),
                onPressed: onOpenHf,
              ),
          ],
        ),
      ),
    );
  }
}

class _RecentRunRow extends StatelessWidget {
  final FrbRunListRow run;

  const _RecentRunRow({required this.run});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
      child: Row(
        children: [
          Expanded(
            child: Text(run.displayName, overflow: TextOverflow.ellipsis),
          ),
          Text(
            run.contextSize != null
                ? '${formatContext(platformIntToInt(run.contextSize))} 컨텍스트'
                : '',
            style: const TextStyle(color: AppTheme.inkMuted, fontSize: 12),
          ),
          const SizedBox(width: 12),
          TierBadge(
            decodeTps: run.decodeTps,
            tier: run.tier,
            generationKind: run.generationKind,
            compact: true,
          ),
          const SizedBox(width: 8),
          Text(
            run.status,
            style: TextStyle(
              fontSize: 12,
              color: run.status == 'completed'
                  ? AppTheme.success
                  : AppTheme.warning,
            ),
          ),
        ],
      ),
    );
  }
}