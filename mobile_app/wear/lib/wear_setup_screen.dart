import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:luna_mobile/application/app_controller.dart';
import 'package:luna_mobile/application/app_state.dart';
import 'package:luna_mobile/core/config/server_config.dart';
import 'package:luna_mobile/core/config/stt_preferences.dart';
import 'package:luna_mobile/services/tts_service.dart';

class WearSetupScreen extends ConsumerStatefulWidget {
  const WearSetupScreen({super.key});

  @override
  ConsumerState<WearSetupScreen> createState() => _WearSetupScreenState();
}

class _WearSetupScreenState extends ConsumerState<WearSetupScreen> {
  final _hostController = TextEditingController();
  final _portController = TextEditingController();
  final _apiKeyController = TextEditingController();
  final _profileController = TextEditingController();
  List<dynamic> _availableLanguages = [];
  bool _loadingLanguages = true;

  @override
  void initState() {
    super.initState();
    final config = ref.read(serverConfigProvider);
    _hostController.text = config.host;
    _portController.text = config.port.toString();
    _apiKeyController.text = config.apiKey;
    _profileController.text = config.profile;
    _loadAvailableLanguages();
  }

  Future<void> _loadAvailableLanguages() async {
    final ttsService = ref.read(ttsServiceProvider);
    try {
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
  void dispose() {
    _hostController.dispose();
    _portController.dispose();
    _apiKeyController.dispose();
    _profileController.dispose();
    super.dispose();
  }

  void _saveAndConnect() {
    final host = _hostController.text.trim();
    final port = int.tryParse(_portController.text.trim()) ?? 8080;
    final apiKey = _apiKeyController.text.trim();
    final profile = _profileController.text.trim();

    if (host.isEmpty) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Host is required')),
      );
      return;
    }

    final config = ServerConfig(
      host: host,
      port: port,
      apiKey: apiKey.isEmpty ? 'LUna' : apiKey,
      profile: profile.isEmpty ? 'default' : profile,
    );

    final notifier = ref.read(serverConfigProvider.notifier);
    notifier.updateHost(config.host);
    notifier.updatePort(config.port);
    notifier.updateApiKey(config.apiKey);
    notifier.updateProfile(config.profile);
    ref.read(appControllerProvider.notifier).checkAndReconnect();
  }

  String _getLanguageDisplayName(String languageCode) {
    final parts = languageCode.split('-');
    final lang = parts[0];
    final country = parts.length > 1 ? parts[1] : null;

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
      'uk': 'Ukrainian',
      'sv': 'Swedish',
      'da': 'Danish',
      'fi': 'Finnish',
      'no': 'Norwegian',
      'cs': 'Czech',
      'hu': 'Hungarian',
    };

    final langName = languageNames[lang] ?? lang.toUpperCase();
    if (country != null) {
      return '$langName ($country)';
    }
    return langName;
  }

  void _showLanguageSelector() {
    final sttPrefs = ref.read(sttPreferencesProvider);
    
    showModalBottomSheet(
      context: context,
      isScrollControlled: true,
      backgroundColor: Theme.of(context).colorScheme.surface,
      shape: const RoundedRectangleBorder(
        borderRadius: BorderRadius.vertical(top: Radius.circular(16)),
      ),
      builder: (context) => Consumer(
        builder: (context, ref, _) {
          final currentPrefs = ref.watch(sttPreferencesProvider);
          
          // Sort languages: favorites first
          final sortedLanguages = List<dynamic>.from(_availableLanguages);
          sortedLanguages.sort((a, b) {
            final aCode = a.toString();
            final bCode = b.toString();
            final aFav = currentPrefs.favoriteLanguages.contains(aCode);
            final bFav = currentPrefs.favoriteLanguages.contains(bCode);
            if (aFav && !bFav) return -1;
            if (!aFav && bFav) return 1;
            return _getLanguageDisplayName(aCode)
                .compareTo(_getLanguageDisplayName(bCode));
          });
          
          return DraggableScrollableSheet(
            initialChildSize: 0.7,
            minChildSize: 0.5,
            maxChildSize: 0.9,
            expand: false,
            builder: (context, scrollController) => Column(
              children: [
                Container(
                  padding: const EdgeInsets.all(12),
                  child: Row(
                    children: [
                      const Icon(Icons.star, size: 18),
                      const SizedBox(width: 8),
                      const Text(
                        'Favorite Languages',
                        style: TextStyle(fontSize: 14, fontWeight: FontWeight.bold),
                      ),
                      const Spacer(),
                      Text(
                        '${currentPrefs.favoriteLanguages.length} selected',
                        style: TextStyle(
                          fontSize: 12,
                          color: Theme.of(context).colorScheme.outline,
                        ),
                      ),
                    ],
                  ),
                ),
                const Divider(height: 1),
                Expanded(
                  child: ListView.builder(
                    controller: scrollController,
                    itemCount: sortedLanguages.length,
                    itemBuilder: (context, index) {
                      final langCode = sortedLanguages[index].toString();
                      final displayName = _getLanguageDisplayName(langCode);
                      final isFavorite = currentPrefs.favoriteLanguages.contains(langCode);
                      final isOnlyFavorite = currentPrefs.favoriteLanguages.length == 1 && isFavorite;
                      
                      return ListTile(
                        leading: Icon(
                          isFavorite ? Icons.star : Icons.star_border,
                          size: 18,
                          color: isFavorite
                              ? Theme.of(context).colorScheme.primary
                              : Theme.of(context).colorScheme.outline,
                        ),
                        title: Text(displayName, style: const TextStyle(fontSize: 12)),
                        subtitle: Text(langCode, style: const TextStyle(fontSize: 10)),
                        dense: true,
                        onTap: () {
                          if (isFavorite) {
                            if (!isOnlyFavorite) {
                              ref.read(sttPreferencesProvider.notifier)
                                  .removeFavoriteLanguage(langCode);
                            } else {
                              ScaffoldMessenger.of(context).showSnackBar(
                                const SnackBar(
                                  content: Text('At least one favorite is required'),
                                  duration: Duration(seconds: 2),
                                ),
                              );
                            }
                          } else {
                            ref.read(sttPreferencesProvider.notifier)
                                .addFavoriteLanguage(langCode);
                          }
                        },
                      );
                    },
                  ),
                ),
                SafeArea(
                  child: Padding(
                    padding: const EdgeInsets.all(8),
                    child: FilledButton(
                      onPressed: () => Navigator.pop(context),
                      child: const Text('Done', style: TextStyle(fontSize: 12)),
                    ),
                  ),
                ),
              ],
            ),
          );
        },
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final sttPrefs = ref.watch(sttPreferencesProvider);
    final state = ref.watch(appControllerProvider);
    
    return Scaffold(
      body: ListView(
        padding: const EdgeInsets.fromLTRB(12, 32, 12, 16),
        children: [
          // Header
          const Center(
            child: Text(
              'Setup',
              style: TextStyle(fontSize: 16, fontWeight: FontWeight.bold),
            ),
          ),
          const SizedBox(height: 16),
          
          // Server settings
          TextField(
            controller: _hostController,
            decoration: const InputDecoration(
              labelText: 'Host',
              hintText: 'example.com',
              isDense: true,
            ),
            style: const TextStyle(fontSize: 12),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _portController,
            decoration: const InputDecoration(
              labelText: 'Port',
              hintText: '8080',
              isDense: true,
            ),
            keyboardType: TextInputType.number,
            style: const TextStyle(fontSize: 12),
          ),
          const SizedBox(height: 8),
          TextField(
            controller: _apiKeyController,
            decoration: const InputDecoration(
              labelText: 'API Key',
              hintText: 'LUna',
              isDense: true,
            ),
            style: const TextStyle(fontSize: 12),
          ),
          const SizedBox(height: 16),
          
          // Favorite Languages
          ListTile(
            contentPadding: EdgeInsets.zero,
            leading: const Icon(Icons.language, size: 20),
            title: const Text('Favorite Languages', style: TextStyle(fontSize: 12)),
            subtitle: Text(
              sttPrefs.favoriteLanguages
                  .take(3)
                  .map(_getLanguageDisplayName)
                  .join(', ') +
                  (sttPrefs.favoriteLanguages.length > 3 
                      ? ' +${sttPrefs.favoriteLanguages.length - 3}' 
                      : ''),
              style: TextStyle(
                fontSize: 10,
                color: Theme.of(context).colorScheme.outline,
              ),
            ),
            trailing: const Icon(Icons.chevron_right, size: 18),
            dense: true,
            onTap: _loadingLanguages ? null : _showLanguageSelector,
          ),
          
          const SizedBox(height: 16),
          
          // Connect button
          FilledButton(
            onPressed: _saveAndConnect,
            child: const Text('Connect', style: TextStyle(fontSize: 12)),
          ),
          
          const SizedBox(height: 8),
          
          // Back to chat if connected
          if (state.connection == ConnectionStatus.online)
            OutlinedButton(
              onPressed: () {
                if (state.activeConversation != null) {
                  ref.read(appControllerProvider.notifier)
                      .selectConversation(state.activeConversation!.id);
                } else {
                  ref.read(appControllerProvider.notifier).startNewConversation();
                }
              },
              child: const Text('Back to Chat', style: TextStyle(fontSize: 12)),
            ),
        ],
      ),
    );
  }
}
