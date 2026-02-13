import 'dart:typed_data';

/// Wraps raw PCM (16-bit mono) in a WAV file header.
/// [pcmBytes] - raw PCM samples (16-bit little-endian)
/// [sampleRate] - e.g. 24000 for Qwen TTS
Uint8List pcmToWav(
  List<int> pcmBytes, {
  int sampleRate = 24000,
  int channels = 1,
}) {
  final dataSize = pcmBytes.length;
  final byteRate = sampleRate * channels * 2; // 16-bit = 2 bytes
  final blockAlign = channels * 2;
  final totalSize = 36 + dataSize;

  final buffer = ByteData(44);
  // RIFF header
  buffer.setUint8(0, 0x52); // R
  buffer.setUint8(1, 0x49); // I
  buffer.setUint8(2, 0x46); // F
  buffer.setUint8(3, 0x46); // F
  buffer.setUint32(4, totalSize, Endian.little);
  buffer.setUint8(8, 0x57);  // W
  buffer.setUint8(9, 0x41);  // A
  buffer.setUint8(10, 0x56); // V
  buffer.setUint8(11, 0x45); // E
  // fmt chunk
  buffer.setUint8(12, 0x66);  // f
  buffer.setUint8(13, 0x6d);  // m
  buffer.setUint8(14, 0x74);  // t
  buffer.setUint8(15, 0x20);  // space
  buffer.setUint32(16, 16, Endian.little); // chunk size
  buffer.setUint16(20, 1, Endian.little);  // audio format (PCM)
  buffer.setUint16(22, channels, Endian.little);
  buffer.setUint32(24, sampleRate, Endian.little);
  buffer.setUint32(28, byteRate, Endian.little);
  buffer.setUint16(32, blockAlign, Endian.little);
  buffer.setUint16(34, 16, Endian.little); // bits per sample
  // data chunk
  buffer.setUint8(36, 0x64);  // d
  buffer.setUint8(37, 0x61);  // a
  buffer.setUint8(38, 0x74);  // t
  buffer.setUint8(39, 0x61);  // a
  buffer.setUint32(40, dataSize, Endian.little);

  return Uint8List.fromList([
    ...buffer.buffer.asUint8List(),
    ...pcmBytes,
  ]);
}
