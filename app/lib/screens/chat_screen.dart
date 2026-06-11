import 'dart:async';

import 'package:app/services/aidash_api.dart';
import 'package:app/task_labels.dart';
import 'package:app/theme/app_theme.dart';
import 'package:app/src/rust/api.dart';
import 'package:file_picker/file_picker.dart';
import 'package:fl_chart/fl_chart.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

class ChatScreen extends StatefulWidget {
  const ChatScreen({super.key});

  @override
  State<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends State<ChatScreen> {
  List<FrbProfileRow> _profiles = [];
  String? _profileId;
  int _ctx = 4096;
  final _controller = TextEditingController();
  final _messages = <_ChatBubble>[];
  final List<FrbResourceSample> _samples = [];
  String? _imagePath;
  bool _serving = false;
  bool _streaming = false;
  StreamSubscription<FrbResourceSample>? _resourceSub;
  StreamSubscription<String>? _chatSub;

  @override
  void initState() {
    super.initState();
    _loadProfiles();
  }

  @override
  void dispose() {
    _controller.dispose();
    _resourceSub?.cancel();
    _chatSub?.cancel();
    super.dispose();
  }

  void _loadProfiles() {
    final api = context.read<AidashApi>();
    final profiles = api.listProfiles();
    setState(() {
      _profiles = profiles;
      if (profiles.isNotEmpty) {
        _profileId = profiles.first.id;
        _ctx = profiles.first.contextDefault;
      }
    });
  }

  Future<void> _ensureServe() async {
    if (_serving || _profileId == null) return;
    final api = context.read<AidashApi>();
    await api.serveStart(profileId: _profileId!, ctx: _ctx);
    _resourceSub?.cancel();
    _resourceSub = api.systemResources().listen((s) {
      if (mounted) {
        setState(() {
          _samples.add(s);
          if (_samples.length > 60) _samples.removeAt(0);
        });
      }
    });
    setState(() => _serving = true);
  }

  Future<void> _pickImage() async {
    final result = await FilePicker.pickFiles(
      type: FileType.image,
      allowMultiple: false,
    );
    if (result != null && result.files.single.path != null) {
      setState(() => _imagePath = result.files.single.path);
    }
  }

  Future<void> _send() async {
    final text = _controller.text.trim();
    if (text.isEmpty || _streaming) return;

    final api = context.read<AidashApi>();
    await _ensureServe();

    setState(() {
      _messages.add(_ChatBubble(role: 'user', content: text));
      _messages.add(_ChatBubble(role: 'assistant', content: '', streaming: true));
      _controller.clear();
      _streaming = true;
    });

    final history = _messages
        .where((m) => !m.streaming)
        .map((m) => FrbChatMessage(role: m.role, content: m.content))
        .toList();

    _chatSub?.cancel();
    final buffer = StringBuffer();
    _chatSub = api
        .chatSend(messages: history, imagePath: _imagePath)
        .listen(
      (token) {
        buffer.write(token);
        if (!mounted) return;
        setState(() {
          _messages.last = _ChatBubble(
            role: 'assistant',
            content: buffer.toString(),
            streaming: true,
          );
        });
      },
      onDone: () {
        if (!mounted) return;
        setState(() {
          _messages.last = _ChatBubble(
            role: 'assistant',
            content: buffer.toString(),
          );
          _streaming = false;
        });
      },
      onError: (e) {
        if (!mounted) return;
        setState(() {
          _messages.last = _ChatBubble(
            role: 'assistant',
            content: '오류: $e',
          );
          _streaming = false;
        });
      },
    );
  }

  Future<void> _stopServe() async {
    await context.read<AidashApi>().serveStop();
    _resourceSub?.cancel();
    setState(() => _serving = false);
  }

  FrbProfileRow? get _profile {
    if (_profileId == null) return null;
    return _profiles.cast<FrbProfileRow?>().firstWhere(
          (p) => p?.id == _profileId,
          orElse: () => null,
        );
  }

  bool get _chatEnabled {
    final p = _profile;
    if (p == null) return false;
    return TaskLabels.isChatCapable(p.modelType);
  }

  @override
  Widget build(BuildContext context) {
    final chatEnabled = _chatEnabled;
    final profile = _profile;

    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(24, 24, 24, 8),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  '채팅',
                  style: Theme.of(context).textTheme.headlineSmall?.copyWith(
                        fontWeight: FontWeight.bold,
                      ),
                ),
              ),
              if (_profiles.isNotEmpty)
                DropdownMenu<String>(
                  initialSelection: _profileId,
                  dropdownMenuEntries: _profiles
                      .map((p) => DropdownMenuEntry(value: p.id, label: p.id))
                      .toList(),
                  onSelected: (v) {
                    if (v == null) return;
                    final p = _profiles.firstWhere((x) => x.id == v);
                    setState(() {
                      _profileId = v;
                      _ctx = p.contextDefault;
                      _serving = false;
                    });
                  },
                ),
              const SizedBox(width: 8),
              if (profile != null && profile.modelType == 'multimodal')
                IconButton(
                  tooltip: '이미지 첨부',
                  onPressed: chatEnabled ? _pickImage : null,
                  icon: Badge(
                    isLabelVisible: _imagePath != null,
                    child: const Icon(Icons.attach_file),
                  ),
                ),
              IconButton(
                tooltip: _serving ? '서버 중지' : '서버 시작',
                onPressed: _serving ? _stopServe : _ensureServe,
                icon: Icon(_serving ? Icons.stop_circle_outlined : Icons.play_circle_outline),
              ),
            ],
          ),
        ),
        if (_imagePath != null)
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 24),
            child: Align(
              alignment: Alignment.centerLeft,
              child: Chip(
                label: Text(_imagePath!.split('/').last),
                onDeleted: () => setState(() => _imagePath = null),
              ),
            ),
          ),
        if (_samples.isNotEmpty)
          SizedBox(
            height: 80,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 24),
              child: Card(
                child: Padding(
                  padding: const EdgeInsets.all(8),
                  child: _LiveMetricChart(samples: _samples),
                ),
              ),
            ),
          ),
        Expanded(
          child: ListView.builder(
            padding: const EdgeInsets.all(24),
            itemCount: _messages.length,
            itemBuilder: (context, i) {
              final m = _messages[i];
              final isUser = m.role == 'user';
              return Align(
                alignment: isUser ? Alignment.centerRight : Alignment.centerLeft,
                child: Container(
                  margin: const EdgeInsets.only(bottom: 10),
                  padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
                  constraints: const BoxConstraints(maxWidth: 520),
                  decoration: BoxDecoration(
                    color: isUser
                        ? AppTheme.primary.withValues(alpha: 0.15)
                        : AppTheme.surface,
                    borderRadius: BorderRadius.circular(12),
                  ),
                  child: Text(
                    m.content.isEmpty && m.streaming ? '▌' : m.content,
                    style: const TextStyle(height: 1.4),
                  ),
                ),
              );
            },
          ),
        ),
        if (!chatEnabled && profile != null)
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 24),
            child: Card(
              color: AppTheme.warning.withValues(alpha: 0.08),
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Text(
                  '이 모델은 채팅형이 아닙니다 — \'벤치\' 탭에서 테스트하세요 '
                  '(${TaskLabels.label(profile.modelType)})',
                  style: Theme.of(context).textTheme.bodyMedium,
                ),
              ),
            ),
          ),
        Padding(
          padding: const EdgeInsets.fromLTRB(24, 0, 24, 24),
          child: Row(
            children: [
              Expanded(
                child: TextField(
                  controller: _controller,
                  enabled: chatEnabled,
                  decoration: InputDecoration(
                    hintText: chatEnabled
                        ? '메시지를 입력하세요…'
                        : '채팅 불가 모델',
                  ),
                  onSubmitted: chatEnabled ? (_) => _send() : null,
                ),
              ),
              const SizedBox(width: 12),
              FilledButton(
                onPressed: chatEnabled && !_streaming ? _send : null,
                child: const Text('전송'),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _ChatBubble {
  final String role;
  final String content;
  final bool streaming;

  _ChatBubble({
    required this.role,
    required this.content,
    this.streaming = false,
  });
}

class _LiveMetricChart extends StatelessWidget {
  final List<FrbResourceSample> samples;

  const _LiveMetricChart({required this.samples});

  @override
  Widget build(BuildContext context) {
    final ram = samples.asMap().entries.map((e) {
      return FlSpot(
        e.key.toDouble(),
        e.value.physFootprintBytes.toDouble() / (1024 * 1024 * 1024),
      );
    }).toList();
    final maxY = ram.map((s) => s.y).reduce((a, b) => a > b ? a : b) * 1.1;

    return LineChart(
      LineChartData(
        minY: 0,
        maxY: maxY,
        gridData: const FlGridData(show: false),
        titlesData: const FlTitlesData(show: false),
        lineTouchData: const LineTouchData(enabled: false),
        lineBarsData: [
          LineChartBarData(
            spots: ram,
            color: AppTheme.primary,
            barWidth: 2,
            dotData: const FlDotData(show: false),
          ),
        ],
      ),
    );
  }
}