enum VoiceInputMode { tap, hold }

enum VoiceApiProtocol {
  openAiSpeech,
  dashScopeTts,
  openAiTranscription,
  dashScopeAsr,
}

enum VoiceModelKind { stt, tts }

/// A provider groups one STT and/or TTS transport.
class VoiceProviderPreset {
  const VoiceProviderPreset({
    required this.id,
    required this.label,
    this.sttProtocol,
    this.ttsProtocol,
    this.sttEndpoint,
    this.ttsEndpoint,
    this.requiresApiKey = true,
    this.isBuiltIn = false,
    this.isCustom = false,
  });

  final String id;
  final String label;
  final VoiceApiProtocol? sttProtocol;
  final VoiceApiProtocol? ttsProtocol;
  final String? sttEndpoint;
  final String? ttsEndpoint;
  final bool requiresApiKey;
  final bool isBuiltIn;
  final bool isCustom;
}

class SttModelPreset {
  const SttModelPreset({
    required this.id,
    required this.providerId,
    required this.label,
    required this.model,
    this.maxAudioSeconds = 60,
    this.supportsVocabularyHint = false,
  });

  final String id;
  final String providerId;
  final String label;
  final String model;
  final int maxAudioSeconds;
  final bool supportsVocabularyHint;
}

class TtsModelPreset {
  const TtsModelPreset({
    required this.id,
    required this.providerId,
    required this.label,
    required this.model,
    this.maxChars = 600,
    this.voices = const [],
    this.defaultVoice = '',
    this.supportsInstructions = false,
    this.responseFormat = 'mp3',
  });

  final String id;
  final String providerId;
  final String label;
  final String model;
  final int maxChars;
  final List<String> voices;
  final String defaultVoice;
  final bool supportsInstructions;
  final String responseFormat;
}

/// Concrete STT selection. Empty overrides fall back to the catalog preset.
class SttConfig {
  const SttConfig({
    this.providerId = 'qwen',
    this.modelId = 'qwen3-asr-flash',
    this.endpointOverride = '',
    this.modelOverride = '',
    this.extraParamsJson = '',
  });

  final String providerId;
  final String modelId;
  final String endpointOverride;
  final String modelOverride;
  final String extraParamsJson;

  SttConfig copyWith({
    String? providerId,
    String? modelId,
    String? endpointOverride,
    String? modelOverride,
    String? extraParamsJson,
  }) {
    return SttConfig(
      providerId: providerId ?? this.providerId,
      modelId: modelId ?? this.modelId,
      endpointOverride: endpointOverride ?? this.endpointOverride,
      modelOverride: modelOverride ?? this.modelOverride,
      extraParamsJson: extraParamsJson ?? this.extraParamsJson,
    );
  }

  Map<String, dynamic> toJson() => {
        'providerId': providerId,
        'modelId': modelId,
        'endpointOverride': endpointOverride,
        'modelOverride': modelOverride,
        'extraParamsJson': extraParamsJson,
      };

  factory SttConfig.fromJson(Map<String, dynamic> json) => SttConfig(
        providerId: json['providerId'] as String? ?? 'qwen',
        modelId: json['modelId'] as String? ?? 'qwen3-asr-flash',
        endpointOverride: json['endpointOverride'] as String? ?? '',
        modelOverride: json['modelOverride'] as String? ?? '',
        extraParamsJson: json['extraParamsJson'] as String? ?? '',
      );
}

/// Concrete TTS selection. Empty overrides fall back to the catalog preset.
class TtsConfig {
  const TtsConfig({
    this.providerId = 'builtin',
    this.modelId = 'builtin',
    this.voice = '',
    this.endpointOverride = '',
    this.modelOverride = '',
    this.extraParamsJson = '',
  });

  final String providerId;
  final String modelId;
  final String voice;
  final String endpointOverride;
  final String modelOverride;
  final String extraParamsJson;

  TtsConfig copyWith({
    String? providerId,
    String? modelId,
    String? voice,
    String? endpointOverride,
    String? modelOverride,
    String? extraParamsJson,
  }) {
    return TtsConfig(
      providerId: providerId ?? this.providerId,
      modelId: modelId ?? this.modelId,
      voice: voice ?? this.voice,
      endpointOverride: endpointOverride ?? this.endpointOverride,
      modelOverride: modelOverride ?? this.modelOverride,
      extraParamsJson: extraParamsJson ?? this.extraParamsJson,
    );
  }

  Map<String, dynamic> toJson() => {
        'providerId': providerId,
        'modelId': modelId,
        'voice': voice,
        'endpointOverride': endpointOverride,
        'modelOverride': modelOverride,
        'extraParamsJson': extraParamsJson,
      };

  factory TtsConfig.fromJson(Map<String, dynamic> json) => TtsConfig(
        providerId: json['providerId'] as String? ?? 'builtin',
        modelId: json['modelId'] as String? ?? 'builtin',
        voice: json['voice'] as String? ?? '',
        endpointOverride: json['endpointOverride'] as String? ?? '',
        modelOverride: json['modelOverride'] as String? ?? '',
        extraParamsJson: json['extraParamsJson'] as String? ?? '',
      );
}
