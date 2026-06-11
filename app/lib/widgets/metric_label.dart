import 'package:app/metric_help.dart';
import 'package:app/theme/app_theme.dart';
import 'package:flutter/material.dart';

/// 라벨 옆 ⓘ 아이콘 — 툴팁 + "자세히" 도움말 다이얼로그.
class MetricLabel extends StatelessWidget {
  final String term;
  final TextStyle? style;
  final bool showShortHint;

  const MetricLabel({
    super.key,
    required this.term,
    this.style,
    this.showShortHint = false,
  });

  @override
  Widget build(BuildContext context) {
    final entry = MetricHelp.get(term);
    final label = entry?.label ?? term;
    final theme = Theme.of(context);

    return Row(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.center,
      children: [
        Text(label, style: style ?? theme.textTheme.labelSmall),
        const SizedBox(width: 4),
        Tooltip(
          message: entry?.tooltip ?? term,
          child: InkWell(
            onTap: () => MetricHelp.showDetailDialog(context, term),
            borderRadius: BorderRadius.circular(10),
            child: Padding(
              padding: const EdgeInsets.all(2),
              child: Icon(
                Icons.info_outline,
                size: 14,
                color: AppTheme.inkMuted.withValues(alpha: 0.8),
              ),
            ),
          ),
        ),
        if (showShortHint && entry != null) ...[
          const SizedBox(width: 6),
          Text(
            entry.shortHint,
            style: theme.textTheme.labelSmall?.copyWith(
              color: AppTheme.inkMuted,
              fontSize: 10,
            ),
          ),
        ],
      ],
    );
  }
}

/// 수치 아래 회색 한 줄 부연.
class MetricHint extends StatelessWidget {
  final String term;

  const MetricHint({super.key, required this.term});

  @override
  Widget build(BuildContext context) {
    final hint = MetricHelp.get(term)?.shortHint;
    if (hint == null) return const SizedBox.shrink();
    return Text(
      hint,
      style: Theme.of(context).textTheme.labelSmall?.copyWith(
            color: AppTheme.inkMuted,
            fontSize: 10,
          ),
    );
  }
}