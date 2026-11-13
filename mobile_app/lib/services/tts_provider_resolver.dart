import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../voice/remote_tts_provider.dart';
import '../voice/voice_preferences.dart';
import 'built_in_tts_provider.dart';
import 'tts_provider.dart';
import 'tts_service.dart';

/// Resolves the active TTS provider from voice preferences.
final ttsProviderResolver = Provider<TtsProvider>((ref) {
  final prefs = ref.watch(voicePreferencesProvider);
  final ttsService = ref.read(ttsServiceProvider);

  if (prefs.tts.providerId == 'builtin') {
    return BuiltInTtsProvider(
      ttsService: ttsService,
      getLanguage: () => ref.read(voicePreferencesProvider).language,
    );
  }

  final remote = RemoteTtsProvider(ref);
  ref.onDispose(remote.dispose);
  return remote;
});
