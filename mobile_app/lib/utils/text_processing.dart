/// Strips emojis from text
String stripEmojis(String text) {
  // Remove emojis using regex pattern
  // This pattern matches most emojis including:
  // - Emoticons (😀, 😊, etc.)
  // - Symbols & Pictographs (🎉, 🚀, etc.)
  // - Transport & Map Symbols (🚗, 🏠, etc.)
  // - Flags (🇺🇸, etc.)
  // - And other emoji ranges
  final emojiRegex = RegExp(
    r'[\u{1F600}-\u{1F64F}]' // Emoticons
    r'|[\u{1F300}-\u{1F5FF}]' // Misc Symbols and Pictographs
    r'|[\u{1F680}-\u{1F6FF}]' // Transport and Map
    r'|[\u{1F1E0}-\u{1F1FF}]' // Flags
    r'|[\u{2600}-\u{26FF}]' // Misc symbols
    r'|[\u{2700}-\u{27BF}]' // Dingbats
    r'|[\u{FE00}-\u{FE0F}]' // Variation Selectors
    r'|[\u{1F900}-\u{1F9FF}]' // Supplemental Symbols and Pictographs
    r'|[\u{1FA00}-\u{1FA6F}]' // Chess Symbols
    r'|[\u{1FA70}-\u{1FAFF}]' // Symbols and Pictographs Extended-A
    r'|[\u{200D}]' // Zero Width Joiner
    r'|[\u{20D0}-\u{20FF}]' // Combining Diacritical Marks for Symbols
    r'|[\u{FE0F}]', // Variation Selector-16
    unicode: true,
  );
  return text.replaceAll(emojiRegex, '').trim();
}

/// Strips markdown formatting from text
String stripMarkdown(String text) {
  String result = text;

  // Remove headers (# ## ### etc.)
  result = result.replaceAll(RegExp(r'^#{1,6}\s+', multiLine: true), '');

  // Remove bold (**text** or __text__) - using replaceAllMapped for reliable capture groups
  result = result.replaceAllMapped(
    RegExp(r'\*\*(.+?)\*\*'),
    (match) => match.group(1) ?? '',
  );
  result = result.replaceAllMapped(
    RegExp(r'__(.+?)__'),
    (match) => match.group(1) ?? '',
  );

  // Remove italic (*text* or _text_) - must come after bold removal
  result = result.replaceAllMapped(
    RegExp(r'(?<!\*)\*([^*]+)\*(?!\*)'),
    (match) => match.group(1) ?? '',
  );
  result = result.replaceAllMapped(
    RegExp(r'(?<!_)_([^_]+)_(?!_)'),
    (match) => match.group(1) ?? '',
  );

  // Remove code blocks (```code```)
  result = result.replaceAll(RegExp(r'```[\s\S]*?```'), '');

  // Remove inline code (`code`)
  result = result.replaceAllMapped(
    RegExp(r'`([^`]+)`'),
    (match) => match.group(1) ?? '',
  );

  // Remove links [text](url) - keep the text
  result = result.replaceAllMapped(
    RegExp(r'\[([^\]]+)\]\([^\)]+\)'),
    (match) => match.group(1) ?? '',
  );

  // Remove images ![alt](url)
  result = result.replaceAll(RegExp(r'!\[([^\]]*)\]\([^\)]+\)'), '');

  // Remove strikethrough (~~text~~)
  result = result.replaceAllMapped(
    RegExp(r'~~(.+?)~~'),
    (match) => match.group(1) ?? '',
  );

  // Remove blockquotes (> text)
  result = result.replaceAll(RegExp(r'^>\s+', multiLine: true), '');

  // Remove horizontal rules (--- or ***)
  result = result.replaceAll(RegExp(r'^[-*]{3,}$', multiLine: true), '');

  // Remove list markers (- * + or 1. 2. etc.)
  result = result.replaceAll(RegExp(r'^[\s]*[-*+]\s+', multiLine: true), '');
  result = result.replaceAll(RegExp(r'^[\s]*\d+\.\s+', multiLine: true), '');

  // Clean up multiple spaces and newlines
  result = result.replaceAll(RegExp(r'\n{3,}'), '\n\n');
  result = result.replaceAll(RegExp(r'[ \t]+'), ' ');

  return result.trim();
}

/// Strips both emojis and markdown from text
String stripEmojisAndMarkdown(String text) {
  return stripMarkdown(stripEmojis(text));
}
