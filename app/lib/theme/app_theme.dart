import 'package:flutter/material.dart';

class AppTheme {
  // 종이색 라이트 팔레트
  static const Color paper = Color(0xFFF6F1E7);
  static const Color surface = Color(0xFFFCF9F2);
  static const Color border = Color(0xFFE5DCC8);
  static const Color ink = Color(0xFF2C2A25);
  static const Color inkMuted = Color(0xFF6B6557);
  static const Color primary = Color(0xFF3A5A78);
  static const Color primaryMuted = Color(0xFF5A7A98);

  // TPS 등급색 (라이트)
  static const Color tierUnusable = Color(0xFFC44536);
  static const Color tierSluggish = Color(0xFFC87D2F);
  static const Color tierIdeal = Color(0xFF4E7C4A);
  static const Color tierFast = Color(0xFF3A6EA5);
  static const Color tierRealtime = Color(0xFF7B5EA7);

  static const Color success = Color(0xFF4E7C4A);
  static const Color warning = Color(0xFFC87D2F);
  static const Color error = Color(0xFFC44536);

  static ThemeData light() {
    final colorScheme = ColorScheme.light(
      primary: primary,
      onPrimary: surface,
      secondary: primaryMuted,
      onSecondary: surface,
      surface: surface,
      onSurface: ink,
      error: error,
      onError: surface,
      outline: border,
    );

    final base = ThemeData(
      useMaterial3: true,
      brightness: Brightness.light,
      colorScheme: colorScheme,
      scaffoldBackgroundColor: paper,
      cardTheme: CardThemeData(
        color: surface,
        elevation: 0,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(12),
          side: const BorderSide(color: border, width: 1),
        ),
        margin: const EdgeInsets.symmetric(vertical: 6),
      ),
      navigationRailTheme: NavigationRailThemeData(
        backgroundColor: surface,
        indicatorColor: primary.withValues(alpha: 0.12),
        selectedIconTheme: const IconThemeData(color: primary),
        unselectedIconTheme: const IconThemeData(color: inkMuted),
        selectedLabelTextStyle: const TextStyle(
          color: primary,
          fontWeight: FontWeight.w600,
          fontSize: 12,
        ),
        unselectedLabelTextStyle: const TextStyle(
          color: inkMuted,
          fontSize: 12,
        ),
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: paper,
        border: OutlineInputBorder(
          borderRadius: BorderRadius.circular(10),
          borderSide: const BorderSide(color: border),
        ),
        enabledBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(10),
          borderSide: const BorderSide(color: border),
        ),
        focusedBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(10),
          borderSide: const BorderSide(color: primary, width: 1.5),
        ),
        contentPadding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
        hintStyle: const TextStyle(color: inkMuted),
      ),
      chipTheme: ChipThemeData(
        backgroundColor: paper,
        selectedColor: primary.withValues(alpha: 0.15),
        labelStyle: const TextStyle(fontSize: 13, color: ink),
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(8),
          side: const BorderSide(color: border),
        ),
      ),
      dividerTheme: const DividerThemeData(
        color: border,
        thickness: 1,
      ),
      snackBarTheme: SnackBarThemeData(
        behavior: SnackBarBehavior.floating,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
        backgroundColor: ink,
        contentTextStyle: const TextStyle(color: surface),
      ),
      dialogTheme: DialogThemeData(
        backgroundColor: surface,
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(14)),
      ),
      appBarTheme: const AppBarTheme(
        backgroundColor: paper,
        elevation: 0,
        centerTitle: false,
        foregroundColor: ink,
      ),
      progressIndicatorTheme: const ProgressIndicatorThemeData(
        color: primary,
      ),
      filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
          backgroundColor: primary,
          foregroundColor: surface,
        ),
      ),
      textButtonTheme: TextButtonThemeData(
        style: TextButton.styleFrom(foregroundColor: primary),
      ),
    );

    return base.copyWith(
      textTheme: base.textTheme.apply(
        bodyColor: ink,
        displayColor: ink,
      ),
    );
  }

  static Color tierColorForKey(String key) {
    switch (key) {
      case 'unusable':
        return tierUnusable;
      case 'sluggish':
        return tierSluggish;
      case 'ideal':
        return tierIdeal;
      case 'fast':
        return tierFast;
      case 'realtime':
        return tierRealtime;
      default:
        return inkMuted;
    }
  }
}