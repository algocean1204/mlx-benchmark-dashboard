import 'dart:async';

import 'package:app/services/aidash_api.dart';
import 'package:app/src/rust/api.dart';
import 'package:app/task_labels.dart';
import 'package:app/theme/app_theme.dart';
import 'package:app/utils/formatters.dart';
import 'package:app/widgets/error_card.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

class ModelManageScreen extends StatefulWidget {
  const ModelManageScreen({super.key});

  @override
  State<ModelManageScreen> createState() => _ModelManageScreenState();
}

class _ModelManageScreenState extends State<ModelManageScreen> {
  FrbDiskUsage? _disk;
  List<FrbCacheRepoEntry> _repos = [];
  Set<String> _drafterRepoIds = {};
  bool _loading = true;
  String? _error;
  String _searchQuery = '';
  List<FrbHfSearchResult> _searchResults = [];
  bool _searching = false;
  String? _searchError;
  String? _selectedRepo;
  int? _selectedSize;
  bool _downloading = false;
  double? _downloadPercent;
  String? _downloadLine;
  String? _downloadError;
  String? _lastDownloadedRepo;
  StreamSubscription<FrbDownloadProgress>? _downloadSub;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void dispose() {
    _downloadSub?.cancel();
    super.dispose();
  }

  Future<void> _load() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    final api = context.read<AidashApi>();
    try {
      final disk = await api.diskUsage();
      final scan = await api.cacheScan();
      final drafters = api
          .listProfiles()
          .where((p) => p.isDrafter)
          .map((p) => p.id)
          .toSet();
      if (!mounted) return;
      setState(() {
        _disk = disk;
        _repos = scan.repos;
        _drafterRepoIds = drafters;
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

  Future<void> _search() async {
    if (_searchQuery.trim().isEmpty) return;
    setState(() {
      _searching = true;
      _searchError = null;
      _searchResults = [];
      _selectedRepo = null;
      _selectedSize = null;
    });
    final api = context.read<AidashApi>();
    try {
      final results = await api.hfSearch(query: _searchQuery.trim());
      if (!mounted) return;
      setState(() {
        _searchResults = results;
        _searching = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _searching = false;
        _searchError = e.toString();
      });
    }
  }

  Future<void> _selectForInstall(FrbHfSearchResult result) async {
    final api = context.read<AidashApi>();
    try {
      final size = await api.hfModelSize(repoId: result.repoId);
      if (!mounted) return;
      setState(() {
        _selectedRepo = result.repoId;
        _selectedSize = size;
      });
    } catch (e) {
      if (!mounted) return;
      final msg = e.toString();
      if (msg.contains('gated') || msg.contains('401') || msg.contains('unauthorized')) {
        setState(() => _searchError = '토큰 필요 또는 권한 없음 — 설정 탭에서 HF 토큰을 등록하세요.');
      } else {
        setState(() => _searchError = msg);
      }
    }
  }

  Future<void> _startDownload() async {
    final repo = _selectedRepo;
    if (repo == null) return;
    final api = context.read<AidashApi>();
    setState(() {
      _downloading = true;
      _downloadError = null;
      _downloadPercent = 0;
      _lastDownloadedRepo = null;
    });
    _downloadSub?.cancel();
    _downloadSub = api.hfDownloadStart(repoId: repo).listen(
      (p) {
        if (!mounted) return;
        setState(() {
          _downloadLine = p.line;
          _downloadPercent = p.percent;
          if (p.done) {
            _downloading = false;
            if (p.success) {
              _lastDownloadedRepo = repo;
            } else {
              _downloadError = p.line;
            }
          }
        });
        if (p.done && p.success) {
          _load();
          _autoGenerateProfile(repo);
        }
      },
      onError: (e) {
        if (!mounted) return;
        setState(() {
          _downloading = false;
          _downloadError = e.toString();
        });
      },
    );
  }

  void _cancelDownload() {
    context.read<AidashApi>().hfDownloadCancel();
    setState(() {
      _downloading = false;
      _downloadLine = '취소됨';
    });
  }

  String _fmtDate(String? raw) {
    if (raw == null) return '—';
    final dt = DateTime.tryParse(raw);
    if (dt == null) return raw;
    final l = dt.toLocal();
    String two(int v) => v.toString().padLeft(2, '0');
    return '${l.year}-${two(l.month)}-${two(l.day)}';
  }

  Future<void> _confirmDelete(FrbCacheRepoEntry repo) async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('모델 캐시 삭제'),
        content: Text(
          '${repo.repoId}\n'
          '크기: ${formatBytesInt(repo.sizeBytes.toInt())}\n\n'
          'HF 캐시에서 이 모델을 삭제합니다.\n'
          '측정 기록(DB)은 보존됩니다.',
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
    try {
      await api.cacheDelete(repoId: repo.repoId);
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('${repo.repoId} 캐시 삭제 완료')),
      );
      _load();
    } catch (e) {
      if (!mounted) return;
      ErrorCard.showSnackBar(context, e.toString());
    }
  }

  Future<void> _autoGenerateProfile(String repoId) async {
    final api = context.read<AidashApi>();
    if (api.listProfiles().any((p) => p.id == repoId)) {
      return;
    }
    await _generateProfile(repoId, auto: true);
  }

  Future<void> _generateProfile(String repoId, {bool auto = false}) async {
    final api = context.read<AidashApi>();
    try {
      api.profileGenerate(repoId: repoId);
      if (!mounted) return;
      final profile = api.listProfiles().cast<FrbProfileRow?>().firstWhere(
            (p) => p?.id == repoId,
            orElse: () => null,
          );
      if (auto) {
        final maxLabel =
            profile != null ? formatContext(profile.contextMax) : '—';
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('프로파일 자동 생성됨 — 최대 컨텍스트 $maxLabel 감지'),
          ),
        );
      } else {
        final path = profile?.filename ?? repoId;
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('프로파일 생성됨: $path')),
        );
      }
    } catch (e) {
      if (!mounted) return;
      final msg = e.toString();
      if (auto && msg.contains('already exists')) {
        return;
      }
      ErrorCard.showSnackBar(
        context,
        auto ? '프로파일 자동 생성 실패: $msg' : msg,
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_loading) {
      return const Center(child: CircularProgressIndicator());
    }

    return ListView(
      padding: const EdgeInsets.all(24),
      children: [
        Text(
          '모델 관리',
          style: Theme.of(context).textTheme.headlineSmall?.copyWith(
                fontWeight: FontWeight.bold,
              ),
        ),
        const SizedBox(height: 16),
        if (_error != null) ErrorCard(message: _error!, onRetry: _load),
        if (_disk != null) _DiskCard(disk: _disk!),
        const SizedBox(height: 20),
        Text('설치된 모델', style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: 8),
        Card(
          child: _repos.isEmpty
              ? const Padding(
                  padding: EdgeInsets.all(24),
                  child: Text('캐시된 모델이 없습니다.'),
                )
              : Column(
                  children: _repos
                      .map(
                        (r) => ListTile(
                          title: Text(r.repoId),
                          subtitle: Text(
                            '${formatBytesInt(r.sizeBytes.toInt())} · ${_fmtDate(r.lastModified)}'
                            '${_drafterRepoIds.contains(r.repoId) ? ' · 보조(drafter) 모델' : ''}',
                          ),
                          trailing: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              if (_drafterRepoIds.contains(r.repoId))
                                const Chip(
                                  label: Text('drafter'),
                                  visualDensity: VisualDensity.compact,
                                ),
                              if (r.hasProfile)
                                const Chip(
                                  label: Text('프로파일'),
                                  visualDensity: VisualDensity.compact,
                                ),
                              IconButton(
                                icon: const Icon(Icons.delete_outline),
                                tooltip: '캐시 삭제',
                                onPressed: () => _confirmDelete(r),
                              ),
                            ],
                          ),
                        ),
                      )
                      .toList(),
                ),
        ),
        const SizedBox(height: 20),
        Text('모델 검색 · 설치', style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: 8),
        Row(
          children: [
            Expanded(
              child: TextField(
                decoration: const InputDecoration(
                  hintText: 'Hugging Face 모델 검색',
                  prefixIcon: Icon(Icons.search),
                ),
                onChanged: (v) => _searchQuery = v,
                onSubmitted: (_) => _search(),
              ),
            ),
            const SizedBox(width: 8),
            FilledButton(
              onPressed: _searching ? null : _search,
              child: _searching
                  ? const SizedBox(
                      width: 20,
                      height: 20,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Text('검색'),
            ),
          ],
        ),
        if (_searchError != null) ...[
          const SizedBox(height: 8),
          ErrorCard(message: _searchError!),
        ],
        if (_searchResults.isNotEmpty) ...[
          const SizedBox(height: 12),
          Card(
            child: Column(
              children: _searchResults
                  .map(
                    (r) => ListTile(
                      title: Text(r.repoId),
                      subtitle: Text(
                        '⬇ ${r.downloads} · ♥ ${r.likes}'
                        '${r.pipelineTag != null ? ' · ${TaskLabels.pipelineTagLabel(r.pipelineTag)}' : ''}',
                      ),
                      trailing: r.installed
                          ? const Chip(label: Text('설치됨'))
                          : null,
                      selected: _selectedRepo == r.repoId,
                      onTap: () => _selectForInstall(r),
                    ),
                  )
                  .toList(),
            ),
          ),
        ],
        if (_selectedRepo != null && _selectedSize != null) ...[
          const SizedBox(height: 12),
          Card(
            child: Padding(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text('선택: $_selectedRepo'),
                  Text(
                    '예상 크기: ${formatBytesInt(_selectedSize!)}',
                    style: Theme.of(context).textTheme.bodySmall,
                  ),
                  const SizedBox(height: 12),
                  if (_downloading) ...[
                    LinearProgressIndicator(
                      value: (_downloadPercent ?? 0) / 100,
                    ),
                    const SizedBox(height: 8),
                    Text(_downloadLine ?? '', style: Theme.of(context).textTheme.bodySmall),
                    const SizedBox(height: 8),
                    OutlinedButton(
                      onPressed: _cancelDownload,
                      child: const Text('취소'),
                    ),
                  ] else
                    FilledButton(
                      onPressed: _startDownload,
                      child: const Text('설치'),
                    ),
                  if (_downloadError != null) ...[
                    const SizedBox(height: 8),
                    Text(
                      _downloadError!,
                      style: const TextStyle(color: AppTheme.error),
                    ),
                  ],
                  if (_lastDownloadedRepo != null) ...[
                    const SizedBox(height: 12),
                    FilledButton.tonal(
                      onPressed: () => _generateProfile(_lastDownloadedRepo!),
                      child: const Text('프로파일 생성'),
                    ),
                  ],
                ],
              ),
            ),
          ),
        ],
      ],
    );
  }
}

class _DiskCard extends StatelessWidget {
  final FrbDiskUsage disk;

  const _DiskCard({required this.disk});

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('디스크 · 캐시 현황', style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 12),
            Text(
              'SSD: ${formatBytesInt(disk.availableBytes.toInt())} 여유 / '
              '${formatBytesInt(disk.totalBytes.toInt())} 총',
            ),
            Text(
              'HF 캐시: ${formatBytesInt(disk.cacheTotalBytes.toInt())}',
            ),
            Text(
              '경로: ${disk.cacheDir}',
              style: Theme.of(context).textTheme.bodySmall?.copyWith(
                    color: AppTheme.inkMuted,
                  ),
            ),
          ],
        ),
      ),
    );
  }
}