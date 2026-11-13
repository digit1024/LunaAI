import 'package:flutter_test/flutter_test.dart';
import 'package:luna_mobile/voice/voice_catalog.dart';

void main() {
  test('OpenRouter TTS catalog includes Gemini and Orpheus families', () {
    final ids = VoiceCatalog.ttsModelsFor('openrouter').map((m) => m.model).toSet();
    expect(ids, contains('google/gemini-3.8-flash-tts'));
    expect(ids, contains('google/gemini-3.8-flash-lite-tts'));
    expect(ids, contains('canopylabs/orpheus-3b-0.1-ft'));
    expect(ids, contains('deepgram/aura-2'));
    expect(ids, contains('x-ai/grok-voice-tts-1.0'));
  });

  test('Gemini TTS resolves a valid default voice', () {
    final model = VoiceCatalog.ttsModel(
      'openrouter',
      'openrouter-gemini-3.8-flash-tts',
    );
    expect(model.defaultVoice, 'Kore');
    expect(model.voices, contains(model.defaultVoice));
  });
}
