import 'dart:async';
import 'dart:convert';

import 'package:http/http.dart' as http;

import '../../core/config/qween_tts_preferences.dart';

/// International endpoint (Singapore) for Qwen TTS.
const String kQweenTtsEndpoint =
    'https://dashscope-intl.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation';

/// Qwen TTS model with instruction control support.
const String kQweenTtsModel = 'qwen3-tts-instruct-flash';

/// Qwen TTS sample rate (24 kHz PCM).
const int kQweenTtsSampleRate = 24000;

/// Client for Qwen TTS streaming API.
/// Consumes SSE response with Base64 PCM chunks and yields raw PCM bytes.
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

  /// Synthesize text to speech. Yields PCM chunks (16-bit mono, 24 kHz) as they arrive.
  /// Returns when stream completes or is cancelled via [cancel].
  Stream<List<int>> synthesizeStream(String text) async* {
    if (text.trim().isEmpty) return;

    _cancelled = false;
    _client ??= http.Client();

    final uri = Uri.parse(kQweenTtsEndpoint);
    final body = {
      'model': kQweenTtsModel,
      'input': {'text': text},
      'parameters': {
        'voice': _voice,
        'language_type': 'English',
        'instructions': _instructions.trim().isEmpty ? kQweenDefaultInstructions : _instructions,
        'optimize_instructions': true,
      },
      'stream': true,
    };

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
        throw Exception('Qween TTS failed: ${response.statusCode} - $bodyStr');
      }

      await for (final line in response.stream
          .transform(utf8.decoder)
          .transform(const LineSplitter())) {
        if (_cancelled) return;
        if (!line.startsWith('data: ')) continue;

        final data = line.substring(6).trim();
        if (data == '[DONE]' || data.isEmpty) continue;

        try {
          final json = jsonDecode(data) as Map<String, dynamic>;
          final output = json['output'] as Map<String, dynamic>?;
          final audio = output?['audio'] as String?;
          if (audio != null && audio.isNotEmpty) {
            final decoded = base64Decode(audio);
            if (decoded.isNotEmpty) yield decoded;
          }
        } catch (_) {
          // Skip malformed SSE data
        }
      }
    } finally {
      if (_cancelled) {
        _client?.close();
        _client = null;
      }
    }
  }

  /// Cancel any in-flight request.
  void cancel() {
    _cancelled = true;
    _client?.close();
    _client = null;
  }

  void dispose() {
    cancel();
  }
}
