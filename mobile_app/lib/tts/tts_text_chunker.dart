/// Default max chars per remote TTS API request.
const int kDefaultTtsMaxInputLength = 600;

/// Default max chars for the first chunk of a message.
///
/// A short opener gets audio to the listener sooner. Later chunks are
/// synthesised while earlier ones are still playing, so they can use the full
/// [kDefaultTtsMaxInputLength] and keep the number of handovers down.
const int kDefaultTtsFirstChunkLength = 200;

/// Safe max chars per built-in (flutter_tts) utterance on Android.
const int kBuiltInTtsMaxInputLength = 3500;

/// Tokens after which a full stop does not end a sentence.
const Set<String> _abbreviations = {
  'al',
  'approx',
  'apt',
  'ave',
  'blvd',
  'capt',
  'cf',
  'dept',
  'dr',
  'e.g',
  'etc',
  'fig',
  'gen',
  'i.e',
  'inc',
  'jr',
  'ltd',
  'mr',
  'mrs',
  'ms',
  'no',
  'prof',
  'rd',
  'rev',
  'sr',
  'st',
  'univ',
  'vol',
  'vs',
};

final RegExp _sentenceEnd = RegExp(r'[.!?]\s+');

/// Splits text into chunks no longer than [maxLen] chars, preferring sentence
/// and then word boundaries. No characters are dropped: joining the result with
/// a single space reproduces [text] whenever it used single spaces.
///
/// [firstMaxLen] caps only the first chunk, to shorten the wait before speech
/// starts. It defaults to [maxLen].
List<String> chunkTextForTts(
  String text, {
  int maxLen = kDefaultTtsMaxInputLength,
  int? firstMaxLen,
}) {
  final trimmed = text.trim();
  if (trimmed.isEmpty) return [];

  final limit = maxLen < 1 ? 1 : maxLen;
  final firstLimit = (firstMaxLen ?? limit).clamp(1, limit);

  final chunks = <String>[];
  var start = 0;
  var budget = firstLimit;

  while (start < trimmed.length) {
    if (trimmed.length - start <= budget) {
      final tail = trimmed.substring(start).trim();
      if (tail.isNotEmpty) chunks.add(tail);
      break;
    }

    final cut = _findCut(trimmed, start, start + budget);
    final piece = trimmed.substring(start, cut).trim();
    if (piece.isNotEmpty) chunks.add(piece);
    start = cut;
    budget = limit;
  }

  return chunks;
}

/// Returns the exclusive end index of the chunk starting at [start], never
/// further than [end] and always greater than [start] so callers make progress.
int _findCut(String text, int start, int end) {
  final sentence = _lastSentenceEnd(text, start, end);
  if (sentence != null) return sentence;

  final lastSpace = text.lastIndexOf(' ', end);
  if (lastSpace > start) return lastSpace + 1;

  return end;
}

/// Index just past the last sentence terminator at or before [end], skipping
/// full stops that belong to an abbreviation or an initial.
///
/// Searching from `end - 1` keeps the resulting chunk within [end], so a
/// request never exceeds the caller's character budget.
int? _lastSentenceEnd(String text, int start, int end) {
  var from = end - 1;
  // Bounded: a run of abbreviations must not cost a scan of the whole message.
  for (var attempts = 0; attempts < 4 && from > start; attempts++) {
    final match = text.lastIndexOf(_sentenceEnd, from);
    if (match <= start) return null;
    if (!_isAbbreviated(text, match)) return match + 1;
    from = match - 1;
  }
  return null;
}

bool _isAbbreviated(String text, int terminatorIndex) {
  var begin = terminatorIndex;
  while (begin > 0 && _isTokenChar(text.codeUnitAt(begin - 1))) {
    begin--;
  }
  final token = text.substring(begin, terminatorIndex).toLowerCase();
  if (token.isEmpty) return false;
  if (_abbreviations.contains(token)) return true;
  // Single letters are initials ("J. K. Rowling").
  return token.length == 1 && token.codeUnitAt(0) >= 0x61;
}

bool _isTokenChar(int codeUnit) {
  final isDigit = codeUnit >= 0x30 && codeUnit <= 0x39;
  final isUpper = codeUnit >= 0x41 && codeUnit <= 0x5A;
  final isLower = codeUnit >= 0x61 && codeUnit <= 0x7A;
  // Keep "e.g" together so the dotted form matches the abbreviation table.
  return isDigit || isUpper || isLower || codeUnit == 0x2E;
}
