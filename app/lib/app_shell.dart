import 'package:app/screens/bench_screen.dart';
import 'package:app/screens/chat_screen.dart';
import 'package:app/screens/compare_screen.dart';
import 'package:app/screens/dashboard_screen.dart';
import 'package:app/screens/model_detail_screen.dart';
import 'package:app/screens/onboarding_screen.dart';
import 'package:app/screens/model_manage_screen.dart';
import 'package:app/screens/settings_screen.dart';
import 'package:flutter/material.dart';

class AppShell extends StatefulWidget {
  const AppShell({super.key});

  @override
  State<AppShell> createState() => _AppShellState();
}

class _AppShellState extends State<AppShell> {
  int _index = 0;
  String? _selectedModelId;

  static const _destinations = [
    (icon: Icons.dashboard_outlined, selected: Icons.dashboard, label: '대시보드'),
    (icon: Icons.model_training_outlined, selected: Icons.model_training, label: '모델'),
    (icon: Icons.compare_arrows_outlined, selected: Icons.compare_arrows, label: '비교'),
    (icon: Icons.speed_outlined, selected: Icons.speed, label: '벤치'),
    (icon: Icons.chat_outlined, selected: Icons.chat, label: '채팅'),
    (icon: Icons.settings_outlined, selected: Icons.settings, label: '설정'),
    (icon: Icons.health_and_safety_outlined, selected: Icons.health_and_safety, label: '환경 점검'),
    (icon: Icons.storage_outlined, selected: Icons.storage, label: '모델 관리'),
  ];

  Widget _buildBody() {
    switch (_index) {
      case 0:
        return DashboardScreen(
          onModelSelected: (id) {
            setState(() {
              _selectedModelId = id;
              _index = 1;
            });
          },
        );
      case 1:
        return ModelDetailScreen(modelId: _selectedModelId);
      case 2:
        return const CompareScreen();
      case 3:
        return const BenchScreen();
      case 4:
        return const ChatScreen();
      case 5:
        return const SettingsScreen();
      case 6:
        return OnboardingScreen(
          onComplete: () => setState(() => _index = 0),
        );
      case 7:
        return const ModelManageScreen();
      default:
        return const SizedBox();
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      body: Row(
        children: [
          NavigationRail(
            selectedIndex: _index,
            onDestinationSelected: (i) => setState(() => _index = i),
            labelType: NavigationRailLabelType.all,
            destinations: _destinations
                .map(
                  (d) => NavigationRailDestination(
                    icon: Icon(d.icon),
                    selectedIcon: Icon(d.selected),
                    label: Text(d.label),
                  ),
                )
                .toList(),
          ),
          const VerticalDivider(width: 1),
          Expanded(child: _buildBody()),
        ],
      ),
    );
  }
}