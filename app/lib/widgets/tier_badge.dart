import 'package:app/services/aidash_api.dart';
import 'package:app/src/rust/api.dart';
import 'package:app/theme/app_theme.dart';
import 'package:flutter/material.dart';
import 'package:provider/provider.dart';

Color tierColor(String key) => AppTheme.tierColorForKey(key);

/// TPS tier chip — always calls Rust [tpsTier] via [AidashApi], never reimplemented in Dart.
class TierBadge extends StatelessWidget {
  final double? decodeTps;
  final FrbTierInfo? tier;
  final bool compact;
  final bool large;

  const TierBadge({
    super.key,
    required this.decodeTps,
    this.tier,
    this.compact = false,
    this.large = false,
  });

  @override
  Widget build(BuildContext context) {
    if (decodeTps == null) {
      return Text(
        '—',
        style: TextStyle(
          color: Theme.of(context).colorScheme.onSurface.withValues(alpha: 0.5),
        ),
      );
    }
    final api = context.read<AidashApi>();
    final info = tier ?? api.tpsTier(decodeTps: decodeTps!);
    final color = tierColor(info.key);

    return Container(
      padding: EdgeInsets.symmetric(
        horizontal: large ? 14 : (compact ? 8 : 10),
        vertical: large ? 8 : (compact ? 2 : 4),
      ),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: color.withValues(alpha: 0.35)),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(info.badge, style: TextStyle(fontSize: large ? 20 : (compact ? 12 : 14))),
          if (!compact || large) const SizedBox(width: 6),
          Text(
            compact && !large
                ? info.label
                : '${decodeTps!.toStringAsFixed(1)} TPS · ${info.label}',
            style: TextStyle(
              color: color,
              fontSize: large ? 14 : (compact ? 11 : 12),
              fontWeight: FontWeight.w600,
            ),
          ),
        ],
      ),
    );
  }
}

/// Background bands for compare/bench charts (tier thresholds from Rust keys).
List<({double min, double max, Color color, String label})> tierBands(
  AidashApi api,
) {
  final samples = [5.0, 25.0, 50.0, 80.0, 120.0];
  return samples.map((tps) {
    final info = api.tpsTier(decodeTps: tps);
    final next = switch (info.key) {
      'unusable' => 10.0,
      'sluggish' => 40.0,
      'ideal' => 60.0,
      'fast' => 100.0,
      _ => 200.0,
    };
    final min = switch (info.key) {
      'unusable' => 0.0,
      'sluggish' => 10.0,
      'ideal' => 40.0,
      'fast' => 60.0,
      _ => 100.0,
    };
    return (
      min: min,
      max: next,
      color: tierColor(info.key).withValues(alpha: 0.08),
      label: info.label,
    );
  }).toList();
}