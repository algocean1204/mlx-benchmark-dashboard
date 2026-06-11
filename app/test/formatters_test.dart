import 'package:app/theme/app_theme.dart';
import 'package:app/utils/formatters.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('formatContext', () {
    test('below 1024 stays raw', () {
      expect(formatContext(512), '512');
      expect(formatContext(1023), '1023');
    });

    test('integer K values', () {
      expect(formatContext(1024), '1K');
      expect(formatContext(2048), '2K');
      expect(formatContext(4096), '4K');
      expect(formatContext(32768), '32K');
      expect(formatContext(262144), '256K');
    });

    test('fractional K values', () {
      expect(formatContext(1536), '1.5K');
    });

    test('M values', () {
      expect(formatContext(524288), '512K');
      expect(formatContext(1048576), '1M');
    });
  });

  group('evalScoreColor', () {
    test('tier thresholds', () {
      expect(evalScoreColor(90), AppTheme.tierIdeal);
      expect(evalScoreColor(80), AppTheme.tierIdeal);
      expect(evalScoreColor(65), AppTheme.tierSluggish);
      expect(evalScoreColor(50), AppTheme.tierSluggish);
      expect(evalScoreColor(30), AppTheme.tierUnusable);
    });
  });
}