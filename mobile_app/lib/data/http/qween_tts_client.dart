import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:http/http.dart' as http;

import '../../core/config/qween_tts_preferences.dart';

/// International endpoint (Singapore) for Qwen TTS.
const String kQweenTtsEndpoint =
    'https://dashscope-intl.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation';

/// Qwen TTS model with instruction control support.
const String kQweenTtsModel = 'qwen3-tts-instruct-flash';

/// Qwen TTS sample rate (24 kHz PCM).
const int kQweenTtsSampleRate = 24000;

/// Client for Qwen TTS API.
///
/// Response format (discovered via probing):
///
/// Non-streaming:
///   { "output": { "audio": { "data": "", "url": "http://...wav", "id": "..." }, "finish_reason": "stop" } }
///   → audio.data is empty; audio.url has a temporary WAV download link
///
/// SSE streaming (X-DashScope-SSE: enable):
///   data:{ "output": { "audio": { "data": "...base64 PCM...", "id": "..." }, "finish_reason": "null" } }
///   ...multiple chunks...
///   data:{ "output": { "audio": { "data": "", "url": "http://...wav", "id": "..." }, "finish_reason": "stop" } }
///   → audio.data has base64-encoded raw PCM (16-bit mono, 24 kHz)
class QweenTtsClient {
  QweenTtsClient({
    required String apiKey,
    required String voice,
    required String instructions,
  })  : _apiKey = apiKey,
        _voice = voice,
        _instructions = instructions;

  final String _apiKey;
  final String _voice;
  final String _instructions;

  http.Client? _client;
  bool _cancelled = false;

  /// Non-streaming: returns a URL to a .wav file. Caller must download it.
  Future<String?> synthesizeUrl(String text) async {
    if (text.trim().isEmpty) return null;

    _cancelled = false;
    _client = http.Client();

    final uri = Uri.parse(kQweenTtsEndpoint);
    final body = _buildRequestBody(text);

    try {
      final response = await _client!.post(
        uri,
        headers: {
          'Authorization': 'Bearer $_apiKey',
          'Content-Type': 'application/json',
        },
        body: jsonEncode(body),
      );
      if (_cancelled) return null;

      if (response.statusCode != 200) {
        debugPrint('Qween TTS: error ${response.statusCode} - ${response.body}');
        return null;
      }

      final json = jsonDecode(response.body) as Map<String, dynamic>;
      final output = json['output'] as Map<String, dynamic>?;
      final audio = output?['audio'] as Map<String, dynamic>?;
      final url = audio?['url'] as String?;

      debugPrint('Qween TTS: got URL ${url != null ? "(${url.length} chars)" : "null"}');
      return url;
    } catch (e) {
      if (!_cancelled) debugPrint('Qween TTS error: $e');
      return null;
    } finally {
      _client?.close();
      _client = null;
    }
  }

  /// Download a WAV file from the temporary URL.
  Future<Uint8List?> downloadWav(String url) async {
    _client = http.Client();
    try {
      final response = await _client!.get(Uri.parse(url));
      if (_cancelled) return null;
      if (response.statusCode == 200) {
        debugPrint('Qween TTS: downloaded ${response.bodyBytes.length} bytes');
        return response.bodyBytes;
      }
      debugPrint('Qween TTS: download failed ${response.statusCode}');
      return null;
    } catch (e) {
      if (!_cancelled) debugPrint('Qween TTS download error: $e');
      return null;
    } finally {
      _client?.close();
      _client = null;
    }
  }

  /// SSE streaming: yields raw PCM chunks (16-bit mono, 24 kHz) as they arrive.
  /// Path: output.audio.data (base64 string inside audio dict)
  Stream<List<int>> synthesizeStream(String text) async* {
    if (text.trim().isEmpty) return;

    _cancelled = false;
    _client = http.Client();

    final uri = Uri.parse(kQweenTtsEndpoint);
    final body = _buildRequestBody(text);

    final request = http.Request('POST', uri);
    request.headers['Authorization'] = 'Bearer $_apiKey';
    request.headers['Content-Type'] = 'application/json';
    request.headers['X-DashScope-SSE'] = 'enable';
    request.body = jsonEncode(body);

    try {
      final response = await _client!.send(request);
      if (_cancelled) return;

      if (response.statusCode != 200) {
        final bodyStr = await response.stream.bytesToString();
        debugPrint('Qween TTS SSE error: $bodyStr');
        return;
      }

      int chunkCount = 0;

      await for (final line in response.stream
          .transform(utf8.decoder)
          .transform(const LineSplitter())) {
        if (_cancelled) return;
        if (!line.startsWith('data:')) continue;

        final dataStr = line.substring(5).trim();
        if (dataStr.isEmpty) continue;

        try {
          final json = jsonDecode(dataStr) as Map<String, dynamic>;
          final output = json['output'] as Map<String, dynamic>?;
          final audio = output?['audio'] as Map<String, dynamic>?;
          final audioData = audio?['data'] as String?;
          final finishReason = output?['finish_reason'] as String?;

          if (audioData != null && audioData.isNotEmpty) {
            final decoded = base64Decode(audioData);
            if (decoded.isNotEmpty) {
              chunkCount++;
              yield decoded;
            }
          }

          // Last chunk has finish_reason: "stop" and a url field
          if (finishReason == 'stop') {
            debugPrint('Qween TTS SSE: done after $chunkCount audio chunks');
            return;
          }
        } catch (e) {
          debugPrint('Qween TTS SSE parse error: $e');
        }
      }

      debugPrint('Qween TTS SSE: stream ended, $chunkCount chunks');
    } finally {
      _client?.close();
      _client = null;
    }
  }

  Map<String, dynamic> _buildRequestBody(String text) {
    final params = <String, dynamic>{
      'voice': _voice,
      'language_type': 'English',
    };

    // Instructions only for instruct model
    if (kQweenTtsModel.contains('instruct')) {
      params['instructions'] = _instructions.trim().isEmpty
          ? kQweenDefaultInstructions
          : _instructions;
      params['optimize_instructions'] = true;
    }

    return {
      'model': kQweenTtsModel,
      'input': {'text': text},
      'parameters': params,
    };
  }

  void cancel() {
    _cancelled = true;
    _client?.close();
    _client = null;
  }

  void dispose() {
    cancel();
  }
}
