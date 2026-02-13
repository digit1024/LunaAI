import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/config/qween_tts_preferences.dart';
import '../../core/config/tts_provider_type.dart';
import '../../core/config/tts_preferences.dart';

class QweenTtsSettingsScreen extends ConsumerStatefulWidget {
  const QweenTtsSettingsScreen({super.key});

  @override
  ConsumerState<QweenTtsSettingsScreen> createState() =>
      _QweenTtsSettingsScreenState();
}

class _QweenTtsSettingsScreenState extends ConsumerState<QweenTtsSettingsScreen> {
  late final TextEditingController _apiKeyController;
  late final TextEditingController _instructionsController;
  bool _obscureApiKey = true;
  bool _loadingApiKey = true;
  bool _saving = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    _apiKeyController = TextEditingController();
    _instructionsController = TextEditingController(
      text: kQweenDefaultInstructions,
    );
    _loadSettings();
  }

  Future<void> _loadSettings() async {
    setState(() {
      _loadingApiKey = true;
      _error = null;
    });
    try {
      final qweenPrefs = ref.read(qweenTtsPreferencesProvider);
      final apiKey = await ref.read(qweenTtsPreferencesProvider.notifier).getApiKey();

      if (mounted) {
        _apiKeyController.text = apiKey ?? '';
        _instructionsController.text = qweenPrefs.instructions;
        setState(() => _loadingApiKey = false);
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _loadingApiKey = false;
          _error = e.toString();
        });
      }
    }
  }

  Future<void> _save() async {
    setState(() {
      _saving = true;
      _error = null;
    });

    final apiKey = _apiKeyController.text.trim();
    if (apiKey.isEmpty) {
      setState(() {
        _saving = false;
        _error = 'API Key is required';
      });
      return;
    }

    try {
      final notifier = ref.read(qweenTtsPreferencesProvider.notifier);
      await notifier.setApiKey(apiKey);
      await notifier.setInstructions(_instructionsController.text.trim());
      await notifier.setVoice(
        ref.read(qweenTtsPreferencesProvider).voice,
      );

      if (mounted) {
        setState(() => _saving = false);
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Qween TTS settings saved')),
        );
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _saving = false;
          _error = e.toString();
        });
      }
    }
  }

  @override
  void dispose() {
    _apiKeyController.dispose();
    _instructionsController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final qweenPrefs = ref.watch(qweenTtsPreferencesProvider);
    final qweenNotifier = ref.read(qweenTtsPreferencesProvider.notifier);
    final ttsNotifier = ref.read(ttsPreferencesProvider.notifier);

    return Scaffold(
      appBar: AppBar(
        title: const Text('Qween TTS Settings'),
        leading: IconButton(
          icon: const Icon(Icons.arrow_back),
          onPressed: () => Navigator.of(context).pop(),
        ),
      ),
      body: _loadingApiKey
          ? const Center(child: CircularProgressIndicator())
          : SingleChildScrollView(
              padding: const EdgeInsets.all(24),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  if (_error != null) ...[
                    Container(
                      padding: const EdgeInsets.all(12),
                      decoration: BoxDecoration(
                        color: Theme.of(context).colorScheme.errorContainer,
                        borderRadius: BorderRadius.circular(8),
                      ),
                      child: Row(
                        children: [
                          Icon(
                            Icons.error_outline,
                            color: Theme.of(context).colorScheme.error,
                          ),
                          const SizedBox(width: 8),
                          Expanded(
                            child: Text(
                              _error!,
                              style: TextStyle(
                                color: Theme.of(context).colorScheme.onErrorContainer,
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                    const SizedBox(height: 16),
                  ],
                  Text(
                    'Configure Alibaba Qwen TTS. API Key is stored securely.',
                    style: Theme.of(context).textTheme.bodyMedium,
                  ),
                  const SizedBox(height: 24),
                  TextField(
                    controller: _apiKeyController,
                    obscureText: _obscureApiKey,
                    decoration: InputDecoration(
                      labelText: 'API Key (required)',
                      hintText: 'DASHSCOPE_API_KEY',
                      border: const OutlineInputBorder(),
                      suffixIcon: IconButton(
                        icon: Icon(
                          _obscureApiKey ? Icons.visibility : Icons.visibility_off,
                        ),
                        onPressed: () {
                          setState(() => _obscureApiKey = !_obscureApiKey);
                        },
                      ),
                    ),
                  ),
                  const SizedBox(height: 16),
                  DropdownButtonFormField<String>(
                    value: qweenPrefs.voice,
                    decoration: const InputDecoration(
                      labelText: 'Voice',
                      border: OutlineInputBorder(),
                    ),
                    items: kQwenSupportedVoices
                        .map((v) => DropdownMenuItem(value: v, child: Text(v)))
                        .toList(),
                    onChanged: (value) {
                      if (value != null) {
                        qweenNotifier.setVoice(value);
                      }
                    },
                  ),
                  const SizedBox(height: 16),
                  TextField(
                    controller: _instructionsController,
                    maxLines: 4,
                    decoration: const InputDecoration(
                      labelText: 'Instructions (optional)',
                      hintText: kQweenDefaultInstructions,
                      border: OutlineInputBorder(),
                      alignLabelWithHint: true,
                    ),
                  ),
                  const SizedBox(height: 8),
                  Text(
                    'Controls expressiveness. Only for qwen3-tts-instruct-flash.',
                    style: Theme.of(context).textTheme.bodySmall,
                  ),
                  const SizedBox(height: 32),
                  FilledButton(
                    onPressed: _saving ? null : _save,
                    child: _saving
                        ? const SizedBox(
                            height: 20,
                            width: 20,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : const Text('Save'),
                  ),
                  const SizedBox(height: 16),
                  OutlinedButton(
                    onPressed: () {
                      ttsNotifier.setProviderType(TtsProviderType.qween);
                      Navigator.of(context).pop();
                    },
                    child: const Text('Use Qween TTS'),
                  ),
                ],
              ),
            ),
    );
  }
}
