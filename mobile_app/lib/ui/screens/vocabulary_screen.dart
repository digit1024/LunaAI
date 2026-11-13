import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../voice/voice_catalog.dart';
import '../../voice/voice_preferences.dart';

class VocabularyScreen extends ConsumerStatefulWidget {
  const VocabularyScreen({super.key});

  @override
  ConsumerState<VocabularyScreen> createState() => _VocabularyScreenState();
}

class _VocabularyScreenState extends ConsumerState<VocabularyScreen> {
  final _controller = TextEditingController();

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  Future<void> _add() async {
    final term = _controller.text.trim();
    if (term.isEmpty) return;
    await ref.read(voicePreferencesProvider.notifier).addVocabularyTerm(term);
    _controller.clear();
  }

  @override
  Widget build(BuildContext context) {
    final prefs = ref.watch(voicePreferencesProvider);
    final notifier = ref.read(voicePreferencesProvider.notifier);
    final model = VoiceCatalog.sttModel(prefs.stt.providerId, prefs.stt.modelId);

    return Scaffold(
      appBar: AppBar(title: const Text('Vocabulary')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Text(
            'Names and terms are passed to the STT model as a hint. '
            'They are not rewritten in the transcript.',
            style: Theme.of(context).textTheme.bodyMedium,
          ),
          const SizedBox(height: 12),
          if (!model.supportsVocabularyHint)
            Container(
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: Theme.of(context).colorScheme.errorContainer,
                borderRadius: BorderRadius.circular(8),
              ),
              child: Text(
                'The selected STT model (${model.label}) does not support '
                'vocabulary hints. Switch to Qwen3 ASR or Groq Whisper to use this list.',
                style: TextStyle(
                  color: Theme.of(context).colorScheme.onErrorContainer,
                ),
              ),
            ),
          const SizedBox(height: 12),
          Row(
            children: [
              Expanded(
                child: TextField(
                  controller: _controller,
                  textInputAction: TextInputAction.done,
                  decoration: const InputDecoration(
                    labelText: 'Add a word or phrase',
                    hintText: 'e.g. Filip',
                    border: OutlineInputBorder(),
                  ),
                  onSubmitted: (_) => _add(),
                ),
              ),
              const SizedBox(width: 8),
              FilledButton.icon(
                onPressed: _add,
                icon: const Icon(Icons.add),
                label: const Text('Add'),
              ),
            ],
          ),
          const SizedBox(height: 16),
          if (prefs.vocabulary.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 24),
              child: Text(
                'No vocabulary terms yet.',
                textAlign: TextAlign.center,
                style: Theme.of(context).textTheme.bodySmall,
              ),
            )
          else
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: prefs.vocabulary
                  .map(
                    (term) => InputChip(
                      label: Text(term),
                      onDeleted: () => notifier.removeVocabularyTerm(term),
                    ),
                  )
                  .toList(),
            ),
        ],
      ),
    );
  }
}
