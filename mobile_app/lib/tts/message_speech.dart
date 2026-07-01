import '../utils/emoji_strip.dart';
import 'markdown_to_speech.dart';
import 'speech_config.dart';

final _defaultConverter = MarkdownToSpeechConverter();

/// Prepares assistant message markdown for text-to-speech playback.
String prepareMessageForTts(
  String markdown, {
  SpeechConfig config = SpeechConfig.defaults,
}) {
  final trimmed = markdown.trim();
  if (trimmed.isEmpty) return '';

  final converter = identical(config, SpeechConfig.defaults)
      ? _defaultConverter
      : MarkdownToSpeechConverter(config: config);

  final speechText = converter.convert(trimmed);
  if (speechText.isEmpty) return '';

  return stripEmojis(speechText).replaceAll(RegExp(r'\s+'), ' ').trim();
}
