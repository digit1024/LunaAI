import 'dart:typed_data';

/// Splits PCM16 WAV audio into provider-sized chunks and merges transcripts.
class AudioChunker {
  AudioChunker._();

  static List<Uint8List> chunkWav(
    Uint8List bytes, {
    required int maxSeconds,
    double overlapSeconds = 1.0,
  }) {
    if (maxSeconds <= 0) return [bytes];
    final info = _WavInfo.parse(bytes);
    if (info == null) return [bytes];

    final frameSize = info.channels * (info.bitsPerSample ~/ 8);
    final bytesPerSecond = info.sampleRate * frameSize;
    if (frameSize <= 0 || bytesPerSecond <= 0) return [bytes];

    final maxBytes = maxSeconds * bytesPerSecond;
    if (info.dataSize <= maxBytes) return [bytes];

    final overlapRaw = (overlapSeconds * bytesPerSecond).round();
    final overlapBytes = (overlapRaw ~/ frameSize) * frameSize;
    final chunks = <Uint8List>[];
    var start = 0;
    while (start < info.dataSize) {
      final end = (start + maxBytes).clamp(0, info.dataSize);
      final alignedEnd = ((end - start) ~/ frameSize) * frameSize + start;
      if (alignedEnd <= start) break;

      final data = bytes.sublist(info.dataOffset + start, info.dataOffset + alignedEnd);
      chunks.add(_buildWav(info, data));

      if (alignedEnd >= info.dataSize) break;
      final next = alignedEnd - overlapBytes;
      start = next > start ? next : alignedEnd;
    }
    return chunks;
  }

  static String mergeTranscripts(List<String> parts) {
    final pieces = parts
        .map((p) => p.trim().replaceAll(RegExp(r'\s+'), ' '))
        .where((p) => p.isNotEmpty)
        .toList();
    if (pieces.isEmpty) return '';
    if (pieces.length == 1) return pieces.first;

    final words = pieces.first.split(' ').toList();
    for (var i = 1; i < pieces.length; i++) {
      final next = pieces[i].split(' ');
      final maxOverlap = words.length < next.length ? words.length : next.length;
      var overlap = 0;
      for (var k = maxOverlap; k > 0; k--) {
        final a = words.sublist(words.length - k).join(' ').toLowerCase();
        final b = next.sublist(0, k).join(' ').toLowerCase();
        if (a == b) {
          overlap = k;
          break;
        }
      }
      words.addAll(next.sublist(overlap));
    }
    return words.join(' ');
  }

  static Uint8List _buildWav(_WavInfo info, List<int> data) {
    final dataSize = data.length;
    final header = Uint8List(44);
    final bd = ByteData.view(header.buffer);
    _writeAscii(header, 0, 'RIFF');
    bd.setUint32(4, 36 + dataSize, Endian.little);
    _writeAscii(header, 8, 'WAVE');
    _writeAscii(header, 12, 'fmt ');
    bd.setUint32(16, 16, Endian.little);
    bd.setUint16(20, info.audioFormat, Endian.little);
    bd.setUint16(22, info.channels, Endian.little);
    bd.setUint32(24, info.sampleRate, Endian.little);
    bd.setUint32(28, info.byteRate, Endian.little);
    bd.setUint16(32, info.blockAlign, Endian.little);
    bd.setUint16(34, info.bitsPerSample, Endian.little);
    _writeAscii(header, 36, 'data');
    bd.setUint32(40, dataSize, Endian.little);

    final out = Uint8List(44 + dataSize);
    out.setRange(0, 44, header);
    out.setRange(44, out.length, data);
    return out;
  }

  static void _writeAscii(Uint8List bytes, int offset, String value) {
    for (var i = 0; i < value.length; i++) {
      bytes[offset + i] = value.codeUnitAt(i);
    }
  }
}

class _WavInfo {
  const _WavInfo({
    required this.audioFormat,
    required this.channels,
    required this.sampleRate,
    required this.byteRate,
    required this.blockAlign,
    required this.bitsPerSample,
    required this.dataOffset,
    required this.dataSize,
  });

  final int audioFormat;
  final int channels;
  final int sampleRate;
  final int byteRate;
  final int blockAlign;
  final int bitsPerSample;
  final int dataOffset;
  final int dataSize;

  static _WavInfo? parse(Uint8List bytes) {
    if (bytes.length < 44) return null;
    if (String.fromCharCodes(bytes.sublist(0, 4)) != 'RIFF') return null;
    if (String.fromCharCodes(bytes.sublist(8, 12)) != 'WAVE') return null;

    final bd = ByteData.view(bytes.buffer, bytes.offsetInBytes, bytes.length);
    int? audioFormat;
    int? channels;
    int? sampleRate;
    int? byteRate;
    int? blockAlign;
    int? bitsPerSample;
    int? dataOffset;
    int? dataSize;

    var offset = 12;
    while (offset + 8 <= bytes.length) {
      final id = String.fromCharCodes(bytes.sublist(offset, offset + 4));
      final size = bd.getUint32(offset + 4, Endian.little);
      final body = offset + 8;
      if (id == 'fmt ' && body + 16 <= bytes.length) {
        audioFormat = bd.getUint16(body, Endian.little);
        channels = bd.getUint16(body + 2, Endian.little);
        sampleRate = bd.getUint32(body + 4, Endian.little);
        byteRate = bd.getUint32(body + 8, Endian.little);
        blockAlign = bd.getUint16(body + 12, Endian.little);
        bitsPerSample = bd.getUint16(body + 14, Endian.little);
      } else if (id == 'data') {
        dataOffset = body;
        dataSize = size;
        if (dataOffset + dataSize > bytes.length) {
          dataSize = bytes.length - dataOffset;
        }
      }
      offset = body + size + (size.isOdd ? 1 : 0);
    }

    if (audioFormat == null ||
        channels == null ||
        sampleRate == null ||
        byteRate == null ||
        blockAlign == null ||
        bitsPerSample == null ||
        dataOffset == null ||
        dataSize == null ||
        dataSize <= 0) {
      return null;
    }

    return _WavInfo(
      audioFormat: audioFormat,
      channels: channels,
      sampleRate: sampleRate,
      byteRate: byteRate,
      blockAlign: blockAlign,
      bitsPerSample: bitsPerSample,
      dataOffset: dataOffset,
      dataSize: dataSize,
    );
  }
}
