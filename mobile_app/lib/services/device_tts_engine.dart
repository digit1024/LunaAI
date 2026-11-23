import 'package:flutter/foundation.dart';

/// The slice of the on-device speech engine that [BuiltInTtsProvider] drives.
///
/// Implemented by `TtsService` (flutter_tts). Extracted as an interface so the
/// chunk-queuing logic can be unit tested without a platform channel.
abstract class DeviceTtsEngine {
  /// True when the engine can append utterances to the one currently speaking,
  /// so a long message is read without a gap between chunks.
  ///
  /// Android's `TextToSpeech` supports this via `QUEUE_ADD`; other platforms do
  /// not and fall back to speaking chunks one at a time.
  bool get supportsUtteranceQueue;

  Future<void> setLanguage(String languageCode);

  /// Switches the engine between appending utterances ([enabled] true) and
  /// flushing whatever is speaking before each new utterance (false).
  ///
  /// No-op when [supportsUtteranceQueue] is false.
  Future<void> setUtteranceQueueing(bool enabled);

  /// Speaks [text], appending it to the queue when queueing is enabled.
  ///
  /// [onComplete] fires once for this utterance, whether it finished, errored
  /// or was cancelled. It is not fired by [stop], which drops pending
  /// callbacks instead.
  Future<void> speak(String text, {VoidCallback? onComplete});

  /// Stops speech immediately and drops every pending utterance callback.
  Future<void> stop();
}
