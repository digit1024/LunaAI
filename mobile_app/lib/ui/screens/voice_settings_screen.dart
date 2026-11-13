import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../services/tts_provider_resolver.dart';
import '../../voice/stt_service.dart';
import '../../voice/voice_catalog.dart';
import '../../voice/voice_models.dart';
import '../../voice/voice_preferences.dart';
import '../../voice/voice_recorder.dart';

const _languages = <String, String>{
  'en-US': 'English (US)',
  'en-GB': 'English (UK)',
  'pl-PL': 'Polish',
  'de-DE': 'German',
  'fr-FR': 'French',
  'es-ES': 'Spanish',
  'it-IT': 'Italian',
  'pt-BR': 'Portuguese (Brazil)',
  'pt-PT': 'Portuguese (Portugal)',
  'ru-RU': 'Russian',
  'uk-UA': 'Ukrainian',
  'cs-CZ': 'Czech',
  'sk-SK': 'Slovak',
  'nl-NL': 'Dutch',
  'sv-SE': 'Swedish',
  'da-DK': 'Danish',
  'fi-FI': 'Finnish',
  'nb-NO': 'Norwegian',
  'hu-HU': 'Hungarian',
  'ro-RO': 'Romanian',
  'tr-TR': 'Turkish',
  'el-GR': 'Greek',
  'he-IL': 'Hebrew',
  'ar-SA': 'Arabic',
  'hi-IN': 'Hindi',
  'ja-JP': 'Japanese',
  'ko-KR': 'Korean',
  'zh-CN': 'Chinese (Mandarin)',
  'th-TH': 'Thai',
  'vi-VN': 'Vietnamese',
  'id-ID': 'Indonesian',
};

class VoiceSettingsScreen extends ConsumerStatefulWidget {
  const VoiceSettingsScreen({super.key});

  @override
  ConsumerState<VoiceSettingsScreen> createState() =>
      _VoiceSettingsScreenState();
}

class _VoiceSettingsScreenState extends ConsumerState<VoiceSettingsScreen> {
  late final TextEditingController _sttKey;
  late final TextEditingController _ttsKey;
  late final TextEditingController _sttEndpoint;
  late final TextEditingController _sttModel;
  late final TextEditingController _sttExtra;
  late final TextEditingController _ttsEndpoint;
  late final TextEditingController _ttsModel;
  late final TextEditingController _ttsVoice;
  late final TextEditingController _ttsExtra;

  bool _loading = true;
  bool _obscureSttKey = true;
  bool _obscureTtsKey = true;
  bool _testingMic = false;
  bool _previewing = false;

  @override
  void initState() {
    super.initState();
    _sttKey = TextEditingController();
    _ttsKey = TextEditingController();
    _sttEndpoint = TextEditingController();
    _sttModel = TextEditingController();
    _sttExtra = TextEditingController();
    _ttsEndpoint = TextEditingController();
    _ttsModel = TextEditingController();
    _ttsVoice = TextEditingController();
    _ttsExtra = TextEditingController();
    unawaited(_reload());
  }

  Future<void> _reload() async {
    final notifier = ref.read(voicePreferencesProvider.notifier);
    await notifier.ensureLoaded();
    final prefs = ref.read(voicePreferencesProvider);
    final sttKey = await notifier.getApiKey(prefs.stt.providerId);
    final ttsKey = await notifier.getApiKey(prefs.tts.providerId);
    if (!mounted) return;
    setState(() {
      _sttKey.text = sttKey ?? '';
      _ttsKey.text = ttsKey ?? '';
      _sttEndpoint.text = prefs.stt.endpointOverride;
      _sttModel.text = prefs.stt.modelOverride;
      _sttExtra.text = prefs.stt.extraParamsJson;
      _ttsEndpoint.text = prefs.tts.endpointOverride;
      _ttsModel.text = prefs.tts.modelOverride;
      _ttsVoice.text = prefs.tts.voice;
      _ttsExtra.text = prefs.tts.extraParamsJson;
      _loading = false;
    });
  }

  @override
  void dispose() {
    _sttKey.dispose();
    _ttsKey.dispose();
    _sttEndpoint.dispose();
    _sttModel.dispose();
    _sttExtra.dispose();
    _ttsEndpoint.dispose();
    _ttsModel.dispose();
    _ttsVoice.dispose();
    _ttsExtra.dispose();
    super.dispose();
  }

  Future<void> _previewVoice() async {
    if (_previewing) return;
    setState(() => _previewing = true);
    try {
      final provider = ref.read(ttsProviderResolver);
      await provider.stop();
      await provider.speak('Hello, this is Luna. How can I help you today?');
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Voice preview failed: $e')),
        );
      }
    } finally {
      if (mounted) setState(() => _previewing = false);
    }
  }

  Future<void> _testMicrophone() async {
    if (_testingMic) return;
    setState(() => _testingMic = true);
    final recorder = VoiceRecorderService();
    try {
      await recorder.start();
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Recording for 3 seconds…')),
        );
      }
      await Future<void>.delayed(const Duration(seconds: 3));
      final path = await recorder.stop();
      if (path == null) throw SttException('Recording failed');
      final text = await ref.read(sttServiceProvider).transcribeFile(path);
      if (!mounted) return;
      await showDialog<void>(
        context: context,
        builder: (ctx) => AlertDialog(
          title: const Text('Transcription'),
          content: Text(text),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(ctx),
              child: const Text('Close'),
            ),
          ],
        ),
      );
    } catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text('Microphone test failed: $e')),
        );
      }
    } finally {
      await recorder.dispose();
      if (mounted) setState(() => _testingMic = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final prefs = ref.watch(voicePreferencesProvider);
    final notifier = ref.read(voicePreferencesProvider.notifier);
    final sttProvider = VoiceCatalog.provider(prefs.stt.providerId);
    final sttModels = VoiceCatalog.sttModelsFor(prefs.stt.providerId);
    final sttPreset =
        VoiceCatalog.sttModel(prefs.stt.providerId, prefs.stt.modelId);
    final ttsProvider = VoiceCatalog.provider(prefs.tts.providerId);
    final ttsModels = VoiceCatalog.ttsModelsFor(prefs.tts.providerId);
    final ttsPreset =
        VoiceCatalog.ttsModel(prefs.tts.providerId, prefs.tts.modelId);

    final sttProviders = VoiceCatalog.providers
        .where((p) => p.sttProtocol != null)
        .toList();
    final ttsProviders =
        VoiceCatalog.providers.where((p) => p.ttsProtocol != null).toList();

    final currentLanguage = _languages.containsKey(prefs.language)
        ? prefs.language
        : (_languages.keys.first);

    if (_loading) {
      return const Scaffold(
        body: Center(child: CircularProgressIndicator()),
      );
    }

    return Scaffold(
      appBar: AppBar(title: const Text('Voice Mode')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          _sectionTitle('Input'),
          const SizedBox(height: 8),
          SegmentedButton<VoiceInputMode>(
            segments: const [
              ButtonSegment(
                value: VoiceInputMode.tap,
                label: Text('Tap'),
                icon: Icon(Icons.touch_app),
              ),
              ButtonSegment(
                value: VoiceInputMode.hold,
                label: Text('Hold'),
                icon: Icon(Icons.pan_tool_alt),
              ),
            ],
            selected: {prefs.inputMode},
            onSelectionChanged: (selected) {
              notifier.setInputMode(selected.first);
            },
          ),
          const SizedBox(height: 8),
          Text(
            prefs.inputMode == VoiceInputMode.tap
                ? 'Tap the microphone to start, tap again to stop.'
                : 'Press and hold the microphone; release to transcribe.',
            style: Theme.of(context).textTheme.bodySmall,
          ),
          const SizedBox(height: 16),
          DropdownButtonFormField<String>(
            value: currentLanguage,
            decoration: const InputDecoration(
              labelText: 'Language',
              helperText: 'Shared by speech recognition and synthesis',
              border: OutlineInputBorder(),
            ),
            isExpanded: true,
            items: _languages.entries
                .map(
                  (e) => DropdownMenuItem(
                    value: e.key,
                    child: Text(e.value),
                  ),
                )
                .toList(),
            onChanged: (value) {
              if (value != null) notifier.setLanguage(value);
            },
          ),
          const SizedBox(height: 24),
          const Divider(),
          _sectionTitle('Speech to text'),
          const SizedBox(height: 8),
          DropdownButtonFormField<String>(
            value: prefs.stt.providerId,
            decoration: const InputDecoration(
              labelText: 'Provider',
              border: OutlineInputBorder(),
            ),
            isExpanded: true,
            items: sttProviders
                .map(
                  (p) => DropdownMenuItem(
                    value: p.id,
                    child: Text(p.label),
                  ),
                )
                .toList(),
            onChanged: (value) async {
              if (value == null) return;
              await notifier.setSttProvider(value);
              await _reload();
            },
          ),
          const SizedBox(height: 12),
          DropdownButtonFormField<String>(
            value: sttModels.any((m) => m.id == prefs.stt.modelId)
                ? prefs.stt.modelId
                : sttModels.first.id,
            decoration: const InputDecoration(
              labelText: 'Model',
              border: OutlineInputBorder(),
            ),
            isExpanded: true,
            items: sttModels
                .map(
                  (m) => DropdownMenuItem(
                    value: m.id,
                    child: Text(m.label),
                  ),
                )
                .toList(),
            onChanged: (value) {
              if (value != null) notifier.setSttModel(value);
            },
          ),
          const SizedBox(height: 8),
          Text(
            'Max request: ${sttPreset.maxAudioSeconds}s · '
            'Vocabulary hints: ${sttPreset.supportsVocabularyHint ? 'supported' : 'not supported'}',
            style: Theme.of(context).textTheme.bodySmall,
          ),
          if (sttProvider.requiresApiKey) ...[
            const SizedBox(height: 12),
            TextField(
              controller: _sttKey,
              obscureText: _obscureSttKey,
              decoration: InputDecoration(
                labelText: '${sttProvider.label} API key',
                helperText: 'Stored in secure storage on this device',
                border: const OutlineInputBorder(),
                suffixIcon: IconButton(
                  icon: Icon(
                    _obscureSttKey ? Icons.visibility : Icons.visibility_off,
                  ),
                  onPressed: () =>
                      setState(() => _obscureSttKey = !_obscureSttKey),
                ),
              ),
              onChanged: (value) {
                unawaited(notifier.setApiKey(sttProvider.id, value));
              },
            ),
          ],
          ExpansionTile(
            title: const Text('Advanced'),
            tilePadding: EdgeInsets.zero,
            childrenPadding: const EdgeInsets.only(bottom: 8),
            children: [
              TextField(
                controller: _sttEndpoint,
                decoration: const InputDecoration(
                  labelText: 'Endpoint override',
                  border: OutlineInputBorder(),
                ),
                onChanged: (value) =>
                    notifier.setSttEndpointOverride(value),
              ),
              const SizedBox(height: 8),
              TextField(
                controller: _sttModel,
                decoration: const InputDecoration(
                  labelText: 'Model override',
                  border: OutlineInputBorder(),
                ),
                onChanged: (value) => notifier.setSttModelOverride(value),
              ),
              const SizedBox(height: 8),
              TextField(
                controller: _sttExtra,
                maxLines: 2,
                decoration: const InputDecoration(
                  labelText: 'Extra params (JSON)',
                  helperText: 'Merged into the request body',
                  border: OutlineInputBorder(),
                ),
                onChanged: (value) => notifier.setSttExtraParams(value),
              ),
            ],
          ),
          const SizedBox(height: 8),
          OutlinedButton.icon(
            onPressed: _testingMic ? null : _testMicrophone,
            icon: _testingMic
                ? const SizedBox(
                    width: 16,
                    height: 16,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Icon(Icons.mic),
            label: const Text('Test microphone (3s)'),
          ),
          const SizedBox(height: 24),
          const Divider(),
          _sectionTitle('Text to speech'),
          const SizedBox(height: 8),
          SwitchListTile(
            contentPadding: EdgeInsets.zero,
            title: const Text('Speak assistant replies'),
            value: prefs.ttsEnabled,
            onChanged: notifier.setTtsEnabled,
          ),
          DropdownButtonFormField<String>(
            value: prefs.tts.providerId,
            decoration: const InputDecoration(
              labelText: 'Provider',
              border: OutlineInputBorder(),
            ),
            isExpanded: true,
            items: ttsProviders
                .map(
                  (p) => DropdownMenuItem(
                    value: p.id,
                    child: Text(p.label),
                  ),
                )
                .toList(),
            onChanged: (value) async {
              if (value == null) return;
              await notifier.setTtsProvider(value);
              await _reload();
            },
          ),
          const SizedBox(height: 12),
          DropdownButtonFormField<String>(
            value: ttsModels.any((m) => m.id == prefs.tts.modelId)
                ? prefs.tts.modelId
                : ttsModels.first.id,
            decoration: const InputDecoration(
              labelText: 'Model',
              border: OutlineInputBorder(),
            ),
            isExpanded: true,
            items: ttsModels
                .map(
                  (m) => DropdownMenuItem(
                    value: m.id,
                    child: Text(m.label),
                  ),
                )
                .toList(),
            onChanged: (value) async {
              if (value == null) return;
              await notifier.setTtsModel(value);
              await _reload();
            },
          ),
          const SizedBox(height: 8),
          Text(
            'Max text per request: ${ttsPreset.maxChars} characters',
            style: Theme.of(context).textTheme.bodySmall,
          ),
          if (prefs.tts.providerId == 'qwen' &&
              VoicePreferencesNotifier.languageToQwenName(prefs.language) ==
                  'Auto') ...[
            const SizedBox(height: 6),
            Text(
              'Qwen TTS does not support this language. Choose another TTS provider for correct pronunciation.',
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
          ],
          if (!ttsProvider.isBuiltIn) ...[
            const SizedBox(height: 12),
            if (ttsPreset.voices.isNotEmpty) ...[
              DropdownButtonFormField<String>(
                value: ttsPreset.voices.contains(prefs.tts.voice)
                    ? prefs.tts.voice
                    : null,
                decoration: const InputDecoration(
                  labelText: 'Voice preset',
                  border: OutlineInputBorder(),
                ),
                isExpanded: true,
                items: ttsPreset.voices
                    .map(
                      (v) => DropdownMenuItem(
                        value: v,
                        child: Text(v),
                      ),
                    )
                    .toList(),
                onChanged: (value) {
                  if (value == null) return;
                  _ttsVoice.text = value;
                  notifier.setTtsVoice(value);
                },
              ),
              const SizedBox(height: 8),
            ],
            TextField(
              controller: _ttsVoice,
              decoration: const InputDecoration(
                labelText: 'Voice',
                helperText: 'Enter any voice ID supported by the provider',
                border: OutlineInputBorder(),
              ),
              onChanged: (value) => notifier.setTtsVoice(value),
            ),
            if (ttsProvider.requiresApiKey) ...[
              const SizedBox(height: 12),
              TextField(
                controller: _ttsKey,
                obscureText: _obscureTtsKey,
                decoration: InputDecoration(
                  labelText: '${ttsProvider.label} API key',
                  helperText: 'Stored in secure storage on this device',
                  border: const OutlineInputBorder(),
                  suffixIcon: IconButton(
                    icon: Icon(
                      _obscureTtsKey
                          ? Icons.visibility
                          : Icons.visibility_off,
                    ),
                    onPressed: () =>
                        setState(() => _obscureTtsKey = !_obscureTtsKey),
                  ),
                ),
                onChanged: (value) {
                  unawaited(notifier.setApiKey(ttsProvider.id, value));
                },
              ),
            ],
            ExpansionTile(
              title: const Text('Advanced'),
              tilePadding: EdgeInsets.zero,
              childrenPadding: const EdgeInsets.only(bottom: 8),
              children: [
                TextField(
                  controller: _ttsEndpoint,
                  decoration: const InputDecoration(
                    labelText: 'Endpoint override',
                    border: OutlineInputBorder(),
                  ),
                  onChanged: (value) =>
                      notifier.setTtsEndpointOverride(value),
                ),
                const SizedBox(height: 8),
                TextField(
                  controller: _ttsModel,
                  decoration: const InputDecoration(
                    labelText: 'Model override',
                    border: OutlineInputBorder(),
                  ),
                  onChanged: (value) => notifier.setTtsModelOverride(value),
                ),
                const SizedBox(height: 8),
                TextField(
                  controller: _ttsExtra,
                  maxLines: 2,
                  decoration: const InputDecoration(
                    labelText: 'Extra params (JSON)',
                    helperText: 'Merged into the request body',
                    border: OutlineInputBorder(),
                  ),
                  onChanged: (value) => notifier.setTtsExtraParams(value),
                ),
              ],
            ),
          ],
          const SizedBox(height: 8),
          OutlinedButton.icon(
            onPressed: _previewing ? null : _previewVoice,
            icon: _previewing
                ? const SizedBox(
                    width: 16,
                    height: 16,
                    child: CircularProgressIndicator(strokeWidth: 2),
                  )
                : const Icon(Icons.volume_up),
            label: const Text('Preview voice'),
          ),
          const SizedBox(height: 32),
        ],
      ),
    );
  }

  Widget _sectionTitle(String text) {
    return Text(
      text,
      style: Theme.of(context)
          .textTheme
          .titleMedium
          ?.copyWith(fontWeight: FontWeight.w600),
    );
  }
}
