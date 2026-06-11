import 'package:app/theme/app_theme.dart';
import 'package:flutter/material.dart';

class ErrorCard extends StatelessWidget {
  final String message;
  final VoidCallback? onRetry;

  const ErrorCard({
    super.key,
    required this.message,
    this.onRetry,
  });

  static void showSnackBar(BuildContext context, String message) {
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text(message),
        action: SnackBarAction(
          label: '닫기',
          onPressed: () {},
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(20),
        child: Row(
          children: [
            const Icon(Icons.error_outline, color: AppTheme.error),
            const SizedBox(width: 12),
            Expanded(
              child: Text(
                message,
                style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                      color: AppTheme.error,
                    ),
              ),
            ),
            if (onRetry != null) ...[
              const SizedBox(width: 12),
              FilledButton.tonal(
                onPressed: onRetry,
                child: const Text('재시도'),
              ),
            ],
          ],
        ),
      ),
    );
  }
}