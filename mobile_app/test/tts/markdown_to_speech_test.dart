import 'package:flutter_test/flutter_test.dart';
import 'package:luna_mobile/tts/markdown_to_speech.dart';
import 'package:luna_mobile/tts/message_speech.dart';

void main() {
  final converter = MarkdownToSpeechConverter();

  String convert(String input) => converter.convert(input);
  String tts(String input) => prepareMessageForTts(input);

  group('MarkdownToSpeechConverter', () {
    test('bold and italic', () {
      expect(convert('**bold**'), 'bold');
      expect(convert('*italic*'), 'italic');
      expect(convert('**bold** and *italic*'), contains('bold'));
      expect(convert('**bold** and *italic*'), isNot(contains('\$1')));
    });

    test('links speak label only', () {
      expect(
        convert('See [Google](https://google.com) for more'),
        contains('Google'),
      );
      expect(
        convert('See [Google](https://google.com) for more'),
        isNot(contains('https://')),
      );
    });

    test('bare URLs are omitted', () {
      final out = convert('Visit https://example.com today');
      expect(out, isNot(contains('https://')));
      expect(out, contains('Visit'));
      expect(out, contains('today'));
    });

    test('images speak alt only', () {
      expect(
        convert('Here ![chart](https://example.com/img.png) end'),
        contains('chart'),
      );
      expect(
        convert('Here ![chart](https://example.com/img.png) end'),
        isNot(contains('!')),
      );
      expect(
        convert('Here ![chart](https://example.com/img.png) end'),
        isNot(contains('https://')),
      );
    });

    test('images without alt are omitted', () {
      expect(convert('![](https://example.com/img.png)'), isEmpty);
    });

    test('luna-static images speak alt only', () {
      expect(
        convert('![diagram](luna-static:charts/out.png)'),
        'diagram',
      );
    });

    test('GFM table is spoken row by row', () {
      const table = '''
| Name | Age |
|------|-----|
| Alice | 30 |
''';
      final out = convert(table);
      expect(out, contains('Table:'));
      expect(out, contains('Row:'));
      expect(out, contains('Name Alice'));
      expect(out, contains('Age 30'));
      expect(out, isNot(contains('|')));
    });

    test('fenced code block speaks hint', () {
      const block = '''
Before
```dart
print("hello");
```
After
''';
      final out = convert(block);
      expect(out, contains('code block'));
      expect(out, contains('Before'));
      expect(out, contains('After'));
      expect(out, isNot(contains('print')));
    });

    test('inline code is spoken', () {
      expect(convert('Use `foo()` function'), contains('foo()'));
    });

    test('dollar amounts become dollars phrasing', () {
      expect(convert('The cost is \$100'), contains('100 dollars'));
    });

    test('regex capture groups are not spoken as one dollar', () {
      final out = convert('Use capture group \$1 in regex');
      expect(out, contains('group 1'));
      expect(out, isNot(contains('\$1')));
    });

    test('multi paragraph retains content', () {
      const input = 'First paragraph.\n\nSecond paragraph.';
      final out = convert(input);
      expect(out, contains('First paragraph'));
      expect(out, contains('Second paragraph'));
    });

    test('lists speak items', () {
      final out = convert('- item one\n- item two');
      expect(out, contains('item one'));
      expect(out, contains('item two'));
    });
  });

  group('prepareMessageForTts', () {
    test('strips emojis after markdown conversion', () {
      expect(tts('Hello 😀 world'), 'Hello world');
    });

    test('play-button style markdown does not produce dollar one', () {
      expect(tts('**hello** world'), isNot(contains('\$1')));
      expect(tts('[Google](https://x.com)'), isNot(contains('\$1')));
    });
  });
}
