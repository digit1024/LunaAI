import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../application/app_controller.dart';

/// Shows a dialog to rename a conversation.
Future<void> showRenameConversationDialog({
  required BuildContext context,
  required WidgetRef ref,
  required String conversationId,
  required String currentTitle,
}) async {
  final controller = TextEditingController(text: currentTitle);
  final formKey = GlobalKey<FormState>();

  await showDialog<void>(
    context: context,
    builder: (dialogContext) {
      return AlertDialog(
        title: const Text('Rename conversation'),
        content: Form(
          key: formKey,
          child: TextFormField(
            controller: controller,
            autofocus: true,
            decoration: const InputDecoration(
              labelText: 'Title',
              hintText: 'Enter a title',
            ),
            validator: (value) {
              if (value == null || value.trim().isEmpty) {
                return 'Title cannot be empty';
              }
              if (value.trim().length > 200) {
                return 'Title is too long (max 200 characters)';
              }
              return null;
            },
            onFieldSubmitted: (_) {
              if (formKey.currentState?.validate() ?? false) {
                Navigator.of(dialogContext).pop();
                ref.read(appControllerProvider.notifier).renameConversation(
                      conversationId,
                      controller.text.trim(),
                    );
              }
            },
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () {
              if (formKey.currentState?.validate() ?? false) {
                Navigator.of(dialogContext).pop();
                ref.read(appControllerProvider.notifier).renameConversation(
                      conversationId,
                      controller.text.trim(),
                    );
              }
            },
            child: const Text('Save'),
          ),
        ],
      );
    },
  );

  controller.dispose();
}
