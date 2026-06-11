import 'package:app/services/aidash_api.dart';
import 'package:app/services/config_service.dart';
import 'package:app/src/rust/api.dart';
import 'package:app/theme/app_theme.dart';
import 'package:app/widgets/doctor_badge.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

const _bootstrapFixAction = '자동 설정 실행';

class OnboardingScreen extends StatefulWidget {
  final VoidCallback onComplete;

  const OnboardingScreen({super.key, required this.onComplete});

  @override
  State<OnboardingScreen> createState() => _OnboardingScreenState();
}

class _OnboardingScreenState extends State<OnboardingScreen> {
  FrbDoctorReport? _report;
  bool _loading = true;
  String? _error;
  final Map<String, List<String>> _fixLogs = {};
  final Set<String> _fixing = {};

  @override
  void initState() {
    super.initState();
    _runDoctor();
  }

  Future<void> _runDoctor() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      final api = context.read<AidashApi>();
      final report = await api.doctorReport();
      if (!mounted) return;
      setState(() {
        _report = report;
        _loading = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _error = e.toString();
        _loading = false;
      });
    }
  }

  int _countByStatus(String status) =>
      _report?.items.where((i) => i.status == status).length ?? 0;

  Future<void> _runBootstrap() async {
    setState(() => _fixing.add('bootstrap'));
    _fixLogs['bootstrap'] = [];
    final api = context.read<AidashApi>();
    try {
      await for (final ev in api.envBootstrap()) {
        if (!mounted) return;
        setState(() => _fixLogs['bootstrap']!.add(ev.message));
      }
      await _runDoctor();
    } catch (e) {
      if (!mounted) return;
      setState(() => _fixLogs['bootstrap']!.add('오류: $e'));
    } finally {
      if (mounted) setState(() => _fixing.remove('bootstrap'));
    }
  }

  Future<void> _runFix(FrbDoctorItem item) async {
    final cmd = item.fixAction;
    if (cmd == null || cmd.isEmpty) return;
    if (cmd == _bootstrapFixAction) {
      await _runBootstrap();
      return;
    }
    setState(() => _fixing.add(item.name));
    final api = context.read<AidashApi>();
    _fixLogs[item.name] = [];
    try {
      await for (final progress in api.runFixAction(command: cmd)) {
        if (!mounted) return;
        setState(() {
          _fixLogs[item.name]!.add(progress.line);
        });
        if (progress.done) {
          await _runDoctor();
        }
      }
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _fixLogs[item.name]!.add('오류: $e');
      });
    } finally {
      if (mounted) setState(() => _fixing.remove(item.name));
    }
  }

  Future<void> _finish() async {
    final config = context.read<ConfigService>();
    await config.setOnboardingComplete(true);
    widget.onComplete();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final issues = _report?.items
            .where((i) => i.status == 'warn' || i.status == 'missing')
            .toList() ??
        [];

    return Scaffold(
      body: SafeArea(
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 720),
            child: Padding(
              padding: const EdgeInsets.all(28),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Text(
                    'AI Dashboard 환경 점검',
                    style: theme.textTheme.headlineMedium?.copyWith(
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                  const SizedBox(height: 8),
                  Text(
                    'doctor가 시스템·백엔드·모델·토큰을 자동 감지합니다.',
                    style: theme.textTheme.bodyMedium?.copyWith(
                      color: AppTheme.inkMuted,
                    ),
                  ),
                  const SizedBox(height: 24),
                  if (_loading)
                    const Expanded(
                      child: Center(child: CircularProgressIndicator()),
                    )
                  else if (_error != null)
                    Expanded(
                      child: Center(
                        child: Column(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Text('점검 실패: $_error'),
                            const SizedBox(height: 12),
                            FilledButton(
                              onPressed: _runDoctor,
                              child: const Text('다시 시도'),
                            ),
                          ],
                        ),
                      ),
                    )
                  else ...[
                    _SummaryCards(
                      ok: _countByStatus('ok'),
                      warn: _countByStatus('warn'),
                      missing: _countByStatus('missing'),
                      total: _report!.items.length,
                    ),
                    const SizedBox(height: 20),
                    Expanded(
                      child: ListView(
                        children: [
                          if (issues.isEmpty)
                            Card(
                              child: Padding(
                                padding: const EdgeInsets.all(20),
                                child: Row(
                                  children: [
                                    const Text('✅', style: TextStyle(fontSize: 28)),
                                    const SizedBox(width: 16),
                                    Expanded(
                                      child: Text(
                                        '모든 항목이 준비되었습니다. 바로 시작할 수 있습니다.',
                                        style: theme.textTheme.titleMedium,
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                            )
                          else
                            ...issues.map((item) => _FixStepCard(
                                  item: item,
                                  logs: _fixLogs[item.name] ?? const [],
                                  fixing: _fixing.contains(item.name),
                                  onFix: () => _runFix(item),
                                )),
                        ],
                      ),
                    ),
                    const SizedBox(height: 16),
                    Row(
                      children: [
                        TextButton(
                          onPressed: _runDoctor,
                          child: const Text('다시 점검'),
                        ),
                        const Spacer(),
                        FilledButton(
                          onPressed: _finish,
                          child: Text(
                            issues.isEmpty ? '시작하기' : '토큰 없이 계속',
                          ),
                        ),
                      ],
                    ),
                  ],
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _SummaryCards extends StatelessWidget {
  final int ok;
  final int warn;
  final int missing;
  final int total;

  const _SummaryCards({
    required this.ok,
    required this.warn,
    required this.missing,
    required this.total,
  });

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        Expanded(child: _SummaryCard(title: '준비됨', count: ok, status: 'ok')),
        const SizedBox(width: 12),
        Expanded(
          child: _SummaryCard(title: '조치 필요', count: warn, status: 'warn'),
        ),
        const SizedBox(width: 12),
        Expanded(
          child: _SummaryCard(title: '설치 가능', count: missing, status: 'missing'),
        ),
        const SizedBox(width: 12),
        Expanded(
          child: Card(
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text('전체', style: Theme.of(context).textTheme.labelLarge),
                  const SizedBox(height: 8),
                  Text(
                    '$total 항목',
                    style: Theme.of(context).textTheme.headlineSmall,
                  ),
                ],
              ),
            ),
          ),
        ),
      ],
    );
  }
}

class _SummaryCard extends StatelessWidget {
  final String title;
  final int count;
  final String status;

  const _SummaryCard({
    required this.title,
    required this.count,
    required this.status,
  });

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                DoctorBadge(status: status, showLabel: false),
                const SizedBox(width: 8),
                Text(title, style: Theme.of(context).textTheme.labelLarge),
              ],
            ),
            const SizedBox(height: 8),
            Text(
              '$count',
              style: Theme.of(context).textTheme.headlineSmall,
            ),
          ],
        ),
      ),
    );
  }
}

class _FixStepCard extends StatelessWidget {
  final FrbDoctorItem item;
  final List<String> logs;
  final bool fixing;
  final VoidCallback onFix;

  const _FixStepCard({
    required this.item,
    required this.logs,
    required this.fixing,
    required this.onFix,
  });

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                DoctorBadge(status: item.status),
                const SizedBox(width: 12),
                Expanded(
                  child: Text(
                    '${item.category} · ${item.name}',
                    style: Theme.of(context).textTheme.titleSmall,
                  ),
                ),
                if (item.fixAction != null)
                  FilledButton.tonal(
                    onPressed: fixing ? null : onFix,
                    child: fixing
                        ? const SizedBox(
                            width: 18,
                            height: 18,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : const Text('고치기'),
                  ),
              ],
            ),
            const SizedBox(height: 8),
            Text(
              item.detail,
              style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                    color: AppTheme.inkMuted,
                  ),
            ),
            if (logs.isNotEmpty) ...[
              const SizedBox(height: 8),
              ExpansionTile(
                title: const Text('실행 로그', style: TextStyle(fontSize: 13)),
                children: logs
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
            ],
          ],
        ),
      ),
    );
  }
}