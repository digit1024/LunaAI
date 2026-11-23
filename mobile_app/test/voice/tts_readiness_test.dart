import 'package:flutter_test/flutter_test.dart';
import 'package:luna_mobile/voice/tts_readiness.dart';
import 'package:luna_mobile/voice/voice_models.dart';

void main() {
  final keyRequests = <String>[];

  Future<String?> problem(TtsConfig config, {String? key = 'sk-stored'}) {
    keyRequests.clear();
    return ttsSetupProblem(
      config,
      readApiKey: (providerId) async {
        keyRequests.add(providerId);
        return key;
      },
    );
  }

  group('ttsSetupProblem', () {
    test('the built-in device voice needs no setup', () async {
      expect(await problem(const TtsConfig(), key: null), isNull);
      expect(keyRequests, isEmpty);
    });

    test('a hosted provider with a stored key is ready', () async {
      expect(
        await problem(
          const TtsConfig(providerId: 'qwen', modelId: 'qwen3-tts-flash'),
        ),
        isNull,
      );
      expect(keyRequests, ['qwen']);
    });

    test('a missing key blocks entry', () async {
      expect(
        await problem(
          const TtsConfig(providerId: 'qwen', modelId: 'qwen3-tts-flash'),
          key: null,
        ),
        'Add the API key for Qwen / DashScope.',
      );
    });

    test('a blank key counts as missing', () async {
      expect(
        await problem(
          const TtsConfig(
            providerId: 'openrouter',
            modelId: 'openrouter-kokoro-82m',
          ),
          key: '   ',
        ),
        'Add the API key for OpenRouter.',
      );
    });

    test('a custom provider without an endpoint blocks entry', () async {
      expect(
        await problem(
          const TtsConfig(providerId: 'custom_openai', modelId: 'custom'),
        ),
        'Set the TTS endpoint for Custom (OpenAI-compatible).',
      );
    });

    test('a custom provider without a model blocks entry', () async {
      expect(
        await problem(
          const TtsConfig(
            providerId: 'custom_openai',
            modelId: 'custom',
            endpointOverride: 'https://example.test/v1/audio/speech',
          ),
        ),
        'Set the TTS model for Custom (OpenAI-compatible).',
      );
    });

    test('custom overrides satisfy the check', () async {
      expect(
        await problem(
          const TtsConfig(
            providerId: 'custom_openai',
            modelId: 'custom',
            endpointOverride: 'https://example.test/v1/audio/speech',
            modelOverride: 'acme-tts',
          ),
        ),
        isNull,
      );
    });
  });
}
