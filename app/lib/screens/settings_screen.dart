import 'dart:async';

import 'package:app/services/aidash_api.dart';
import 'package:app/services/config_service.dart';
import 'package:app/src/rust/api.dart';
import 'package:app/theme/app_theme.dart';
import 'package:app/widgets/doctor_badge.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

class SettingsScreen extends StatefulWidget {
  const SettingsScreen({super.key});

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  FrbDoctorReport? _report;
  FrbAuthStatus? _auth;
  final _tokenController = TextEditingController();
  final _rootController = TextEditingController();
  String? _verifyUser;
  String? _verifyError;
  bool _verifying = false;
  final Map<String, List<String>> _fixLogs = {};
  final Set<String> _fixing = {};

  @override
  void initState() {
    super.initState();
    _rootController.text = context.read<ConfigService>().projectRoot;
    _load();
  }

  @override
  void dispose() {
    _tokenController.dispose();
    _rootController.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    final api = context.read<AidashApi>();
    final report = await api.doctorReport();
    final auth = await api.authStatus();
    if (!mounted) return;
    setState(() {
      _report = report;
      _auth = auth;
    });
  }

  Future<void> _verifyToken() async {
    final token = _tokenController.text.trim();
    if (token.isEmpty) return;
    setState(() {
      _verifying = true;
      _verifyUser = null;
      _verifyError = null;
    });
    try {
      final user = await context.read<AidashApi>().authVerifyToken(token: token);
      if (!mounted) return;
      setState(() {
        _verifyUser = user;
        _verifying = false;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _verifyError = e.toString();
        _verifying = false;
      });
    }
  }

  Future<void> _saveToken() async {
    final token = _tokenController.text.trim();
    if (token.isEmpty) return;
    await context.read<AidashApi>().authSet(token: token);
    _tokenController.clear();
    await _load();
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(
      const SnackBar(content: Text('토큰이 Keychain에 저장되었습니다.')),
    );
  }

  Future<void> _importToken() async {
    try {
      final user = await context.read<AidashApi>().authImport();
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('토큰 가져오기 완료: $user')),
      );
      await _load();
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('가져오기 실패: $e')),
      );
    }
  }

  Future<void> _clearToken() async {
    context.read<AidashApi>().authClear();
    await _load();
  }

  Future<void> _saveProjectRoot() async {
    final path = _rootController.text.trim();
    if (path.isEmpty) return;
    try {
      context.read<AidashApi>().setProjectRoot(path: path);
      await context.read<ConfigService>().setProjectRoot(path);
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('프로젝트 루트가 변경되었습니다.')),
      );
      await _load();
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('변경 실패: $e')),
      );
    }
  }

  Future<void> _runFix(FrbDoctorItem item) async {
    final cmd = item.fixAction;
    if (cmd == null) return;
    final api = context.read<AidashApi>();
    setState(() => _fixing.add(item.name));
    _fixLogs[item.name] = [];
    try {
      await for (final p in api.runFixAction(command: cmd)) {
        if (!mounted) return;
        setState(() => _fixLogs[item.name]!.add(p.line));
        if (p.done) await _load();
      }
    } finally {
      if (mounted) setState(() => _fixing.remove(item.name));
    }
  }

  @override
  Widget build(BuildContext context) {
    final api = context.read<AidashApi>();
    final device = api.deviceLabel();

    return ListView(
      padding: const EdgeInsets.all(24),
      children: [
        Text(
          '설정',
          style: Theme.of(context).textTheme.headlineSmall?.copyWith(
                fontWeight: FontWeight.bold,
              ),
        ),
        const SizedBox(height: 8),
        Text(
          device,
          style: Theme.of(context).textTheme.bodySmall?.copyWith(
                color: AppTheme.inkMuted,
              ),
        ),
        const SizedBox(height: 20),
        Text('프로젝트 루트', style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: 8),
        Card(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Row(
              children: [
                Expanded(
                  child: TextField(
                    controller: _rootController,
                    decoration: const InputDecoration(
                      hintText: '/path/to/AI_Dashboard',
                    ),
                  ),
                ),
                const SizedBox(width: 12),
                FilledButton(
                  onPressed: _saveProjectRoot,
                  child: const Text('저장'),
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 20),
        Text('Hugging Face 토큰', style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: 8),
        Card(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                if (_auth != null) ...[
                  Text(
                    _auth!.whoamiUser.isNotEmpty
                        ? '사용자: ${_auth!.whoamiUser}'
                        : '토큰 미등록',
                  ),
                  if (_auth!.maskedToken != null)
                    Text(
                      '토큰: ${_auth!.maskedToken}',
                      style: const TextStyle(color: AppTheme.inkMuted, fontSize: 12),
                    ),
                  const SizedBox(height: 12),
                  Wrap(
                    spacing: 8,
                    children: _auth!.sources
                        .map(
                          (s) => Chip(
                            avatar: Icon(
                              s.present ? Icons.check_circle : Icons.circle_outlined,
                              size: 16,
                            ),
                            label: Text(s.label),
                          ),
                        )
                        .toList(),
                  ),
                  const SizedBox(height: 16),
                ],
                if (_auth?.canImport == true)
                  FilledButton.tonal(
                    onPressed: _importToken,
                    child: const Text('기존 토큰 가져오기'),
                  ),
                const SizedBox(height: 12),
                TextField(
                  controller: _tokenController,
                  obscureText: true,
                  decoration: const InputDecoration(
                    hintText: 'hf_… 토큰 붙여넣기',
                  ),
                  onChanged: (_) {
                    setState(() {
                      _verifyUser = null;
                      _verifyError = null;
                    });
                  },
                ),
                const SizedBox(height: 8),
                Row(
                  children: [
                    OutlinedButton(
                      onPressed: _verifying ? null : _verifyToken,
                      child: _verifying
                          ? const SizedBox(
                              width: 18,
                              height: 18,
                              child: CircularProgressIndicator(strokeWidth: 2),
                            )
                          : const Text('검증'),
                    ),
                    const SizedBox(width: 12),
                    FilledButton(
                      onPressed: _verifyUser != null ? _saveToken : null,
                      child: const Text('Keychain에 저장'),
                    ),
                    const Spacer(),
                    TextButton(
                      onPressed: _clearToken,
                      child: const Text('토큰 삭제'),
                    ),
                  ],
                ),
                if (_verifyUser != null)
                  Padding(
                    padding: const EdgeInsets.only(top: 8),
                    child: Text('✅ 검증됨: $_verifyUser'),
                  ),
                if (_verifyError != null)
                  Padding(
                    padding: const EdgeInsets.only(top: 8),
                    child: Text(
                      '❌ $_verifyError',
                      style: const TextStyle(color: AppTheme.error),
                    ),
                  ),
                const SizedBox(height: 8),
                Text(
                  '토큰 없이도 공개 모델만 사용할 수 있습니다.',
                  style: Theme.of(context).textTheme.bodySmall?.copyWith(
                        color: AppTheme.inkMuted,
                      ),
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 20),
        Text('환경 점검 (doctor)', style: Theme.of(context).textTheme.titleMedium),
        const SizedBox(height: 8),
        if (_report == null)
          const Center(child: CircularProgressIndicator())
        else
          Card(
            child: Column(
              children: _report!.items.map((item) {
                final logs = _fixLogs[item.name] ?? [];
                return ListTile(
                  leading: DoctorBadge(status: item.status, showLabel: false),
                  title: Text('${item.category} · ${item.name}'),
                  subtitle: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(item.detail),
                      if (logs.isNotEmpty)
                        Text(
                          logs.last,
                          style: const TextStyle(
                            fontFamily: 'monospace',
                            fontSize: 10,
                          ),
                        ),
                    ],
                  ),
                  trailing: item.fixAction != null
                      ? FilledButton.tonal(
                          onPressed: _fixing.contains(item.name)
                              ? null
                              : () => _runFix(item),
                          child: _fixing.contains(item.name)
                              ? const SizedBox(
                                  width: 16,
                                  height: 16,
                                  child: CircularProgressIndicator(strokeWidth: 2),
                                )
                              : const Text('고치기'),
                        )
                      : DoctorBadge(status: item.status),
                );
              }).toList(),
            ),
          ),
      ],
    );
  }
}