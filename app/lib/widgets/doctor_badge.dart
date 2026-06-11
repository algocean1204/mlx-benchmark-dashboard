import 'package:app/theme/app_theme.dart';
import 'package:flutter/material.dart';

enum DoctorStatus { ok, warn, missing, info }

DoctorStatus parseDoctorStatus(String status) {
  switch (status.toLowerCase()) {
    case 'ok':
      return DoctorStatus.ok;
    case 'warn':
    case 'warning':
      return DoctorStatus.warn;
    case 'missing':
      return DoctorStatus.missing;
    default:
      return DoctorStatus.info;
  }
}

/// 3-state doctor badge: ✅ 준비됨 / ⚠️ 조치 필요 / ⬇️ 설치 가능
class DoctorBadge extends StatelessWidget {
  final String status;
  final bool showLabel;

  const DoctorBadge({
    super.key,
    required this.status,
    this.showLabel = true,
  });

  @override
  Widget build(BuildContext context) {
    final parsed = parseDoctorStatus(status);
    final (icon, label, color) = switch (parsed) {
      DoctorStatus.ok => ('✅', '준비됨', AppTheme.tierIdeal),
      DoctorStatus.warn => ('⚠️', '조치 필요', AppTheme.tierSluggish),
      DoctorStatus.missing => ('⬇️', '설치 가능', AppTheme.tierFast),
      DoctorStatus.info => ('ℹ️', '정보', AppTheme.inkMuted),
    };

    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.15),
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: color.withValues(alpha: 0.4)),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(icon, style: const TextStyle(fontSize: 13)),
          if (showLabel) ...[
            const SizedBox(width: 6),
            Text(
              label,
              style: TextStyle(
                color: color,
                fontSize: 12,
                fontWeight: FontWeight.w600,
              ),
            ),
          ],
        ],
      ),
    );
  }
}