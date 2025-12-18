import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/config/stt_preferences.dart';

/// Dialog for managing favorite languages with search
class LanguageFavoritesDialog extends ConsumerStatefulWidget {
  final List<dynamic> availableLanguages;

  const LanguageFavoritesDialog({
    super.key,
    required this.availableLanguages,
  });

  @override
  ConsumerState<LanguageFavoritesDialog> createState() =>
      _LanguageFavoritesDialogState();

  /// Show the dialog and return when closed
  static Future<void> show(
    BuildContext context,
    List<dynamic> availableLanguages,
  ) async {
    await showDialog<void>(
      context: context,
      builder: (context) => LanguageFavoritesDialog(
        availableLanguages: availableLanguages,
      ),
    );
  }
}

class _LanguageFavoritesDialogState
    extends ConsumerState<LanguageFavoritesDialog> {
  String _searchQuery = '';
  final TextEditingController _searchController = TextEditingController();

  @override
  void dispose() {
    _searchController.dispose();
    super.dispose();
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
      'uk': 'Ukrainian',
      'id': 'Indonesian',
      'ms': 'Malay',
      'tl': 'Filipino',
      'bn': 'Bengali',
      'ta': 'Tamil',
      'te': 'Telugu',
      'mr': 'Marathi',
      'gu': 'Gujarati',
      'kn': 'Kannada',
      'ml': 'Malayalam',
      'pa': 'Punjabi',
      'ur': 'Urdu',
      'fa': 'Persian',
      'sw': 'Swahili',
      'af': 'Afrikaans',
      'ca': 'Catalan',
      'hr': 'Croatian',
      'sk': 'Slovak',
      'sl': 'Slovenian',
      'bg': 'Bulgarian',
      'sr': 'Serbian',
      'lt': 'Lithuanian',
      'lv': 'Latvian',
      'et': 'Estonian',
    };

    final langName = languageNames[lang] ?? lang.toUpperCase();
    if (country != null) {
      return '$langName ($country)';
    }
    return langName;
  }

  @override
  Widget build(BuildContext context) {
    final sttPrefs = ref.watch(sttPreferencesProvider);
    final sttPrefsNotifier = ref.read(sttPreferencesProvider.notifier);
    final theme = Theme.of(context);

    // Filter languages based on search
    final filteredLanguages = widget.availableLanguages.where((lang) {
      final langCode = lang.toString();
      final displayName = _getLanguageDisplayName(langCode).toLowerCase();
      return displayName.contains(_searchQuery.toLowerCase()) ||
          langCode.toLowerCase().contains(_searchQuery.toLowerCase());
    }).toList();

    // Sort: favorites first, then alphabetically
    filteredLanguages.sort((a, b) {
      final aCode = a.toString();
      final bCode = b.toString();
      final aFav = sttPrefs.favoriteLanguages.contains(aCode);
      final bFav = sttPrefs.favoriteLanguages.contains(bCode);
      if (aFav && !bFav) return -1;
      if (!aFav && bFav) return 1;
      return _getLanguageDisplayName(aCode)
          .compareTo(_getLanguageDisplayName(bCode));
    });

    return Dialog(
      child: ConstrainedBox(
        constraints: const BoxConstraints(
          maxWidth: 400,
          maxHeight: 500,
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            // Header
            Container(
              padding: const EdgeInsets.all(16),
              decoration: BoxDecoration(
                color: theme.colorScheme.primaryContainer,
                borderRadius: const BorderRadius.only(
                  topLeft: Radius.circular(28),
                  topRight: Radius.circular(28),
                ),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Icon(
                        Icons.star,
                        color: theme.colorScheme.onPrimaryContainer,
                      ),
                      const SizedBox(width: 8),
                      Text(
                        'Favorite Languages',
                        style: theme.textTheme.titleLarge?.copyWith(
                          color: theme.colorScheme.onPrimaryContainer,
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 4),
                  Text(
                    'Select languages to show in the quick menu',
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.onPrimaryContainer
                          .withOpacity(0.7),
                    ),
                  ),
                ],
              ),
            ),
            // Search bar
            Padding(
              padding: const EdgeInsets.all(16),
              child: TextField(
                controller: _searchController,
                decoration: InputDecoration(
                  hintText: 'Search languages...',
                  prefixIcon: const Icon(Icons.search),
                  suffixIcon: _searchQuery.isNotEmpty
                      ? IconButton(
                          icon: const Icon(Icons.clear),
                          onPressed: () {
                            _searchController.clear();
                            setState(() => _searchQuery = '');
                          },
                        )
                      : null,
                  border: OutlineInputBorder(
                    borderRadius: BorderRadius.circular(12),
                  ),
                  isDense: true,
                ),
                onChanged: (value) => setState(() => _searchQuery = value),
              ),
            ),
            // Language list
            Flexible(
              child: ListView.builder(
                shrinkWrap: true,
                itemCount: filteredLanguages.length,
                itemBuilder: (context, index) {
                  final langCode = filteredLanguages[index].toString();
                  final displayName = _getLanguageDisplayName(langCode);
                  final isFavorite =
                      sttPrefs.favoriteLanguages.contains(langCode);
                  final isOnlyFavorite =
                      sttPrefs.favoriteLanguages.length == 1 && isFavorite;

                  return ListTile(
                    leading: Icon(
                      isFavorite ? Icons.star : Icons.star_border,
                      color: isFavorite
                          ? theme.colorScheme.primary
                          : theme.colorScheme.outline,
                    ),
                    title: Text(displayName),
                    subtitle: Text(
                      langCode,
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.colorScheme.outline,
                      ),
                    ),
                    trailing: isFavorite
                        ? Icon(
                            Icons.check_circle,
                            color: theme.colorScheme.primary,
                          )
                        : null,
                    onTap: () {
                      if (isFavorite) {
                        if (!isOnlyFavorite) {
                          sttPrefsNotifier.removeFavoriteLanguage(langCode);
                        } else {
                          ScaffoldMessenger.of(context).showSnackBar(
                            const SnackBar(
                              content:
                                  Text('At least one favorite is required'),
                              duration: Duration(seconds: 2),
                            ),
                          );
                        }
                      } else {
                        sttPrefsNotifier.addFavoriteLanguage(langCode);
                      }
                    },
                  );
                },
              ),
            ),
            // Footer
            Container(
              padding: const EdgeInsets.all(16),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  Text(
                    '${sttPrefs.favoriteLanguages.length} selected',
                    style: theme.textTheme.bodySmall?.copyWith(
                      color: theme.colorScheme.outline,
                    ),
                  ),
                  FilledButton(
                    onPressed: () => Navigator.of(context).pop(),
                    child: const Text('Done'),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}



