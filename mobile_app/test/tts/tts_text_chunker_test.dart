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

    test('never exceeds maxLen when a terminator sits on the boundary', () {
      // The '.' is at index 10, exactly at start + maxLen.
      final chunks = chunkTextForTts('Abcdefghij. Xyz.', maxLen: 10);
      expect(chunks, ['Abcdefghij', '. Xyz.']);
      for (final chunk in chunks) {
        expect(chunk.length, lessThanOrEqualTo(10));
      }
    });

    test('does not split after an abbreviation', () {
      final text = 'Dr. Smith went to the lab.';
      final chunks = chunkTextForTts(text, maxLen: 12);
      expect(chunks, ['Dr. Smith', 'went to the', 'lab.']);
      expect(chunks.join(' '), text);
    });

    test('does not split after a single-letter initial', () {
      final text = 'J. K. Rowling wrote it.';
      final chunks = chunkTextForTts(text, maxLen: 10);
      expect(chunks, ['J. K.', 'Rowling', 'wrote it.']);
      expect(chunks.join(' '), text);
    });
  });

  group('chunkTextForTts firstMaxLen', () {
    test('caps only the opening chunk', () {
      const text = 'aaaa bbbb cccc dddd eeee ffff gggg hhhh';
      final chunks = chunkTextForTts(text, maxLen: 20, firstMaxLen: 8);
      expect(chunks, ['aaaa', 'bbbb cccc dddd eeee', 'ffff gggg hhhh']);
      expect(chunks.first.length, lessThanOrEqualTo(8));
      expect(chunks[1].length, greaterThan(8));
      expect(chunks.join(' '), text);
    });

    test('is clamped to maxLen', () {
      final chunks = chunkTextForTts(
        List.filled(40, 'word').join(' '),
        maxLen: 20,
        firstMaxLen: 500,
      );
      for (final chunk in chunks) {
        expect(chunk.length, lessThanOrEqualTo(20));
      }
      expect(chunks.length, greaterThan(1));
    });

    test('defaults to maxLen so existing callers are unaffected', () {
      const text = 'aaaa bbbb cccc dddd';
      expect(
        chunkTextForTts(text, maxLen: 9),
        chunkTextForTts(text, maxLen: 9, firstMaxLen: 9),
      );
    });
  });

  group('chunkTextForTts invariants', () {
    test('keeps every chunk within budget and loses no words', () {
      final words = List.filled(60, 'word').join(' ');
      final text = '$words. Sentences matter too! Does this hold? Yes it does.';
      const maxLen = 37;
      final chunks = chunkTextForTts(text, maxLen: maxLen, firstMaxLen: 11);

      expect(chunks, isNotEmpty);
      for (final chunk in chunks) {
        expect(chunk, isNotEmpty);
        expect(chunk.length, lessThanOrEqualTo(maxLen));
        expect(chunk, chunk.trim());
      }
      expect(chunks.join(' ').replaceAll(RegExp(r'\s+'), ' '),
          text.replaceAll(RegExp(r'\s+'), ' ').trim());
    });

    test('makes progress on text without any boundary', () {
      final text = 'x' * 250;
      final chunks = chunkTextForTts(text, maxLen: 100);
      expect(chunks, ['x' * 100, 'x' * 100, 'x' * 50]);
    });
  });
}
