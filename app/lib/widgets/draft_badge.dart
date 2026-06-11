import 'package:app/theme/app_theme.dart';
import 'package:flutter/material.dart';

/// Small badge indicating a benchmark run used speculative draft acceleration.
class DraftBadge extends StatelessWidget {
  final bool compact;

  const DraftBadge({super.key, this.compact = true});

  @override
  Widget build(BuildContext context) {
    return Chip(
      label: Text(
        'draft 가속',
        style: TextStyle(
          fontSize: compact ? 11 : 12,
          color: AppTheme.ink,
        ),
      ),
      visualDensity: compact ? VisualDensity.compact : VisualDensity.standard,
      materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
      backgroundColor: AppTheme.primary.withValues(alpha: 0.12),
      side: BorderSide(color: AppTheme.primary.withValues(alpha: 0.35)),
      padding: compact ? const EdgeInsets.symmetric(horizontal: 4) : null,
    );
  }
}