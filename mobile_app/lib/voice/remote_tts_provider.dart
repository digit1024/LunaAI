import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:audioplayers/audioplayers.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:http/http.dart' as http;
import 'package:path_provider/path_provider.dart';

import '../services/tts_provider.dart';
import '../tts/tts_text_chunker.dart';
import 'voice_catalog.dart';
import 'voice_models.dart';
import 'voice_preferences.dart';

/// TTS adapter for OpenAI-compatible `/audio/speech` and DashScope TTS.
class RemoteTtsProvider implements TtsProvider {
  RemoteTtsProvider(this._ref);

  final Ref _ref;
  AudioPlayer? _player;
  StreamSubscription<void>? _completeSub;
  Completer<void>? _playbackDone;
  VoidCallback? _pendingOnComplete;
  bool _stopped = false;

  @override
  Future<void> speak(String text, {VoidCallback? onComplete}) async {
    final trimmed = text.trim();
    if (trimmed.isEmpty) {
      onComplete?.call();
      return;
    }

    _stopped = false;
    _pendingOnComplete = onComplete;

    final prefs = _ref.read(voicePreferencesProvider);
    final config = prefs.tts;
    final provider = VoiceCatalog.provider(config.providerId);
    final preset = VoiceCatalog.ttsModel(config.providerId, config.modelId);

    final endpoint = config.endpointOverride.isNotEmpty
        ? config.endpointOverride
        : (provider.ttsEndpoint ?? '');
    final model =
        config.modelOverride.isNotEmpty ? config.modelOverride : preset.model;
    final voice = config.voice.isNotEmpty ? config.voice : preset.defaultVoice;

    if (endpoint.isEmpty || model.isEmpty) {
      debugPrint('TTS: endpoint/model is not configured');
      _callComplete();
      return;
    }

    String? apiKey;
    if (provider.requiresApiKey) {
      apiKey = await _ref
          .read(voicePreferencesProvider.notifier)
          .getApiKey(config.providerId);
      if (apiKey == null || apiKey.trim().isEmpty) {
        debugPrint('TTS: API key is not configured');
        _callComplete();
        return;
      }
    }

    final chunks = chunkTextForTts(trimmed, maxLen: preset.maxChars);
    if (chunks.isEmpty) {
      _callComplete();
      return;
    }

    try {
      for (var i = 0; i < chunks.length && !_stopped; i++) {
        final audio = await _synthesize(
          text: chunks[i],
          protocol: provider.ttsProtocol ?? VoiceApiProtocol.openAiSpeech,
          endpoint: endpoint,
          model: model,
          voice: voice,
          language: prefs.language,
          apiKey: apiKey,
          extraParams: config.extraParamsJson,
          responseFormat: preset.responseFormat,
        );
        if (_stopped || audio == null) continue;
        await _play(audio.bytes, audio.extension);
      }
    } catch (e, st) {
      debugPrint('TTS: synthesis/playback failed: $e\n$st');
    } finally {
      if (!_stopped) _callComplete();
    }
  }

  Future<_AudioBytes?> _synthesize({
    required String text,
    required VoiceApiProtocol protocol,
    required String endpoint,
    required String model,
    required String voice,
    required String language,
    required String? apiKey,
    required String extraParams,
    required String responseFormat,
  }) async {
    if (protocol == VoiceApiProtocol.dashScopeTts) {
      return _synthesizeDashScope(
        text: text,
        endpoint: endpoint,
        model: model,
        voice: voice,
        language: language,
        apiKey: apiKey,
        extraParams: extraParams,
      );
    }
    return _synthesizeOpenAi(
      text: text,
      endpoint: endpoint,
      model: model,
      voice: voice,
      apiKey: apiKey,
      extraParams: extraParams,
      responseFormat: responseFormat,
    );
  }

  Future<_AudioBytes?> _synthesizeOpenAi({
    required String text,
    required String endpoint,
    required String model,
    required String voice,
    required String? apiKey,
    required String extraParams,
    required String responseFormat,
  }) async {
    final extra = _parseExtraParams(extraParams);
    final extraFormat = extra['response_format'];
    final baseFormat =
        extraFormat is String ? extraFormat : responseFormat;
    final formats = <String>[
      baseFormat,
      if (baseFormat != 'pcm') 'pcm',
    ];

    http.Response? response;
    var requestedFormat = responseFormat;
    for (final format in formats) {
      final body = <String, dynamic>{
        'model': model,
        'input': text,
        'voice': voice,
        ...extra,
        'response_format': format,
      };
      response = await http.post(
        Uri.parse(endpoint),
        headers: {
          if (apiKey != null && apiKey.isNotEmpty)
            'Authorization': 'Bearer $apiKey',
          'Content-Type': 'application/json',
        },
        body: jsonEncode(body),
      );
      requestedFormat = format;
      if (_stopped) return null;
      if (response.statusCode == 200) break;
      final bodyText = response.body;
      debugPrint(
        'TTS: error ${response.statusCode} format=$format - ${_short(bodyText)}',
      );
      final isFormatError = response.statusCode == 400 &&
          (bodyText.toLowerCase().contains('response_format') ||
              bodyText.toLowerCase().contains('format') ||
              bodyText.toLowerCase().contains('pcm') ||
              bodyText.toLowerCase().contains('mp3'));
      if (!isFormatError || format == 'pcm') {
        return null;
      }
    }

    if (response == null || response.statusCode != 200) return null;

    final contentType =
        (response.headers['content-type'] ?? '').toLowerCase();
    if (contentType.contains('json')) {
      debugPrint('TTS: non-audio response - ${_short(response.body)}');
      return null;
    }
    var format = requestedFormat;
    if (contentType.contains('mpeg') || contentType.contains('mp3')) {
      format = 'mp3';
    } else if (contentType.contains('wav')) {
      format = 'wav';
    } else if (contentType.contains('pcm')) {
      format = 'pcm';
    } else {
      final bytes = response.bodyBytes;
      if (bytes.length > 12 &&
          bytes[0] == 0x52 &&
          bytes[1] == 0x49 &&
          bytes[2] == 0x46 &&
          bytes[3] == 0x46) {
        format = 'wav';
      } else if (bytes.length > 3 &&
          bytes[0] == 0x49 &&
          bytes[1] == 0x44 &&
          bytes[2] == 0x33) {
        format = 'mp3';
      }
    }

    var bytes = response.bodyBytes;
    if (bytes.isEmpty) {
      debugPrint(
        'TTS: empty audio response content-type="$contentType"',
      );
      return null;
    }
    if (format == 'pcm') {
      bytes = _wrapPcmAsWav(bytes);
      format = 'wav';
    }
    debugPrint(
      'TTS: ${bytes.length} bytes content-type="$contentType" format=$format',
    );
    return _AudioBytes(bytes, format);
  }

  /// OpenRouter returns raw PCM for some providers. Wrap it so the player can
  /// consume it. PCM from these TTS providers is 24 kHz mono 16-bit.
  static Uint8List _wrapPcmAsWav(
    Uint8List pcm, {
    int sampleRate = 24000,
    int channels = 1,
    int bitsPerSample = 16,
  }) {
    final byteRate = sampleRate * channels * (bitsPerSample ~/ 8);
    final blockAlign = channels * (bitsPerSample ~/ 8);
    final header = ByteData(44);
    void ascii(int offset, String value) {
      for (var i = 0; i < value.length; i++) {
        header.setUint8(offset + i, value.codeUnitAt(i));
      }
    }

    ascii(0, 'RIFF');
    header.setUint32(4, 36 + pcm.length, Endian.little);
    ascii(8, 'WAVE');
    ascii(12, 'fmt ');
    header.setUint32(16, 16, Endian.little);
    header.setUint16(20, 1, Endian.little);
    header.setUint16(22, channels, Endian.little);
    header.setUint32(24, sampleRate, Endian.little);
    header.setUint32(28, byteRate, Endian.little);
    header.setUint16(32, blockAlign, Endian.little);
    header.setUint16(34, bitsPerSample, Endian.little);
    ascii(36, 'data');
    header.setUint32(40, pcm.length, Endian.little);

    final out = Uint8List(44 + pcm.length);
    out.setRange(0, 44, header.buffer.asUint8List());
    out.setRange(44, out.length, pcm);
    return out;
  }

  Future<_AudioBytes?> _synthesizeDashScope({
    required String text,
    required String endpoint,
    required String model,
    required String voice,
    required String language,
    required String? apiKey,
    required String extraParams,
  }) async {
    final body = <String, dynamic>{
      'model': model,
      'input': {
        'text': text,
        'voice': voice,
        'language_type':
            VoicePreferencesNotifier.languageToQwenName(language),
        'format': 'wav',
        'sample_rate': 24000,
      },
      ..._parseExtraParams(extraParams),
    };

    final response = await http.post(
      Uri.parse(endpoint),
      headers: {
        if (apiKey != null && apiKey.isNotEmpty)
          'Authorization': 'Bearer $apiKey',
        'Content-Type': 'application/json',
      },
      body: jsonEncode(body),
    );
    if (_stopped) return null;
    if (response.statusCode != 200) {
      debugPrint(
        'TTS: error ${response.statusCode} - ${_short(response.body)}',
      );
      return null;
    }

    final json = jsonDecode(response.body) as Map<String, dynamic>;
    final output = json['output'];
    if (output is! Map) return null;
    final audio = output['audio'];
    if (audio is! Map) return null;

    final data = audio['data'];
    if (data is String && data.isNotEmpty) {
      return _AudioBytes(base64Decode(data), 'wav');
    }

    final url = audio['url'];
    if (url is String && url.isNotEmpty) {
      final download = await http.get(Uri.parse(url));
      if (download.statusCode != 200) return null;
      return _AudioBytes(download.bodyBytes, 'wav');
    }
    return null;
  }

  Future<void> _play(Uint8List bytes, String extension) async {
    final player = _player ??= AudioPlayer(playerId: 'luna_remote_tts');
    final dir = await getTemporaryDirectory();
    final path =
        '${dir.path}/luna_tts_${DateTime.now().microsecondsSinceEpoch}.$extension';
    final file = File(path);
    await file.writeAsBytes(bytes, flush: true);

    final done = Completer<void>();
    _playbackDone = done;
    _completeSub?.cancel();
    _completeSub = player.onPlayerComplete.listen((_) {
      if (!done.isCompleted) done.complete();
    });

    try {
      await player.play(DeviceFileSource(path));
      await done.future.timeout(const Duration(seconds: 180), onTimeout: () {});
    } catch (e) {
      debugPrint('TTS: playback failed: $e');
    } finally {
      await _completeSub?.cancel();
      _completeSub = null;
      _playbackDone = null;
      try {
        if (await file.exists()) await file.delete();
      } catch (_) {}
    }
  }

  @override
  Future<void> stop() async {
    _stopped = true;
    try {
      await _player?.stop();
    } catch (_) {}
    if (_playbackDone != null && !_playbackDone!.isCompleted) {
      _playbackDone!.complete();
    }
    _callComplete();
  }

  void _callComplete() {
    final callback = _pendingOnComplete;
    _pendingOnComplete = null;
    callback?.call();
  }

  Future<void> dispose() async {
    await stop();
    _completeSub?.cancel();
    await _player?.dispose();
    _player = null;
  }

  static Map<String, dynamic> _parseExtraParams(String raw) {
    if (raw.trim().isEmpty) return const {};
    try {
      final decoded = jsonDecode(raw);
      if (decoded is Map<String, dynamic>) return decoded;
      if (decoded is Map) return decoded.cast<String, dynamic>();
    } catch (e) {
      debugPrint('TTS: ignoring invalid extra params JSON: $e');
    }
    return const {};
  }

  static String _short(String value) {
    final trimmed = value.trim();
    if (trimmed.length <= 300) return trimmed;
    return '${trimmed.substring(0, 300)}…';
  }
}

class _AudioBytes {
  const _AudioBytes(this.bytes, this.extension);
  final Uint8List bytes;
  final String extension;
}
