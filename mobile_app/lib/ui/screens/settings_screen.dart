import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../application/app_controller.dart';
import '../../core/config/tts_preferences.dart';
import '../../services/tts_service.dart';

class SettingsScreen extends ConsumerStatefulWidget {
  const SettingsScreen({super.key});

  @override
  ConsumerState<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends ConsumerState<SettingsScreen> {
  List<dynamic>? _availableLanguages;
  bool _loadingLanguages = false;

  @override
  void initState() {
    super.initState();
    _loadLanguages();
  }

  Future<void> _loadLanguages() async {
    setState(() {
      _loadingLanguages = true;
    });
    try {
      final ttsService = ref.read(ttsServiceProvider);
      final languages = await ttsService.getLanguages();
      if (mounted) {
        setState(() {
          _availableLanguages = languages;
          _loadingLanguages = false;
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _loadingLanguages = false;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final ttsPrefs = ref.watch(ttsPreferencesProvider);
    final ttsPrefsNotifier = ref.read(ttsPreferencesProvider.notifier);
    final appController = ref.read(appControllerProvider.notifier);

    return PopScope(
      canPop: false,
      onPopInvoked: (didPop) {
        if (!didPop) {
          appController.openConversations();
        }
      },
      child: Scaffold(
        appBar: AppBar(
          title: const Text('Settings'),
          leading: IconButton(
            icon: const Icon(Icons.arrow_back),
            onPressed: () => appController.openConversations(),
          ),
        ),
        body: SingleChildScrollView(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // TTS Section
            Text(
              'Text-to-Speech',
              style: Theme.of(context).textTheme.titleLarge?.copyWith(
                    fontWeight: FontWeight.bold,
                  ),
            ),
            const SizedBox(height: 16),
            Card(
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    // TTS Enable/Disable Toggle
                    SwitchListTile(
                      title: const Text('Enable TTS'),
                      subtitle: const Text(
                        'Read the last assistant message aloud after receiving it',
                      ),
                      value: ttsPrefs.enabled,
                      onChanged: (value) {
                        ttsPrefsNotifier.setEnabled(value);
                      },
                    ),
                    const Divider(),
                    // Language Picker
                    if (ttsPrefs.enabled) ...[
                      const SizedBox(height: 8),
                      Text(
                        'Language',
                        style: Theme.of(context).textTheme.titleMedium,
                      ),
                      const SizedBox(height: 8),
                      if (_loadingLanguages)
                        const Center(
                          child: Padding(
                            padding: EdgeInsets.all(16),
                            child: CircularProgressIndicator(),
                          ),
                        )
                      else if (_availableLanguages != null &&
                          _availableLanguages!.isNotEmpty)
                        DropdownButtonFormField<String>(
                          value: ttsPrefs.language,
                          decoration: const InputDecoration(
                            border: OutlineInputBorder(),
                            hintText: 'Select language',
                          ),
                          items: _availableLanguages!
                              .map<DropdownMenuItem<String>>((lang) {
                            final langCode = lang.toString();
                            final displayName = _getLanguageDisplayName(langCode);
                            return DropdownMenuItem<String>(
                              value: langCode,
                              child: Text(displayName),
                            );
                          }).toList(),
                          onChanged: (value) {
                            if (value != null) {
                              ttsPrefsNotifier.setLanguage(value);
                              // Update TTS service language
                              ref.read(ttsServiceProvider).setLanguage(value);
                            }
                          },
                        )
                      else
                        TextButton.icon(
                          onPressed: _loadLanguages,
                          icon: const Icon(Icons.refresh),
                          label: const Text('Load Languages'),
                        ),
                    ],
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    ),
    );
  }

  String _getLanguageDisplayName(String languageCode) {
    // Extract language name from code (e.g., "en-US" -> "English (US)")
    final parts = languageCode.split('-');
    final lang = parts[0];
    final country = parts.length > 1 ? parts[1] : null;

    // Common language names
    final languageNames = {
      'en': 'English',
      'es': 'Spanish',
      'fr': 'French',
      'de': 'German',
      'it': 'Italian',
      'pt': 'Portuguese',
      'ru': 'Russian',
      'ja': 'Japanese',
      'ko': 'Korean',
      'zh': 'Chinese',
      'ar': 'Arabic',
      'hi': 'Hindi',
      'nl': 'Dutch',
      'pl': 'Polish',
      'tr': 'Turkish',
      'sv': 'Swedish',
      'da': 'Danish',
      'fi': 'Finnish',
      'no': 'Norwegian',
      'cs': 'Czech',
      'hu': 'Hungarian',
      'ro': 'Romanian',
      'el': 'Greek',
      'he': 'Hebrew',
      'th': 'Thai',
      'vi': 'Vietnamese',
    };

    final langName = languageNames[lang] ?? lang.toUpperCase();
    if (country != null) {
      return '$langName ($country)';
    }
    return langName;
  }
}
