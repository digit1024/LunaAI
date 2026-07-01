import 'package:flutter_test/flutter_test.dart';
import 'package:luna_mobile/tts/tts_text_chunker.dart';

void main() {
  group('chunkTextForTts', () {
    test('returns empty for blank input', () {
      expect(chunkTextForTts(''), isEmpty);
      expect(chunkTextForTts('   '), isEmpty);
    });

    test('short text stays in one chunk', () {
      expect(chunkTextForTts('Hello world.'), ['Hello world.']);
    });

    test('long text splits without losing words', () {
      final text = List.filled(120, 'Word.').join(' ');
      final chunks = chunkTextForTts(text, maxLen: 100);
      expect(chunks.length, greaterThan(1));
      for (final chunk in chunks) {
        expect(chunk.length, lessThanOrEqualTo(102));
      }
      expect(chunks.join(' '), text);
    });

    test('prefers sentence boundaries', () {
      final text = 'One. Two. Three. Four.';
      final chunks = chunkTextForTts(text, maxLen: 10);
      expect(chunks.join(' '), text);
    });
  });
}
