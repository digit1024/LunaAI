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

/// Qwen TTS sample rate (24 kHz).
const int kQweenTtsSampleRate = 24000;

/// Client for Qwen TTS API.
/// Supports both non-streaming (full response) and streaming (SSE).
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

  /// Synthesize text. Returns complete audio bytes (MP3).
  /// Falls back to trying multiple response formats.
  Future<Uint8List?> synthesize(String text) async {
    if (text.trim().isEmpty) return null;

    _cancelled = false;
    _client = http.Client();

    final uri = Uri.parse(kQweenTtsEndpoint);
    final body = {
      'model': kQweenTtsModel,
      'input': {'text': text},
      'parameters': {
        'voice': _voice,
        'language_type': 'English',
        'response_format': 'mp3',
        'sample_rate': kQweenTtsSampleRate,
        'instructions': _instructions.trim().isEmpty
            ? kQweenDefaultInstructions
            : _instructions,
        'optimize_instructions': true,
      },
    };

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

      debugPrint('Qween TTS: status=${response.statusCode}');
      debugPrint('Qween TTS: content-type=${response.headers['content-type']}');

      if (response.statusCode != 200) {
        debugPrint('Qween TTS: error body=${response.body.substring(0, (response.body.length).clamp(0, 500))}');
        return null;
      }

      final contentType = response.headers['content-type'] ?? '';

      // If content-type is audio, the body IS the audio bytes
      if (contentType.contains('audio/')) {
        debugPrint('Qween TTS: got direct audio bytes (${response.bodyBytes.length} bytes)');
        return response.bodyBytes;
      }

      // Otherwise parse JSON response
      final responseBody = response.body;
      debugPrint('Qween TTS: json response (first 500 chars)=${responseBody.substring(0, responseBody.length.clamp(0, 500))}');

      final json = jsonDecode(responseBody) as Map<String, dynamic>;

      // Try multiple possible response paths:
      // Path 1: output.audio (direct base64 audio)
      final output = json['output'] as Map<String, dynamic>?;
      String? audioBase64 = output?['audio'] as String?;

      // Path 2: output.choices[0].message.content[0].audio
      if (audioBase64 == null && output != null) {
        final choices = output['choices'] as List<dynamic>?;
        if (choices != null && choices.isNotEmpty) {
          final firstChoice = choices[0] as Map<String, dynamic>;
          final message = firstChoice['message'] as Map<String, dynamic>?;
          final content = message?['content'];
          if (content is List && content.isNotEmpty) {
            for (final item in content) {
              if (item is Map<String, dynamic>) {
                audioBase64 = item['audio'] as String? ??
                    item['audio_content'] as String?;
                if (audioBase64 != null) break;
              }
            }
          } else if (content is String) {
            // Maybe the content itself is base64 audio
            audioBase64 = content;
          }
        }
      }

      // Path 3: output.preview_audio.data (voice design format)
      if (audioBase64 == null && output != null) {
        final previewAudio = output['preview_audio'] as Map<String, dynamic>?;
        audioBase64 = previewAudio?['data'] as String?;
      }

      // Path 4: data field at root level
      audioBase64 ??= json['data'] as String?;

      if (audioBase64 != null && audioBase64.isNotEmpty) {
        debugPrint('Qween TTS: decoded base64 audio (${audioBase64.length} chars)');
        return base64Decode(audioBase64);
      }

      debugPrint('Qween TTS: no audio found in response. Keys: ${json.keys.toList()}');
      if (output != null) {
        debugPrint('Qween TTS: output keys: ${output.keys.toList()}');
      }
      return null;
    } catch (e) {
      if (!_cancelled) {
        debugPrint('Qween TTS error: $e');
      }
      return null;
    } finally {
      _client?.close();
      _client = null;
    }
  }

  /// Synthesize text via SSE streaming. Yields audio chunks as they arrive.
  Stream<List<int>> synthesizeStream(String text) async* {
    if (text.trim().isEmpty) return;

    _cancelled = false;
    _client = http.Client();

    final uri = Uri.parse(kQweenTtsEndpoint);
    final body = {
      'model': kQweenTtsModel,
      'input': {'text': text},
      'parameters': {
        'voice': _voice,
        'language_type': 'English',
        'response_format': 'pcm',
        'sample_rate': kQweenTtsSampleRate,
        'instructions': _instructions.trim().isEmpty
            ? kQweenDefaultInstructions
            : _instructions,
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

      debugPrint('Qween TTS stream: status=${response.statusCode}');

      if (response.statusCode != 200) {
        final bodyStr = await response.stream.bytesToString();
        debugPrint('Qween TTS stream error: $bodyStr');
        return;
      }

      await for (final line in response.stream
          .transform(utf8.decoder)
          .transform(const LineSplitter())) {
        if (_cancelled) return;

        // Log first few lines for debugging
        if (line.isNotEmpty) {
          debugPrint('Qween SSE line: ${line.substring(0, line.length.clamp(0, 200))}');
        }

        if (!line.startsWith('data:')) continue;

        final data = line.substring(5).trim();
        if (data == '[DONE]' || data.isEmpty) continue;

        try {
          final json = jsonDecode(data) as Map<String, dynamic>;

          // Try multiple paths for audio data in SSE chunks
          String? audioB64;
          final output = json['output'] as Map<String, dynamic>?;
          audioB64 = output?['audio'] as String?;

          // choices path
          if (audioB64 == null) {
            final choices = (output?['choices'] ?? json['choices']) as List<dynamic>?;
            if (choices != null && choices.isNotEmpty) {
              final choice = choices[0] as Map<String, dynamic>;
              final message = choice['message'] as Map<String, dynamic>?;
              final content = message?['content'];
              if (content is List && content.isNotEmpty) {
                for (final item in content) {
                  if (item is Map<String, dynamic>) {
                    audioB64 = item['audio'] as String? ??
                        item['audio_content'] as String?;
                    if (audioB64 != null) break;
                  }
                }
              }
            }
          }

          // delta path (OpenAI-like streaming)
          if (audioB64 == null) {
            final delta = json['delta'] as String?;
            audioB64 = delta;
          }

          if (audioB64 != null && audioB64.isNotEmpty) {
            final decoded = base64Decode(audioB64);
            if (decoded.isNotEmpty) yield decoded;
          }
        } catch (e) {
          debugPrint('Qween SSE parse error: $e');
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
