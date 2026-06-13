import 'package:app/src/rust/api.dart';
import 'package:app/theme/app_theme.dart';
import 'package:app/utils/formatters.dart';
import 'package:flutter/material.dart';

/// 평가 템플릿 유형 한국어 라벨.
String evalTemplateKindLabel(String kind) {
  switch (kind) {
    case 'direct':
      return '직접 프롬프트';
    case 'needle':
      return 'needle 검색';
    default:
      return kind;
  }
}

/// 채점 방식 한 줄 요약.
String evalTemplateScoringHint() => '키워드 채점';

/// 벤치/평가 상태 영문 → 한국어.
String benchStateLabelKo(String state) {
  final lower = state.toLowerCase();
  if (lower == 'idle' || lower == 'ready') return '대기';
  if (lower == 'starting' ||
      lower == 'busy' ||
      lower == 'loading' ||
      lower == 'spawning' ||
      lower == 'running') {
    return '측정 중';
  }
  if (lower == 'completed' || lower == 'done' || lower == 'success') {
    return '완료';
  }
  if (lower == 'error' || lower == 'aborted' || lower == 'failed') {
    return '실패';
  }
  return state;
}

Color benchStateColorKo(String state) {
  final label = benchStateLabelKo(state);
  return switch (label) {
    '대기' => AppTheme.success,
    '측정 중' => AppTheme.warning,
    '완료' => AppTheme.primary,
    '실패' => AppTheme.error,
    _ => AppTheme.inkMuted,
  };
}

/// 64K+ 컨텍스트 평가 확인 다이얼로그.
Future<bool?> showLargeContextEvalDialog(BuildContext context, int ctx) {
  return showDialog<bool>(
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
}

/// 템플릿 1건 미리보기 카드.
class EvalTemplatePreviewCard extends StatelessWidget {
  final FrbEvalTemplateInfo template;

  const EvalTemplatePreviewCard({super.key, required this.template});

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Card(
      margin: EdgeInsets.zero,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              template.id,
              style: theme.textTheme.labelSmall?.copyWith(
                color: AppTheme.inkMuted,
                fontFamily: 'monospace',
              ),
            ),
            const SizedBox(height: 4),
            Text(
              template.description,
              style: theme.textTheme.bodyMedium?.copyWith(
                fontWeight: FontWeight.w500,
              ),
            ),
            const SizedBox(height: 2),
            Text(
              '${evalTemplateKindLabel(template.kind)} · ${evalTemplateScoringHint()}',
              style: theme.textTheme.bodySmall?.copyWith(
                color: AppTheme.inkMuted,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// 평가 총점 + 항목별 점수 카드 (모델 상세·벤치 공용).
class EvalScoreCard extends StatelessWidget {
  final int totalScore;
  final List<FrbEvalTemplateItemResult> items;

  const EvalScoreCard({
    super.key,
    required this.totalScore,
    required this.items,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          crossAxisAlignment: CrossAxisAlignment.end,
          children: [
            Text(
              '$totalScore',
              style: Theme.of(context).textTheme.displaySmall?.copyWith(
                    fontWeight: FontWeight.bold,
                    color: evalScoreColor(totalScore),
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
    );
  }
}

/// 벤치 상태 뱃지 (한국어).
class BenchStateChip extends StatelessWidget {
  final String state;

  const BenchStateChip({super.key, required this.state});

  @override
  Widget build(BuildContext context) {
    final color = benchStateColorKo(state);
    final label = benchStateLabelKo(state);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.15),
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: color.withValues(alpha: 0.4)),
      ),
      child: Text(label, style: TextStyle(color: color, fontSize: 12)),
    );
  }
}