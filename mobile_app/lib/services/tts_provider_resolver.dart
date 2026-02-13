import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../core/config/tts_preferences.dart';
import '../core/config/tts_provider_type.dart';
import 'built_in_tts_provider.dart';
import 'qween_tts_provider.dart';
import 'tts_provider.dart';
import 'tts_service.dart';

/// Resolves the active TTS provider based on user preferences.
final ttsProviderResolver = Provider<TtsProvider>((ref) {
  final ttsPrefs = ref.watch(ttsPreferencesProvider);
  final ttsService = ref.read(ttsServiceProvider);
  final qweenProvider = ref.read(qweenTtsProvider);

  if (ttsPrefs.providerType == TtsProviderType.qween) {
    return qweenProvider;
  }

  return BuiltInTtsProvider(
    ttsService: ttsService,
    getLanguage: () => ref.read(ttsPreferencesProvider).language,
  );
});
