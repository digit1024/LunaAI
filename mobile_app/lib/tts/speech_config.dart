/// Configuration for markdown-to-speech conversion.
class SpeechConfig {
  const SpeechConfig({
    this.imageAltOnly = true,
    this.codeBlockHint = 'code block',
    this.speakInlineCode = true,
    this.tablePrefix = 'Table:',
    this.tableRowPrefix = 'Row:',
  });

  /// When true, speak image alt text only; omit images with no alt.
  final bool imageAltOnly;

  /// Spoken in place of fenced code blocks.
  final String codeBlockHint;

  /// When true, inline `code` content is spoken verbatim.
  final bool speakInlineCode;

  /// Prefix spoken once before the first table row.
  final String tablePrefix;

  /// Prefix spoken before each data row.
  final String tableRowPrefix;

  static const defaults = SpeechConfig();
}
