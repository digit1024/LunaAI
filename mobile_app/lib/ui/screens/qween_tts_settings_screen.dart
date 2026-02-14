import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/config/qween_tts_preferences.dart';

class QweenTtsSettingsScreen extends ConsumerStatefulWidget {
  const QweenTtsSettingsScreen({super.key});

  @override
  ConsumerState<QweenTtsSettingsScreen> createState() =>
      _QweenTtsSettingsScreenState();
}

class _QweenTtsSettingsScreenState extends ConsumerState<QweenTtsSettingsScreen> {
  late final TextEditingController _apiKeyController;
  bool _obscureApiKey = true;
  bool _loadingApiKey = true;
  String? _error;

  @override
  void initState() {
    super.initState();
    _apiKeyController = TextEditingController();
    _loadSettings();
  }

  Future<void> _loadSettings() async {
    setState(() {
      _loadingApiKey = true;
      _error = null;
    });
    try {
      final apiKey = await ref.read(qweenTtsPreferencesProvider.notifier).getApiKey();

      if (mounted) {
        _apiKeyController.text = apiKey ?? '';
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

  Future<void> _saveApiKey(String value) async {
    try {
      await ref.read(qweenTtsPreferencesProvider.notifier).setApiKey(value);
    } catch (e) {
      if (mounted) {
        setState(() => _error = 'Failed to save API key');
      }
    }
  }

  @override
  void dispose() {
    final apiKey = _apiKeyController.text.trim();
    if (apiKey.isNotEmpty) {
      ref.read(qweenTtsPreferencesProvider.notifier).setApiKey(apiKey);
    }
    _apiKeyController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final qweenPrefs = ref.watch(qweenTtsPreferencesProvider);
    final qweenNotifier = ref.read(qweenTtsPreferencesProvider.notifier);

    return Scaffold(
      appBar: AppBar(
        title: const Text('Qween TTS Settings'),
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
                          Icon(Icons.error_outline,
                              color: Theme.of(context).colorScheme.error),
                          const SizedBox(width: 8),
                          Expanded(
                            child: Text(_error!,
                                style: TextStyle(
                                    color: Theme.of(context)
                                        .colorScheme
                                        .onErrorContainer)),
                          ),
                        ],
                      ),
                    ),
                    const SizedBox(height: 16),
                  ],

                  // API Key
                  TextField(
                    controller: _apiKeyController,
                    obscureText: _obscureApiKey,
                    decoration: InputDecoration(
                      labelText: 'API Key',
                      helperText: 'DashScope API key. Stored securely on device.',
                      border: const OutlineInputBorder(),
                      suffixIcon: IconButton(
                        icon: Icon(_obscureApiKey
                            ? Icons.visibility
                            : Icons.visibility_off),
                        onPressed: () {
                          setState(() => _obscureApiKey = !_obscureApiKey);
                        },
                      ),
                    ),
                    onChanged: (v) {
                      if (v.trim().isNotEmpty) _saveApiKey(v);
                    },
                  ),
                  const SizedBox(height: 20),

                  // Voice
                  DropdownButtonFormField<String>(
                    value: kQwenSupportedVoices.contains(qweenPrefs.voice)
                        ? qweenPrefs.voice
                        : kQwenSupportedVoices.first,
                    decoration: const InputDecoration(
                      labelText: 'Voice',
                      border: OutlineInputBorder(),
                    ),
                    items: kQwenSupportedVoices
                        .map((v) => DropdownMenuItem(value: v, child: Text(v)))
                        .toList(),
                    onChanged: (value) {
                      if (value != null) qweenNotifier.setVoice(value);
                    },
                  ),
                  const SizedBox(height: 8),
                  Text(
                    'Model: qwen3-tts-flash',
                    style: Theme.of(context).textTheme.bodySmall,
                  ),
                ],
              ),
            ),
    );
  }
}
