import 'dart:io';

import 'package:app/app_shell.dart';
import 'package:app/screens/onboarding_screen.dart';
import 'package:app/services/aidash_api.dart';
import 'package:app/services/config_service.dart';
import 'package:app/services/frb_aidash_api.dart';
import 'package:app/src/rust/frb_generated.dart';
import 'package:app/theme/app_theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';
import 'package:path/path.dart' as p;
import 'package:provider/provider.dart';

/// 개발 모드 dylib 경로: AIDASH_ROOT 환경변수 → cwd 상위 탐색.
String? _devDylibPath() {
  const rel = 'core/target/release/libaidash_frb.dylib';
  final env = Platform.environment['AIDASH_ROOT'];
  if (env != null && env.isNotEmpty) {
    final cand = p.join(env, rel);
    if (File(cand).existsSync()) return cand;
  }
  var dir = Directory.current;
  for (var i = 0; i < 6; i++) {
    final cand = p.join(dir.path, rel);
    if (File(cand).existsSync()) return cand;
    final parent = dir.parent;
    if (parent.path == dir.path) break;
    dir = parent;
  }
  return null;
}

ExternalLibrary? _resolveRustLibrary() {
  if (!Platform.isMacOS) {
    return null;
  }

  final bundleDylib = p.normalize(
    p.join(
      p.dirname(Platform.resolvedExecutable),
      '..',
      'Frameworks',
      'libaidash_frb.dylib',
    ),
  );
  if (File(bundleDylib).existsSync()) {
    return ExternalLibrary.open(bundleDylib);
  }

  final devDylib = _devDylibPath();
  if (devDylib != null) {
    return ExternalLibrary.open(devDylib);
  }

  return null;
}

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();

  final configService = ConfigService();
  await configService.load();

  final rustLibrary = _resolveRustLibrary();
  await AidashFrb.init(
    externalLibrary: rustLibrary,
  );
  final api = FrbAidashApi();
  api.init(rootPath: configService.projectRoot);

  runApp(AiDashboardApp(api: api, configService: configService));
}

class AiDashboardApp extends StatefulWidget {
  final AidashApi api;
  final ConfigService configService;

  const AiDashboardApp({
    super.key,
    required this.api,
    required this.configService,
  });

  @override
  State<AiDashboardApp> createState() => _AiDashboardAppState();
}

class _AiDashboardAppState extends State<AiDashboardApp> {
  bool _showOnboarding = false;

  @override
  void initState() {
    super.initState();
    _showOnboarding = !widget.configService.onboardingComplete;
  }

  void _onOnboardingComplete() {
    setState(() => _showOnboarding = false);
  }

  @override
  Widget build(BuildContext context) {
    return MultiProvider(
      providers: [
        Provider<AidashApi>.value(value: widget.api),
        ChangeNotifierProvider<ConfigService>.value(value: widget.configService),
      ],
      child: MaterialApp(
        title: 'AI Dashboard',
        debugShowCheckedModeBanner: false,
        theme: AppTheme.light(),
        home: _showOnboarding
            ? OnboardingScreen(onComplete: _onOnboardingComplete)
            : const AppShell(),
      ),
    );
  }
}