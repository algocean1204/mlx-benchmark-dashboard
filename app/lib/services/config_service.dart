import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:path/path.dart' as p;

class AppConfig {
  final String projectRoot;
  final bool onboardingComplete;

  const AppConfig({
    required this.projectRoot,
    this.onboardingComplete = false,
  });

  AppConfig copyWith({String? projectRoot, bool? onboardingComplete}) {
    return AppConfig(
      projectRoot: projectRoot ?? this.projectRoot,
      onboardingComplete: onboardingComplete ?? this.onboardingComplete,
    );
  }

  Map<String, dynamic> toJson() => {
        'project_root': projectRoot,
        'onboarding_complete': onboardingComplete,
      };

  factory AppConfig.fromJson(Map<String, dynamic> json) {
    return AppConfig(
      projectRoot:
          json['project_root'] as String? ?? ConfigService.defaultProjectRoot,
      onboardingComplete: json['onboarding_complete'] as bool? ?? false,
    );
  }
}

class ConfigService extends ChangeNotifier {
  /// 기본 프로젝트 루트: AIDASH_ROOT 환경변수 → cwd 상위 탐색.
  /// 못 찾으면 빈 문자열 — Rust init이 번들 폴백으로 자동 전환한다.
  static final String defaultProjectRoot = _resolveDefaultRoot();

  static String _resolveDefaultRoot() {
    final env = Platform.environment['AIDASH_ROOT'];
    if (env != null && env.isNotEmpty) {
      return env;
    }
    var dir = Directory.current;
    for (var i = 0; i < 6; i++) {
      if (Directory(p.join(dir.path, 'profiles')).existsSync() &&
          Directory(p.join(dir.path, 'python')).existsSync()) {
        return dir.path;
      }
      final parent = dir.parent;
      if (parent.path == dir.path) break;
      dir = parent;
    }
    return '';
  }

  static String configDir() {
    final home = Platform.environment['HOME'];
    if (home == null || home.isEmpty) {
      throw StateError('HOME environment variable is not set');
    }
    return p.join(home, 'Library', 'Application Support', 'AI_Dashboard');
  }

  static String configPath() => p.join(configDir(), 'config.json');

  AppConfig _config = AppConfig(projectRoot: defaultProjectRoot);
  bool _loaded = false;

  AppConfig get config => _config;
  bool get isLoaded => _loaded;
  String get projectRoot => _config.projectRoot;
  bool get onboardingComplete => _config.onboardingComplete;

  Future<void> load() async {
    final file = File(configPath());
    if (!await file.exists()) {
      _config = AppConfig(projectRoot: defaultProjectRoot);
      _loaded = true;
      notifyListeners();
      return;
    }
    try {
      final raw = await file.readAsString();
      final json = jsonDecode(raw) as Map<String, dynamic>;
      _config = AppConfig.fromJson(json);
    } catch (_) {
      _config = AppConfig(projectRoot: defaultProjectRoot);
    }
    _loaded = true;
    notifyListeners();
  }

  Future<void> save() async {
    final dir = Directory(configDir());
    if (!await dir.exists()) {
      await dir.create(recursive: true);
    }
    final file = File(configPath());
    final encoder = const JsonEncoder.withIndent('  ');
    await file.writeAsString('${encoder.convert(_config.toJson())}\n');
    notifyListeners();
  }

  Future<void> setProjectRoot(String path) async {
    _config = _config.copyWith(projectRoot: path);
    await save();
  }

  Future<void> setOnboardingComplete(bool value) async {
    _config = _config.copyWith(onboardingComplete: value);
    await save();
  }
}