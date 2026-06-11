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

int platformIntToInt(dynamic value) {
  if (value == null) return 0;
  try {
    return (value as dynamic).toInt() as int;
  } catch (_) {
    return int.tryParse(value.toString()) ?? 0;
  }
}