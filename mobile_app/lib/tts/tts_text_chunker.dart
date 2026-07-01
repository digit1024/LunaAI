/// Default max chars per Qween TTS API request.
const int kQweenTtsMaxInputLength = 600;

/// Safe max chars per built-in (flutter_tts) utterance on Android.
const int kBuiltInTtsMaxInputLength = 3500;

/// Splits text into chunks ≤ [maxLen] chars. Prefers sentence, then word boundaries.
List<String> chunkTextForTts(String text, {int maxLen = kQweenTtsMaxInputLength}) {
  final trimmed = text.trim();
  if (trimmed.isEmpty) return [];

  final chunks = <String>[];
  var start = 0;

  while (start < trimmed.length) {
    final end = (start + maxLen).clamp(0, trimmed.length);
    if (end >= trimmed.length) {
      chunks.add(trimmed.substring(start).trim());
      break;
    }

    var cut = end;
    final lastSentence = trimmed.lastIndexOf(RegExp(r'[.!?]\s+'), end);
    if (lastSentence > start) {
      cut = lastSentence + 1;
    } else {
      final lastSpace = trimmed.lastIndexOf(' ', end);
      if (lastSpace > start) cut = lastSpace + 1;
    }

    chunks.add(trimmed.substring(start, cut).trim());
    start = cut;
  }

  return chunks.where((c) => c.isNotEmpty).toList();
}
