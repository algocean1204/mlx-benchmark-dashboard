import 'dart:async';

import 'package:app/services/aidash_api.dart';
import 'package:app/task_labels.dart';
import 'package:app/theme/app_theme.dart';
import 'package:app/src/rust/api.dart';
import 'package:app/utils/formatters.dart';
import 'package:file_picker/file_picker.dart';
import 'package:fl_chart/fl_chart.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

const _compressKeepRecentTurns = 4;

class ChatScreen extends StatefulWidget {
  const ChatScreen({super.key});

  @override
  State<ChatScreen> createState() => _ChatScreenState();
}

class _ChatScreenState extends State<ChatScreen> {
  List<FrbProfileRow> _profiles = [];
  List<FrbChatSessionRow> _sessions = [];
  String? _profileId;
  int? _sessionId;
  int _ctx = 4096;
  int _promptTokens = 0;
  bool _panelOpen = true;
  bool _compressing = false;
  final _controller = TextEditingController();
  final _messages = <_ChatBubble>[];
  final List<FrbChatMessage> _sendHistory = [];
  final List<FrbResourceSample> _samples = [];
  String? _imagePath;
  bool _serving = false;
  bool _streaming = false;
  StreamSubscription<FrbResourceSample>? _resourceSub;
  StreamSubscription<FrbChatStreamEvent>? _chatSub;

  @override
  void initState() {
    super.initState();
    _loadProfiles();
    _loadSessions();
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
      if (profiles.isNotEmpty && _profileId == null) {
        _profileId = profiles.first.id;
        _ctx = profiles.first.contextDefault;
      }
    });
  }

  void _loadSessions() {
    final api = context.read<AidashApi>();
    setState(() => _sessions = api.chatListSessions());
  }

  List<int> get _ctxOptions {
    final p = _profile;
    if (p == null) return [_ctx];
    final steps = p.sweepSteps.map((s) => s.toInt()).toList();
    if (steps.isNotEmpty) return steps;
    final options = {p.contextMin, p.contextDefault, p.contextMax}.toList()
      ..sort();
    return options;
  }

  FrbProfileRow? get _profile {
    if (_profileId == null) return null;
    for (final p in _profiles) {
      if (p.id == _profileId) return p;
    }
    return null;
  }

  bool get _chatEnabled {
    final p = _profile;
    if (p == null) return false;
    return TaskLabels.isChatCapable(p.modelType);
  }

  double get _contextUsagePct =>
      _ctx > 0 ? (_promptTokens / _ctx * 100).clamp(0, 100) : 0;

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

  Future<void> _newSession() async {
    if (_profileId == null) return;
    final api = context.read<AidashApi>();
    final id = api.chatCreateSession(profileId: _profileId!, title: '새 대화');
    _loadSessions();
    setState(() {
      _sessionId = id;
      _messages.clear();
      _sendHistory.clear();
      _promptTokens = 0;
    });
  }

  Future<void> _selectSession(int id) async {
    final api = context.read<AidashApi>();
    final rows = api.chatLoadMessages(sessionId: id);
    final session = _sessions.firstWhere((s) => s.id.toInt() == id);
    setState(() {
      _sessionId = id;
      _profileId = session.profileId;
      _messages
        ..clear()
        ..addAll(
          rows.map(
            (r) => _ChatBubble(role: r.role, content: r.content),
          ),
        );
      _sendHistory
        ..clear()
        ..addAll(
          rows.map(
            (r) => FrbChatMessage(role: r.role, content: r.content),
          ),
        );
      _promptTokens = rows
          .map((r) => r.tokenCount?.toInt() ?? 0)
          .fold<int>(0, (a, b) => a + b);
    });
    final p = _profile;
    if (p != null) {
      setState(() => _ctx = p.contextDefault);
    }
  }

  Future<void> _deleteSession(int id) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: const Text('대화 삭제'),
        content: const Text('이 대화를 삭제할까요?'),
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
    if (confirmed != true || !mounted) return;
    context.read<AidashApi>().chatDeleteSession(sessionId: id);
    _loadSessions();
    if (_sessionId == id) {
      setState(() {
        _sessionId = null;
        _messages.clear();
        _sendHistory.clear();
        _promptTokens = 0;
      });
    }
  }

  Future<void> _maybeCompress(AidashApi api) async {
    if (!api.chatShouldCompress(
      promptTokens: _promptTokens,
      contextSize: _ctx,
    )) {
      return;
    }

    final keepMessages = _compressKeepRecentTurns * 2;
    if (_sendHistory.length <= keepMessages) return;

    setState(() => _compressing = true);
    try {
      final old = _sendHistory.sublist(0, _sendHistory.length - keepMessages);
      final recent =
          _sendHistory.sublist(_sendHistory.length - keepMessages);
      final summary = await api.chatSummarize(messages: old);
      if (!mounted) return;
      setState(() {
        _sendHistory
          ..clear()
          ..add(
            FrbChatMessage(
              role: 'assistant',
              content: '[이전 대화 요약]\n$summary',
            ),
          )
          ..addAll(recent);
        _messages.add(const _ChatBubble(compressionNotice: true));
        _compressing = false;
      });
    } catch (e) {
      if (mounted) setState(() => _compressing = false);
      rethrow;
    }
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
    if (text.isEmpty || _streaming || _compressing || _profileId == null) {
      return;
    }

    final api = context.read<AidashApi>();
    await _ensureServe();

    if (_sessionId == null) {
      final title = text.length > 30 ? text.substring(0, 30) : text;
      _sessionId = api.chatCreateSession(profileId: _profileId!, title: title);
      _loadSessions();
    }

    await _maybeCompress(api);

    setState(() {
      _messages.add(_ChatBubble(role: 'user', content: text));
      _messages.add(_ChatBubble(role: 'assistant', content: '', streaming: true));
      _controller.clear();
      _streaming = true;
    });

    api.chatAppendMessage(
      sessionId: _sessionId!,
      role: 'user',
      content: text,
    );

    final payload = [
      ..._sendHistory,
      FrbChatMessage(role: 'user', content: text),
    ];

    _chatSub?.cancel();
    final buffer = StringBuffer();
    _chatSub = api
        .chatSend(messages: payload, imagePath: _imagePath)
        .listen(
      (event) {
        if (event.isDone) {
          if (!mounted) return;
          setState(() {
            _messages.last = _ChatBubble(
              role: 'assistant',
              content: buffer.toString(),
            );
            _streaming = false;
            _promptTokens = event.promptTokens;
          });
          _sendHistory.add(FrbChatMessage(role: 'user', content: text));
          _sendHistory.add(
            FrbChatMessage(role: 'assistant', content: buffer.toString()),
          );
          api.chatAppendMessage(
            sessionId: _sessionId!,
            role: 'assistant',
            content: buffer.toString(),
            tokenCount: event.promptTokens,
          );
          return;
        }
        buffer.write(event.text);
        if (!mounted) return;
        setState(() {
          _messages.last = _ChatBubble(
            role: 'assistant',
            content: buffer.toString(),
            streaming: true,
          );
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

  String _sessionDate(String ts) {
    final ms = int.tryParse(ts);
    if (ms == null) return ts;
    final dt = DateTime.fromMillisecondsSinceEpoch(ms);
    return '${dt.month}/${dt.day} ${dt.hour.toString().padLeft(2, '0')}:${dt.minute.toString().padLeft(2, '0')}';
  }

  @override
  Widget build(BuildContext context) {
    final chatEnabled = _chatEnabled;
    final profile = _profile;

    return Row(
      children: [
        AnimatedContainer(
          duration: const Duration(milliseconds: 200),
          width: _panelOpen ? 240 : 0,
          child: _panelOpen
              ? Material(
                  color: AppTheme.surface,
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Padding(
                        padding: const EdgeInsets.all(12),
                        child: FilledButton.icon(
                          onPressed: _newSession,
                          icon: const Icon(Icons.add, size: 18),
                          label: const Text('새 대화'),
                        ),
                      ),
                      Expanded(
                        child: _sessions.isEmpty
                            ? const Center(
                                child: Text(
                                  '대화 없음',
                                  style: TextStyle(color: AppTheme.inkMuted),
                                ),
                              )
                            : ListView.builder(
                                itemCount: _sessions.length,
                                itemBuilder: (context, i) {
                                  final s = _sessions[i];
                                  final selected =
                                      s.id.toInt() == _sessionId;
                                  return ListTile(
                                    selected: selected,
                                    title: Text(
                                      s.title,
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                    ),
                                    subtitle: Text(
                                      _sessionDate(s.updatedAt),
                                      style: const TextStyle(fontSize: 11),
                                    ),
                                    onTap: () => _selectSession(s.id.toInt()),
                                    trailing: IconButton(
                                      icon: const Icon(Icons.delete_outline,
                                          size: 18),
                                      onPressed: () =>
                                          _deleteSession(s.id.toInt()),
                                    ),
                                  );
                                },
                              ),
                      ),
                    ],
                  ),
                )
              : const SizedBox.shrink(),
        ),
        VerticalDivider(
          width: 1,
          color: _panelOpen ? AppTheme.border : Colors.transparent,
        ),
        Expanded(
          child: Column(
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 16, 16, 8),
                child: Row(
                  children: [
                    IconButton(
                      tooltip: _panelOpen ? '세션 패널 접기' : '세션 패널 펼치기',
                      onPressed: () =>
                          setState(() => _panelOpen = !_panelOpen),
                      icon: Icon(
                        _panelOpen
                            ? Icons.view_sidebar
                            : Icons.view_sidebar_outlined,
                      ),
                    ),
                    Expanded(
                      child: Text(
                        '채팅',
                        style: Theme.of(context)
                            .textTheme
                            .headlineSmall
                            ?.copyWith(fontWeight: FontWeight.bold),
                      ),
                    ),
                    if (_profiles.isNotEmpty)
                      Flexible(
                        child: DropdownMenu<String>(
                          initialSelection: _profileId,
                          dropdownMenuEntries: _profiles
                              .map(
                                (p) => DropdownMenuEntry(
                                  value: p.id,
                                  label: p.id.split('/').last,
                                ),
                              )
                              .toList(),
                          onSelected: (v) {
                            if (v == null) return;
                            final p =
                                _profiles.firstWhere((x) => x.id == v);
                            setState(() {
                              _profileId = v;
                              _ctx = p.contextDefault;
                            });
                          },
                        ),
                      ),
                    const SizedBox(width: 8),
                    if (profile != null && profile.isMultimodal)
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
                      icon: Icon(_serving
                          ? Icons.stop_circle_outlined
                          : Icons.play_circle_outline),
                    ),
                  ],
                ),
              ),
              if (profile != null && chatEnabled) ...[
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 16),
                  child: Row(
                    children: [
                      const Text('컨텍스트'),
                      const SizedBox(width: 8),
                      Expanded(
                        child: Wrap(
                          spacing: 6,
                          children: _ctxOptions
                              .map(
                                (c) => ChoiceChip(
                                  label: Text(formatContext(c)),
                                  selected: _ctx == c,
                                  onSelected: _streaming || _compressing
                                      ? null
                                      : (_) => setState(() => _ctx = c),
                                ),
                              )
                              .toList(),
                        ),
                      ),
                    ],
                  ),
                ),
                Padding(
                  padding:
                      const EdgeInsets.fromLTRB(16, 8, 16, 0),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        '컨텍스트 사용 ${formatContext(_promptTokens)}/${formatContext(_ctx)} '
                        '(${_contextUsagePct.toStringAsFixed(0)}%)',
                        style: Theme.of(context).textTheme.labelSmall,
                      ),
                      const SizedBox(height: 4),
                      ClipRRect(
                        borderRadius: BorderRadius.circular(4),
                        child: LinearProgressIndicator(
                          value: _contextUsagePct / 100,
                          minHeight: 6,
                          backgroundColor: AppTheme.border,
                          color: _contextUsagePct >= 70
                              ? AppTheme.warning
                              : AppTheme.primary,
                        ),
                      ),
                    ],
                  ),
                ),
              ],
              if (_compressing)
                const Padding(
                  padding: EdgeInsets.all(8),
                  child: Row(
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      SizedBox(
                        width: 16,
                        height: 16,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      ),
                      SizedBox(width: 8),
                      Text('이전 대화 요약·압축 중…'),
                    ],
                  ),
                ),
              if (_imagePath != null)
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 16),
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
                    padding: const EdgeInsets.symmetric(horizontal: 16),
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
                  padding: const EdgeInsets.all(16),
                  itemCount: _messages.length,
                  itemBuilder: (context, i) {
                    final m = _messages[i];
                    if (m.compressionNotice) {
                      return Align(
                        alignment: Alignment.center,
                        child: Chip(
                          avatar: const Icon(Icons.compress, size: 16),
                          label: const Text(
                            '이전 대화가 요약·압축되었습니다 — 토큰 절약',
                          ),
                          backgroundColor:
                              AppTheme.primary.withValues(alpha: 0.08),
                        ),
                      );
                    }
                    final isUser = m.role == 'user';
                    return Align(
                      alignment:
                          isUser ? Alignment.centerRight : Alignment.centerLeft,
                      child: Container(
                        margin: const EdgeInsets.only(bottom: 10),
                        padding: const EdgeInsets.symmetric(
                          horizontal: 14,
                          vertical: 10,
                        ),
                        constraints: const BoxConstraints(maxWidth: 520),
                        decoration: BoxDecoration(
                          color: isUser
                              ? AppTheme.primary.withValues(alpha: 0.15)
                              : AppTheme.surface,
                          borderRadius: BorderRadius.circular(12),
                        ),
                        child: Text(
                          m.content.isEmpty && m.streaming
                              ? '▌'
                              : m.content,
                          style: const TextStyle(height: 1.4),
                        ),
                      ),
                    );
                  },
                ),
              ),
              if (!chatEnabled && profile != null)
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 16),
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
                padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
                child: Row(
                  children: [
                    Expanded(
                      child: TextField(
                        controller: _controller,
                        enabled: chatEnabled && !_compressing,
                        decoration: InputDecoration(
                          hintText: chatEnabled
                              ? '메시지를 입력하세요…'
                              : '채팅 불가 모델',
                        ),
                        onSubmitted:
                            chatEnabled ? (_) => _send() : null,
                      ),
                    ),
                    const SizedBox(width: 12),
                    FilledButton(
                      onPressed: chatEnabled &&
                              !_streaming &&
                              !_compressing
                          ? _send
                          : null,
                      child: const Text('전송'),
                    ),
                  ],
                ),
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
  final bool compressionNotice;

  const _ChatBubble({
    this.role = '',
    this.content = '',
    this.streaming = false,
    this.compressionNotice = false,
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
        borderData: FlBorderData(show: false),
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