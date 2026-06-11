import 'package:app/theme/app_theme.dart';
import 'package:flutter/material.dart';

String formatBytes(BigInt bytes) {
  final b = bytes.toDouble();
  if (b >= 1024 * 1024 * 1024) {
    return '${(b / (1024 * 1024 * 1024)).toStringAsFixed(1)} GB';
  }
  if (b >= 1024 * 1024) {
    return '${(b / (1024 * 1024)).toStringAsFixed(1)} MB';
  }
  return '${(b / 1024).toStringAsFixed(1)} KB';
}

String formatBytesInt(int bytes) => formatBytes(BigInt.from(bytes));

/// 컨텍스트 윈도 크기를 K/M 단위로 표기한다 (1024 기준).
String formatContext(int n) {
  if (n < 1024) return '$n';

  if (n >= 1024 * 1024) {
    final m = n / (1024 * 1024);
    if (m == m.roundToDouble()) return '${m.toInt()}M';
    return '${_trimTrailingZero(m.toStringAsFixed(1))}M';
  }

  final k = n / 1024;
  if (k == k.roundToDouble()) return '${k.toInt()}K';
  return '${_trimTrailingZero(k.toStringAsFixed(1))}K';
}

String _trimTrailingZero(String s) {
  if (s.endsWith('.0')) return s.substring(0, s.length - 2);
  return s;
}

Color evalScoreColor(int score) {
  if (score >= 80) return AppTheme.tierIdeal;
  if (score >= 50) return AppTheme.tierSluggish;
  return AppTheme.tierUnusable;
}

int platformIntToInt(dynamic value) {
  if (value == null) return 0;
  try {
    return (value as dynamic).toInt() as int;
  } catch (_) {
    return int.tryParse(value.toString()) ?? 0;
  }
}