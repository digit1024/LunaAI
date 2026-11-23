import 'voice_catalog.dart';
import 'voice_models.dart';

/// Reads a stored API key for [providerId], or null when there is none.
typedef ApiKeyReader = Future<String?> Function(String providerId);

/// Explains why the selected TTS provider cannot speak yet, or returns null
/// when it is ready to use.
///
/// A voice loop answers out loud, so entering one without a working TTS
/// provider would produce a silent conversation. Callers gate entry on this.
///
/// Mirrors the endpoint/model/key resolution that `RemoteTtsProvider` performs
/// before it synthesises anything.
Future<String?> ttsSetupProblem(
  TtsConfig config, {
  required ApiKeyReader readApiKey,
}) async {
  final provider = VoiceCatalog.provider(config.providerId);
  // The on-device engine needs no endpoint or key.
  if (provider.isBuiltIn) return null;

  final preset = VoiceCatalog.ttsModel(config.providerId, config.modelId);
  final endpoint = config.endpointOverride.isNotEmpty
      ? config.endpointOverride
      : (provider.ttsEndpoint ?? '');
  final model =
      config.modelOverride.isNotEmpty ? config.modelOverride : preset.model;

  if (endpoint.isEmpty) {
    return 'Set the TTS endpoint for ${provider.label}.';
  }
  if (model.isEmpty) {
    return 'Set the TTS model for ${provider.label}.';
  }
  if (provider.requiresApiKey) {
    final key = await readApiKey(config.providerId);
    if (key == null || key.trim().isEmpty) {
      return 'Add the API key for ${provider.label}.';
    }
  }
  return null;
}
