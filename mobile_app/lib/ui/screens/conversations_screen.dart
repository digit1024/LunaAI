import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../application/app_controller.dart';
import '../../application/app_state.dart';
import '../../core/config/server_config.dart';
import '../../core/config/tts_preferences.dart';
import '../../core/config/stt_preferences.dart';
import '../../services/tts_service.dart';
import '../../data/ws/ws_dto.dart';
import '../widgets/conversation_card.dart';
import '../widgets/rename_conversation_dialog.dart';

class ConversationsScreen extends ConsumerStatefulWidget {
  const ConversationsScreen({super.key});

  @override
  ConsumerState<ConversationsScreen> createState() => _ConversationsScreenState();
}

class _ConversationsScreenState extends ConsumerState<ConversationsScreen> {
  final ScrollController _scrollController = ScrollController();
  bool _isLoadingMore = false;
  bool _hasMore = true;
  int _currentOffset = 0;
  int _previousConversationCount = 0;

  @override
  void initState() {
    super.initState();
    _scrollController.addListener(_onScroll);
    // Load initial conversations
    WidgetsBinding.instance.addPostFrameCallback((_) {
      ref.read(appControllerProvider.notifier).refreshConversations();
    });
  }

  @override
  void dispose() {
    _scrollController.dispose();
    super.dispose();
  }

  void _onScroll() {
    if (_scrollController.position.pixels >=
            _scrollController.position.maxScrollExtent * 0.8 &&
        !_isLoadingMore &&
        _hasMore) {
      _loadMore();
    }
  }

  void _loadMore() {
    if (_isLoadingMore || !_hasMore) return;
    setState(() {
      _isLoadingMore = true;
      _previousConversationCount = ref.read(appControllerProvider).conversations.length;
    });
    final nextOffset = _currentOffset + 10;
    ref.read(appControllerProvider.notifier).loadMoreConversations(nextOffset);
    // Reset loading state after a delay (will be updated when new data arrives)
    Future.delayed(const Duration(seconds: 1), () {
      if (mounted) {
        setState(() {
          _isLoadingMore = false;
        });
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final state = ref.watch(appControllerProvider);
    final controller = ref.read(appControllerProvider.notifier);
    final config = ref.watch(serverConfigProvider);

    ref.listen<String?>(
      appControllerProvider.select((s) => s.infoMessage),
      (_, infoMessage) {
        if (infoMessage != null && infoMessage.isNotEmpty) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text(infoMessage)),
          );
          controller.clearInfoMessage();
        }
      },
    );

    // Update offset and hasMore when conversations change
    final currentCount = state.conversations.length;
    
    // Check if conversations increased (pagination) or changed (refresh)
    if (currentCount > _previousConversationCount) {
      // Conversations increased - this could be pagination
      final addedCount = currentCount - _previousConversationCount;
      
      // If count exceeds current offset, it's pagination - update offset
      if (currentCount > _currentOffset) {
        _currentOffset = currentCount;
        // If we got fewer than 10 items, we've reached the end
        if (addedCount < 10) {
          _hasMore = false;
        }
      }
    } else if (currentCount != _previousConversationCount && _previousConversationCount > 0) {
      // Count changed but didn't increase - this was a refresh
      // Reset offset to current count
      _currentOffset = currentCount;
      // Re-enable hasMore if we have 10 or more items
      if (currentCount >= 10) {
        _hasMore = true;
      } else {
        _hasMore = false;
      }
    } else if (_currentOffset == 0 && currentCount < 10 && currentCount > 0) {
      // Initial load with fewer than 10 items
      _hasMore = false;
    }
    
    // Update previous count for next comparison
    _previousConversationCount = currentCount;

    return Scaffold(
      drawer: _ChatDrawer(
        profile: state.currentProfile.isNotEmpty ? state.currentProfile : config.profile,
        availableProfiles: state.availableProfiles,
        onProfileChanged: (newProfile) {
          controller.changeProfile(newProfile);
        },
        onStartNew: controller.startNewConversation,
        onHistory: controller.openConversations,
        onMemories: controller.openMemories,
        onMcpServers: controller.openMcpServers,
        onSetup: controller.openSetup,
      ),
      body: Column(
        children: [
          _Header(
            status: state.connection,
            onMemories: controller.openMemories,
            onMcpServers: controller.openMcpServers,
            onRefresh: () {
              setState(() {
                _currentOffset = 0;
                _hasMore = true;
                _isLoadingMore = false;
                _previousConversationCount = 0;
              });
              controller.refreshConversations();
            },
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: Row(
              children: [
                Expanded(
                  child: TextField(
                    decoration: const InputDecoration(
                      prefixIcon: Icon(Icons.search),
                      hintText: 'Search history…',
                    ),
                    onChanged: controller.search,
                  ),
                ),
                TextButton(
                  onPressed: () {
                    setState(() {
                      _currentOffset = 0;
                      _hasMore = true;
                      _isLoadingMore = false;
                      _previousConversationCount = 0;
                    });
                    controller.toggleShowInternal();
                  },
                  child: Text(state.showInternal ? 'Hide transient' : 'Show transient'),
                ),
              ],
            ),
          ),
          Expanded(
            child: _items(state).isEmpty
                ? (_isSearchActive(state)
                    ? const _SearchEmptyPlaceholder()
                    : const _EmptyPlaceholder())
                : ListView.separated(
                    controller: _scrollController,
                    itemCount: _items(state).length + (_hasMore && !state.searchQuery.isNotEmpty ? 1 : 0),
                    separatorBuilder: (_, __) => const Divider(height: 0),
                    itemBuilder: (context, index) {
                      if (index >= _items(state).length) {
                        return const Center(
                          child: Padding(
                            padding: EdgeInsets.all(16.0),
                            child: CircularProgressIndicator(),
                          ),
                        );
                      }
                      final entry = _items(state)[index];
                      return entry.map(
                        summary: (summary) {
                          final selected =
                              state.activeConversation?.id == summary.id;
                          return ConversationCard(
                            summary: summary,
                            isSelected: selected,
                            onTap: () => controller.selectConversation(summary.id),
                            onEdit: () => showRenameConversationDialog(
                              context: context,
                              ref: ref,
                              conversationId: summary.id,
                              currentTitle: summary.title,
                            ),
                            onDelete: () => controller.deleteConversation(summary.id),
                            onToggleTransient: () => controller.setConversationInternal(
                              summary.id,
                              !summary.internal,
                            ),
                          );
                        },
                        searchResult: (result) {
                          final title = result.conversationTitle.isNotEmpty
                              ? result.conversationTitle
                              : 'Untitled';
                          return ListTile(
                            leading: const Icon(Icons.history),
                            title: Text(title),
                            subtitle: Text(
                              result.snippet,
                              maxLines: 2,
                              overflow: TextOverflow.ellipsis,
                            ),
                            onTap: () => controller.selectConversation(
                              result.conversationId,
                            ),
                          );
                        },
                      );
                    },
                  ),
          ),
        ],
      ),
      floatingActionButton: FloatingActionButton(
        onPressed: () => controller.startNewConversation(),
        tooltip: 'New Chat',
        child: const Icon(Icons.add),
      ),
    );
  }
}

class _Header extends StatelessWidget {
  const _Header({
    required this.status,
    required this.onRefresh,
    required this.onMemories,
    required this.onMcpServers,
  });

  final ConnectionStatus status;
  final VoidCallback onRefresh;
  final VoidCallback onMemories;
  final VoidCallback onMcpServers;

  @override
  Widget build(BuildContext context) {
    final indicator = switch (status) {
      ConnectionStatus.connecting => const Text('🔄 Connecting'),
      ConnectionStatus.online => const Text('🟢 Online'),
      ConnectionStatus.error => const Text('🔴 Error'),
    };

    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 24, 16, 12),
      child: Row(
        children: [
          Builder(
            builder: (context) => IconButton(
              icon: const Icon(Icons.menu),
              onPressed: () => Scaffold.of(context).openDrawer(),
              tooltip: 'Menu',
            ),
          ),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text(
                  'AI Chat',
                  style: TextStyle(fontSize: 20, fontWeight: FontWeight.bold),
                ),
                const SizedBox(height: 4),
                indicator,
              ],
            ),
          ),
          PopupMenuButton<String>(
            onSelected: (value) {
              if (value == 'memories') onMemories();
              if (value == 'mcp') onMcpServers();
            },
            itemBuilder: (context) => [
              const PopupMenuItem(
                value: 'memories',
                child: ListTile(
                  leading: Icon(Icons.psychology_outlined),
                  title: Text('Memories'),
                  contentPadding: EdgeInsets.zero,
                ),
              ),
              const PopupMenuItem(
                value: 'mcp',
                child: ListTile(
                  leading: Icon(Icons.hub_outlined),
                  title: Text('MCP Servers'),
                  contentPadding: EdgeInsets.zero,
                ),
              ),
            ],
          ),
          IconButton(
            tooltip: 'Refresh',
            onPressed: onRefresh,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
    );
  }
}

class _SearchEmptyPlaceholder extends StatelessWidget {
  const _SearchEmptyPlaceholder();

  @override
  Widget build(BuildContext context) {
    return const Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.search_off, size: 48),
          SizedBox(height: 8),
          Text('No matching conversations'),
          SizedBox(height: 8),
          Text('Try a different search term'),
        ],
      ),
    );
  }
}

class _EmptyPlaceholder extends StatelessWidget {
  const _EmptyPlaceholder();

  @override
  Widget build(BuildContext context) {
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const Icon(Icons.hourglass_empty, size: 48),
          const SizedBox(height: 8),
          const Text('No conversations yet.'),
          const SizedBox(height: 8),
          Text(
            'Tap “+ Start New” to begin a chat.',
            style: Theme.of(context).textTheme.bodySmall,
          ),
        ],
      ),
    );
  }
}

bool _isSearchActive(AppState state) => state.searchQuery.isNotEmpty;

List<_ConversationItem> _items(AppState state) {
  if (_isSearchActive(state)) {
    return state.searchResults
        .map<_ConversationItem>(_ConversationItem.searchResult)
        .toList();
  }
  return state.conversations
      .map<_ConversationItem>(_ConversationItem.summary)
      .toList();
}

sealed class _ConversationItem {
  const _ConversationItem();

  T map<T>({
    required T Function(ConversationSummary summary) summary,
    required T Function(SearchResult result) searchResult,
  });

  factory _ConversationItem.summary(ConversationSummary summary) =
      _SummaryItem;
  factory _ConversationItem.searchResult(SearchResult result) =
      _SearchItem;
}

class _SummaryItem extends _ConversationItem {
  const _SummaryItem(this.summary);
  final ConversationSummary summary;

  @override
  T map<T>({
    required T Function(ConversationSummary summary) summary,
    required T Function(SearchResult result) searchResult,
  }) =>
      summary(this.summary);
}

class _SearchItem extends _ConversationItem {
  const _SearchItem(this.result);
  final SearchResult result;

  @override
  T map<T>({
    required T Function(ConversationSummary summary) summary,
    required T Function(SearchResult result) searchResult,
  }) =>
      searchResult(result);
}

class _ChatDrawer extends ConsumerStatefulWidget {
  const _ChatDrawer({
    required this.profile,
    required this.availableProfiles,
    required this.onProfileChanged,
    required this.onStartNew,
    required this.onHistory,
    required this.onMemories,
    required this.onMcpServers,
    required this.onSetup,
  });

  final String profile;
  final List<String> availableProfiles;
  final Function(String) onProfileChanged;
  final VoidCallback onStartNew;
  final VoidCallback onHistory;
  final VoidCallback onMemories;
  final VoidCallback onMcpServers;
  final VoidCallback onSetup;

  @override
  ConsumerState<_ChatDrawer> createState() => _ChatDrawerState();
}

class _ChatDrawerState extends ConsumerState<_ChatDrawer> {
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
    final ttsPrefs = ref.watch(ttsPreferencesProvider);
    final ttsPrefsNotifier = ref.read(ttsPreferencesProvider.notifier);

    return Drawer(
      child: ListView(
        padding: EdgeInsets.zero,
        children: [
          DrawerHeader(
            decoration: BoxDecoration(
              color: Theme.of(context).colorScheme.primaryContainer,
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                Text(
                  'Luna AI',
                  style: Theme.of(context).textTheme.headlineSmall?.copyWith(
                    color: Theme.of(context).colorScheme.onPrimaryContainer,
                  ),
                ),
              ],
            ),
          ),
          // New Chat
          ListTile(
            leading: const Icon(Icons.add),
            title: const Text('New Chat'),
            onTap: () {
              widget.onStartNew();
              Navigator.pop(context);
            },
          ),
          const Divider(),
          // Profile Selection
          if (widget.availableProfiles.isNotEmpty)
            ExpansionTile(
              leading: const Icon(Icons.person),
              title: const Text('Profile'),
              subtitle: Text(widget.profile),
              children: widget.availableProfiles.map((profile) {
                return ListTile(
                  title: Text(profile),
                  selected: profile == widget.profile,
                  onTap: () {
                    widget.onProfileChanged(profile);
                    Navigator.pop(context);
                  },
                );
              }).toList(),
            ),
          // TTS Toggle
          SwitchListTile(
            secondary: const Icon(Icons.volume_up),
            title: const Text('Text-to-Speech'),
            value: ttsPrefs.enabled,
            onChanged: (value) {
              ttsPrefsNotifier.setEnabled(value);
            },
          ),
          // Voice Language
          ListTile(
            leading: const Icon(Icons.language),
            title: const Text('Voice Language'),
            subtitle: _loadingLanguages
                ? const Text('Loading...')
                : (_availableLanguages != null && _availableLanguages!.isNotEmpty)
                    ? Builder(
                        builder: (context) {
                          // Filter to only show favorite languages
                          final favoriteLanguages = sttPrefs.favoriteLanguages;
                          final favoriteLangItems = _availableLanguages!
                              .where((lang) {
                                final langCode = lang.toString();
                                return favoriteLanguages.contains(langCode);
                              })
                              .toList();
                          
                          // If current language is not in favorites, add it temporarily
                          final currentLangCode = ttsPrefs.language;
                          if (!favoriteLanguages.contains(currentLangCode)) {
                            favoriteLangItems.insert(0, currentLangCode);
                          }
                          
                          if (favoriteLangItems.isEmpty) {
                            return TextButton.icon(
                              onPressed: _loadLanguages,
                              icon: const Icon(Icons.refresh, size: 18),
                              label: const Text('Load Languages'),
                            );
                          }
                          
                          return DropdownButton<String>(
                            value: ttsPrefs.language,
                            isExpanded: true,
                            underline: Container(),
                            items: favoriteLangItems
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
                                // Set both TTS and STT language
                                ttsPrefsNotifier.setLanguage(value);
                                sttPrefsNotifier.setLanguage(value);
                                ref.read(ttsServiceProvider).setLanguage(value);
                              }
                            },
                          );
                        },
                      )
                    : TextButton.icon(
                        onPressed: _loadLanguages,
                        icon: const Icon(Icons.refresh, size: 18),
                        label: const Text('Load Languages'),
                      ),
          ),
          const Divider(),
          // History
          ListTile(
            leading: const Icon(Icons.history),
            title: const Text('History'),
            onTap: () {
              widget.onHistory();
              Navigator.pop(context);
            },
          ),
          // Memories
          ListTile(
            leading: const Icon(Icons.psychology_outlined),
            title: const Text('Memories'),
            onTap: () {
              widget.onMemories();
              Navigator.pop(context);
            },
          ),
          // MCP Servers
          ListTile(
            leading: const Icon(Icons.hub_outlined),
            title: const Text('MCP Servers'),
            onTap: () {
              widget.onMcpServers();
              Navigator.pop(context);
            },
          ),
          // Setup
          ListTile(
            leading: const Icon(Icons.settings),
            title: const Text('Setup'),
            onTap: () {
              widget.onSetup();
              Navigator.pop(context);
            },
          ),
        ],
      ),
    );
  }
}

