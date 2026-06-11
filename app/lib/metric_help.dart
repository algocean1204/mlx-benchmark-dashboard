import 'package:flutter/material.dart';

/// 메트릭·태스크 용어 사전 — UI 단일 소스.
class MetricHelp {
  MetricHelp._();

  static const terms = <String, MetricEntry>{
    'TPS': MetricEntry(
      label: 'TPS(토큰속도)',
      tooltip: '초당 생성 토큰 수 — 클수록 빠름',
      shortHint: '초당 생성 토큰',
      detail:
          'TPS( Tokens Per Second )는 모델이 응답을 생성할 때 초당 몇 개의 토큰을 '
          '출력하는지 나타냅니다. 값이 클수록 생성 속도가 빠릅니다.\n\n'
          '일반적으로 사람이 읽는 속도는 약 10토큰/초 수준이므로, '
          '40 TPS 이상이면 쾌적하게 느껴집니다.',
      example: '예: 52 TPS → 약 5초에 260토큰(짧은 문단) 생성',
      tierTable: true,
    ),
    'TTFT': MetricEntry(
      label: 'TTFT(첫 응답)',
      tooltip: '첫 토큰까지 걸린 시간 — 응답 체감 속도',
      shortHint: '첫 응답까지',
      detail:
          'TTFT( Time To First Token )는 요청을 보낸 뒤 첫 번째 토큰이 '
          '도착하기까지 걸린 시간입니다. 채팅에서 "응답이 시작됐다"고 '
          '느끼는 시점과 직결됩니다.',
      example: '예: TTFT 180ms → 0.2초 안에 응답 시작',
    ),
    'Peak RAM': MetricEntry(
      label: 'Peak RAM(최대 메모리)',
      tooltip: '측정 중 최대 메모리 사용량',
      shortHint: '최대 메모리',
      detail:
          '벤치마크 실행 동안 프로세스가 사용한 물리 메모리의 최댓값입니다. '
          '모델 크기·컨텍스트·양자화에 따라 달라지며, 기기 RAM 한도와 '
          '직접 연관됩니다.',
      example: '예: Peak RAM 6GB → 8GB RAM 기기에서 여유 있음',
    ),
    'RTF': MetricEntry(
      label: 'RTF',
      tooltip: '실시간 대비 처리 배속 — 1.0 미만이면 실시간보다 빠름',
      shortHint: '실시간 대비 배속',
      detail:
          'RTF( Real-Time Factor )는 처리 시간을 오디오 길이로 나눈 값입니다. '
          '1.0 미만이면 실시간보다 빠르게 처리한 것이고, 1.0이면 실시간과 '
          '동일한 속도입니다. 낮을수록 빠릅니다.',
      example: '예: 10초 오디오를 2초에 처리 → RTF 0.2',
    ),
    'ctx': MetricEntry(
      label: 'ctx(컨텍스트)',
      tooltip: '모델이 한 번에 기억하는 토큰 길이',
      shortHint: '기억 토큰 길이',
      detail:
          '컨텍스트( context window )는 모델이 한 번의 요청에서 처리할 수 있는 '
          '최대 토큰 수입니다. 프롬프트와 생성 응답을 합쳐 이 한도 안에 '
          '들어야 합니다.',
      example: '예: 4K ctx → 약 3000단어 분량 입력+출력',
    ),
    '컨텍스트': MetricEntry(
      label: '컨텍스트',
      tooltip: '모델이 한 번에 기억하는 토큰 길이',
      shortHint: '기억 토큰 길이',
      detail:
          '컨텍스트( context window )는 모델이 한 번의 요청에서 처리할 수 있는 '
          '최대 토큰 수입니다. 프롬프트와 생성 응답을 합쳐 이 한도 안에 '
          '들어야 합니다.',
      example: '예: 4096 ctx → 약 3000단어 분량 입력+출력',
    ),
    'prefill': MetricEntry(
      label: 'prefill',
      tooltip: '입력 프롬프트 처리 단계',
      shortHint: '입력 처리',
      detail:
          'prefill 단계는 사용자가 보낸 프롬프트(입력)를 모델이 처리하는 '
          '구간입니다. TTFT는 주로 prefill이 끝나고 첫 출력 토큰이 나올 때까지의 '
          '시간을 포함합니다.',
      example: '예: 긴 프롬프트 → prefill 시간 증가 → TTFT 증가',
    ),
    '처리시간': MetricEntry(
      label: '처리시간',
      tooltip: '요청부터 완료까지 걸린 시간',
      shortHint: '완료까지',
      detail:
          '요청을 보낸 시점부터 작업이 완전히 끝날 때까지 걸린 총 시간입니다. '
          'STT·TTS·이미지 생성처럼 스트리밍 TPS가 없는 태스크에서 핵심 지표로 '
          '사용됩니다.',
      example: '예: STT 처리시간 1.2초 → 10초 오디오를 1.2초에 변환',
    ),
  };

  static MetricEntry? get(String term) => terms[term];

  static void showDetailDialog(BuildContext context, String term) {
    final entry = get(term);
    if (entry == null) return;
    showDialog<void>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text(entry.label),
        content: SingleChildScrollView(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(entry.detail),
              if (entry.example != null) ...[
                const SizedBox(height: 12),
                Text(
                  entry.example!,
                  style: Theme.of(ctx).textTheme.bodySmall?.copyWith(
                        fontStyle: FontStyle.italic,
                      ),
                ),
              ],
              if (entry.tierTable) ...[
                const SizedBox(height: 16),
                const Text('TPS 등급 기준', style: TextStyle(fontWeight: FontWeight.bold)),
                const SizedBox(height: 8),
                const _TpsTierTable(),
              ],
            ],
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx),
            child: const Text('닫기'),
          ),
        ],
      ),
    );
  }
}

class MetricEntry {
  final String label;
  final String tooltip;
  final String shortHint;
  final String detail;
  final String? example;
  final bool tierTable;

  const MetricEntry({
    required this.label,
    required this.tooltip,
    required this.shortHint,
    required this.detail,
    this.example,
    this.tierTable = false,
  });
}

class _TpsTierTable extends StatelessWidget {
  const _TpsTierTable();

  @override
  Widget build(BuildContext context) {
    const rows = [
      ('🔴', '<10', '사용 불가'),
      ('🟠', '10–40', '답답함'),
      ('🟢', '40–60', '이상적'),
      ('🔵', '60–100', '빠름'),
      ('🟣', '100+', '실시간급'),
    ];
    return Table(
      columnWidths: const {
        0: FixedColumnWidth(28),
        1: FixedColumnWidth(56),
        2: FlexColumnWidth(),
      },
      children: rows
          .map(
            (r) => TableRow(
              children: [
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 2),
                  child: Text(r.$1),
                ),
                Text(r.$2, style: const TextStyle(fontSize: 12)),
                Text(r.$3, style: const TextStyle(fontSize: 12)),
              ],
            ),
          )
          .toList(),
    );
  }
}