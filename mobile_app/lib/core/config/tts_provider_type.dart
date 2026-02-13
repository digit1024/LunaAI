/// TTS provider selection: built-in device TTS or Qween (Alibaba Qwen TTS).
enum TtsProviderType {
  builtIn('Built-in TTS'),
  qween('Qween TTS');

  const TtsProviderType(this.label);

  final String label;
}
