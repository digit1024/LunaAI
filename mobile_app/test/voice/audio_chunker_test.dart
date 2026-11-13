import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:luna_mobile/voice/audio_chunker.dart';

void main() {
  group('mergeTranscripts', () {
    test('removes overlapping words', () {
      expect(
        AudioChunker.mergeTranscripts([
          'hello world this is',
          'this is a test',
        ]),
        'hello world this is a test',
      );
    });

    test('keeps non-overlapping transcripts', () {
      expect(
        AudioChunker.mergeTranscripts(['hello', 'world']),
        'hello world',
      );
    });

    test('ignores empty chunks', () {
      expect(AudioChunker.mergeTranscripts(['', '  ', 'hello']), 'hello');
    });
  });

  group('chunkWav', () {
    test('returns the original file when it fits', () {
      final wav = _wav(seconds: 2);
      final chunks = AudioChunker.chunkWav(wav, maxSeconds: 3);
      expect(chunks, hasLength(1));
      expect(chunks.first, same(wav));
    });

    test('splits long files and produces valid WAV headers', () {
      final wav = _wav(seconds: 7);
      final chunks = AudioChunker.chunkWav(
        wav,
        maxSeconds: 3,
        overlapSeconds: 0.5,
      );
      expect(chunks.length, greaterThan(1));
      for (final chunk in chunks) {
        expect(String.fromCharCodes(chunk.sublist(0, 4)), 'RIFF');
        expect(String.fromCharCodes(chunk.sublist(8, 12)), 'WAVE');
        expect(String.fromCharCodes(chunk.sublist(36, 40)), 'data');
        final bd = ByteData.view(chunk.buffer);
        expect(bd.getUint32(4, Endian.little), chunk.length - 8);
        expect(bd.getUint32(40, Endian.little), chunk.length - 44);
      }
    });
  });
}

Uint8List _wav({required int seconds}) {
  const sampleRate = 16000;
  const channels = 1;
  const bits = 16;
  final dataSize = seconds * sampleRate * channels * (bits ~/ 8);
  final bytes = Uint8List(44 + dataSize);
  final bd = ByteData.view(bytes.buffer);
  void ascii(int offset, String value) {
    for (var i = 0; i < value.length; i++) {
      bytes[offset + i] = value.codeUnitAt(i);
    }
  }

  ascii(0, 'RIFF');
  bd.setUint32(4, 36 + dataSize, Endian.little);
  ascii(8, 'WAVE');
  ascii(12, 'fmt ');
  bd.setUint32(16, 16, Endian.little);
  bd.setUint16(20, 1, Endian.little);
  bd.setUint16(22, channels, Endian.little);
  bd.setUint32(24, sampleRate, Endian.little);
  bd.setUint32(28, sampleRate * channels * (bits ~/ 8), Endian.little);
  bd.setUint16(32, channels * (bits ~/ 8), Endian.little);
  bd.setUint16(34, bits, Endian.little);
  ascii(36, 'data');
  bd.setUint32(40, dataSize, Endian.little);
  return bytes;
}
