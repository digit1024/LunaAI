import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:http/http.dart' as http;

import '../services/tts_provider.dart';
import '../tts/tts_audio_sink.dart';
import '../tts/tts_playback_pipeline.dart';
import '../tts/tts_text_chunker.dart';
import 'voice_catalog.dart';
import 'voice_models.dart';
import 'voice_preferences.dart';

/// Upper bound for a single synthesis request. Synthesis of chunk N+1 overlaps
/// playback of chunk N, so this only needs to cover a badly stalled provider.
const Duration kTtsRequestTimeout = Duration(seconds: 60);

/// TTS adapter for OpenAI-compatible `/audio/speech` and DashScope TTS.
///
/// Long messages are split into chunks that are synthesised ahead of time and
/// handed to a [GaplessAudioSink], which keeps one clip prepared while the
/// previous one is still audible.
class RemoteTtsProvider implements TtsProvider {
  RemoteTtsProvider(this._ref);

  final Ref _ref;

  /// Remembered `response_format` per model, so a provider that rejects the
  /// preferred format is not probed again for every chunk.
  final Map<String, String> _formatCache = {};

  http.Client? _client;
  TtsAudioSink? _sink;
  TtsPlaybackPipeline? _pipeline;
  VoidCallback? _pendingOnComplete;
  int _generation = 0;

  @override
  Future<void> speak(String text, {VoidCallback? onComplete}) async {
    final trimmed = text.trim();
    if (trimmed.isEmpty) {
      onComplete?.call();
      return;
    }

    final generation = ++_generation;
    await _cancelCurrent();
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
    if (generation != _generation) return;

    final chunks = chunkTextForTts(
      trimmed,
      maxLen: preset.maxChars,
      firstMaxLen: kDefaultTtsFirstChunkLength,
    );
    if (chunks.isEmpty) {
      _callComplete();
      return;
    }

    final client = http.Client();
    final sink = GaplessAudioSink();
    final protocol = provider.ttsProtocol ?? VoiceApiProtocol.openAiSpeech;
    final pipeline = TtsPlaybackPipeline(
      synthesize: (chunk, index) => _synthesize(
        text: chunk,
        protocol: protocol,
        endpoint: endpoint,
        model: model,
        voice: voice,
        language: prefs.language,
        apiKey: apiKey,
        extraParams: config.extraParamsJson,
        responseFormat: preset.responseFormat,
        client: client,
      ),
      sink: sink,
      onChunkFailed: (index) => debugPrint(
        'TTS: no audio for chunk ${index + 1}/${chunks.length}, skipping',
      ),
    );

    _client = client;
    _sink = sink;
    _pipeline = pipeline;

    try {
      await pipeline.run(chunks);
    } catch (e, st) {
      debugPrint('TTS: synthesis/playback failed: $e\n$st');
    } finally {
      await _teardown(pipeline, sink, client);
      // A newer speak()/stop() already reported completion for this session.
      if (generation == _generation) _callComplete();
    }
  }

  @override
  Future<void> stop() async {
    _generation++;
    await _cancelCurrent();
    _callComplete();
  }

  Future<void> dispose() async {
    _generation++;
    await _cancelCurrent();
    _pendingOnComplete = null;
    _formatCache.clear();
  }

  /// Aborts and releases the session currently held in the fields, if any.
  Future<void> _cancelCurrent() async {
    final pipeline = _pipeline;
    final sink = _sink;
    final client = _client;
    _pipeline = null;
    _sink = null;
    _client = null;
    await _teardown(pipeline, sink, client);
  }

  Future<void> _teardown(
    TtsPlaybackPipeline? pipeline,
    TtsAudioSink? sink,
    http.Client? client,
  ) async {
    if (pipeline != null) {
      try {
        await pipeline.cancel();
      } catch (_) {}
    }
    if (sink != null) {
      try {
        await sink.dispose();
      } catch (_) {}
    }
    client?.close();
  }

  void _callComplete() {
    final callback = _pendingOnComplete;
    _pendingOnComplete = null;
    callback?.call();
  }

  Future<TtsAudioClip?> _synthesize({
    required String text,
    required VoiceApiProtocol protocol,
    required String endpoint,
    required String model,
    required String voice,
    required String language,
    required String? apiKey,
    required String extraParams,
    required String responseFormat,
    required http.Client client,
  }) async {
    final stopwatch = Stopwatch()..start();
    try {
      if (protocol == VoiceApiProtocol.dashScopeTts) {
        return await _synthesizeDashScope(
          text: text,
          endpoint: endpoint,
          model: model,
          voice: voice,
          language: language,
          apiKey: apiKey,
          extraParams: extraParams,
          client: client,
        );
      }
      return await _synthesizeOpenAi(
        text: text,
        endpoint: endpoint,
        model: model,
        voice: voice,
        apiKey: apiKey,
        extraParams: extraParams,
        responseFormat: responseFormat,
        client: client,
      );
    } finally {
      debugPrint('TTS: synthesised ${text.length} chars in '
          '${stopwatch.elapsedMilliseconds}ms');
    }
  }

  Future<TtsAudioClip?> _synthesizeOpenAi({
    required String text,
    required String endpoint,
    required String model,
    required String voice,
    required String? apiKey,
    required String extraParams,
    required String responseFormat,
    required http.Client client,
  }) async {
    final extra = _parseExtraParams(extraParams);
    final extraFormat = extra['response_format'];
    final cacheKey = '$endpoint|$model';
    final preferred = extraFormat is String
        ? extraFormat
        : (_formatCache[cacheKey] ?? responseFormat);

    final negotiated = await _postWithFormatFallback(
      client: client,
      endpoint: endpoint,
      apiKey: apiKey,
      text: text,
      model: model,
      voice: voice,
      extra: extra,
      preferredFormat: preferred,
    );
    final response = negotiated.response;
    _formatCache[cacheKey] = negotiated.format;

    final contentType = (response.headers['content-type'] ?? '').toLowerCase();
    if (contentType.contains('json')) {
      throw TtsSynthesisFailure(
        'non-audio response - ${_short(response.body)}',
      );
    }

    var format = negotiated.format;
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
      throw TtsSynthesisFailure('empty audio response '
          'content-type="$contentType"');
    }
    if (format == 'pcm') {
      bytes = _wrapPcmAsWav(bytes);
      format = 'wav';
    }
    debugPrint('TTS: ${bytes.length} bytes content-type="$contentType" '
        'format=$format');
    return TtsAudioClip(bytes, format);
  }

  /// Posts the synthesis request, falling back to `pcm` once when the provider
  /// rejects the preferred `response_format`.
  ///
  /// Without the remembered format this costs a second round-trip per chunk,
  /// which used to land in the audible gap between chunks.
  Future<({http.Response response, String format})> _postWithFormatFallback({
    required http.Client client,
    required String endpoint,
    required String? apiKey,
    required String text,
    required String model,
    required String voice,
    required Map<String, dynamic> extra,
    required String preferredFormat,
  }) async {
    final formats = <String>[
      preferredFormat,
      if (preferredFormat != 'pcm') 'pcm',
    ];

    var last = await _postSpeech(
      client: client,
      endpoint: endpoint,
      apiKey: apiKey,
      text: text,
      model: model,
      voice: voice,
      extra: extra,
      format: formats.first,
    );
    if (last.statusCode == 200) {
      return (response: last, format: formats.first);
    }

    final bodyText = last.body;
    final lowercase = bodyText.toLowerCase();
    final isFormatError = last.statusCode == 400 &&
        (lowercase.contains('response_format') ||
            lowercase.contains('format') ||
            lowercase.contains('pcm') ||
            lowercase.contains('mp3'));
    if (!isFormatError || formats.length < 2) {
      throw TtsSynthesisFailure(
        'error ${last.statusCode} - ${_short(bodyText)}',
        retryable: _isRetryableStatus(last.statusCode),
      );
    }

    debugPrint('TTS: error ${last.statusCode} format=${formats.first} '
        '- ${_short(bodyText)}; retrying with ${formats.last}');
    last = await _postSpeech(
      client: client,
      endpoint: endpoint,
      apiKey: apiKey,
      text: text,
      model: model,
      voice: voice,
      extra: extra,
      format: formats.last,
    );
    if (last.statusCode != 200) {
      throw TtsSynthesisFailure(
        'error ${last.statusCode} - ${_short(last.body)}',
        retryable: _isRetryableStatus(last.statusCode),
      );
    }
    return (response: last, format: formats.last);
  }

  Future<http.Response> _postSpeech({
    required http.Client client,
    required String endpoint,
    required String? apiKey,
    required String text,
    required String model,
    required String voice,
    required Map<String, dynamic> extra,
    required String format,
  }) {
    return _post(
      client: client,
      endpoint: endpoint,
      apiKey: apiKey,
      body: <String, dynamic>{
        'model': model,
        'input': text,
        'voice': voice,
        ...extra,
        'response_format': format,
      },
    );
  }

  Future<http.Response> _post({
    required http.Client client,
    required String endpoint,
    required String? apiKey,
    required Map<String, dynamic> body,
  }) async {
    final request = http.Request('POST', Uri.parse(endpoint))
      ..headers.addAll({
        if (apiKey != null && apiKey.isNotEmpty)
          'Authorization': 'Bearer $apiKey',
        'Content-Type': 'application/json',
      })
      ..body = jsonEncode(body);

    try {
      final streamed = await client.send(request).timeout(kTtsRequestTimeout);
      return await http.Response.fromStream(streamed);
    } on TimeoutException {
      throw TtsSynthesisFailure(
        'request timed out after ${kTtsRequestTimeout.inSeconds}s',
        retryable: true,
      );
    }
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

  Future<TtsAudioClip?> _synthesizeDashScope({
    required String text,
    required String endpoint,
    required String model,
    required String voice,
    required String language,
    required String? apiKey,
    required String extraParams,
    required http.Client client,
  }) async {
    final response = await _post(
      client: client,
      endpoint: endpoint,
      apiKey: apiKey,
      body: <String, dynamic>{
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
      },
    );
    if (response.statusCode != 200) {
      throw TtsSynthesisFailure(
        'error ${response.statusCode} - ${_short(response.body)}',
        retryable: _isRetryableStatus(response.statusCode),
      );
    }

    final json = jsonDecode(response.body) as Map<String, dynamic>;
    final output = json['output'];
    if (output is! Map) {
      throw const TtsSynthesisFailure('DashScope response has no output');
    }
    final audio = output['audio'];
    if (audio is! Map) {
      throw const TtsSynthesisFailure('DashScope output has no audio');
    }

    final data = audio['data'];
    if (data is String && data.isNotEmpty) {
      return TtsAudioClip(base64Decode(data), 'wav');
    }

    final url = audio['url'];
    if (url is String && url.isNotEmpty) {
      final download = await client
          .get(Uri.parse(url))
          .timeout(kTtsRequestTimeout);
      if (download.statusCode != 200) {
        throw TtsSynthesisFailure(
          'audio download failed with ${download.statusCode}',
          retryable: _isRetryableStatus(download.statusCode),
        );
      }
      return TtsAudioClip(download.bodyBytes, 'wav');
    }
    return null;
  }

  static bool _isRetryableStatus(int status) =>
      status == 408 || status == 425 || status == 429 || status >= 500;

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
