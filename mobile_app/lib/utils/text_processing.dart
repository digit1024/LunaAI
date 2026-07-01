import '../tts/message_speech.dart';

export 'emoji_strip.dart' show stripEmojis;

/// Strips markdown formatting from text.
@Deprecated('Use prepareMessageForTts from package:luna_mobile/tts/message_speech.dart')
String stripMarkdown(String text) => prepareMessageForTts(text);

/// Strips both emojis and markdown from text.
@Deprecated('Use prepareMessageForTts from package:luna_mobile/tts/message_speech.dart')
String stripEmojisAndMarkdown(String text) => prepareMessageForTts(text);
