import 'dart:io';
import 'dart:typed_data';

/// PCM WAV 파일 길이(초). 파싱 실패 시 null — 추정 금지.
double? wavDurationSeconds(String path) {
  try {
    final file = File(path);
    if (!file.existsSync()) return null;
    final bytes = file.readAsBytesSync();
    if (bytes.length < 44) return null;
    final data = ByteData.sublistView(bytes);
    if (String.fromCharCodes(bytes.sublist(0, 4)) != 'RIFF') return null;
    if (String.fromCharCodes(bytes.sublist(8, 12)) != 'WAVE') return null;

    var offset = 12;
    int? sampleRate;
    int? channels;
    int? bitsPerSample;
    int? dataSize;

    while (offset + 8 <= bytes.length) {
      final chunkId = String.fromCharCodes(bytes.sublist(offset, offset + 4));
      final chunkSize = data.getUint32(offset + 4, Endian.little);
      offset += 8;
      if (offset + chunkSize > bytes.length) break;

      if (chunkId == 'fmt ') {
        channels = data.getUint16(offset + 2, Endian.little);
        sampleRate = data.getUint32(offset + 4, Endian.little);
        bitsPerSample = data.getUint16(offset + 14, Endian.little);
      } else if (chunkId == 'data') {
        dataSize = chunkSize;
      }
      offset += chunkSize + (chunkSize.isOdd ? 1 : 0);
    }

    if (sampleRate == null ||
        channels == null ||
        bitsPerSample == null ||
        dataSize == null ||
        sampleRate == 0 ||
        channels == 0 ||
        bitsPerSample == 0) {
      return null;
    }

    final bytesPerSec = sampleRate * channels * (bitsPerSample ~/ 8);
    if (bytesPerSec == 0) return null;
    return dataSize / bytesPerSec;
  } catch (_) {
    return null;
  }
}