import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:http/http.dart' as http;

import '../voice/audio_chunker.dart';
import '../voice/voice_catalog.dart';
import '../voice/voice_models.dart';
import '../voice/voice_preferences.dart';

class SttException implements Exception {
  SttException(this.message);
  final String message;

  @override
  String toString() => message;
}

class SttService {
  SttService(this._ref);

  final Ref _ref;

  Future<String> transcribeFile(String path) async {
    final prefs = _ref.read(voicePreferencesProvider);
    final config = prefs.stt;
    final provider = VoiceCatalog.provider(config.providerId);
    final preset = VoiceCatalog.sttModel(config.providerId, config.modelId);

    final endpoint = config.endpointOverride.isNotEmpty
        ? config.endpointOverride
        : (provider.sttEndpoint ?? '');
    final model = config.modelOverride.isNotEmpty
        ? config.modelOverride
        : preset.model;
    if (endpoint.isEmpty || model.isEmpty) {
      throw SttException('STT endpoint or model is not configured');
    }

    String? apiKey;
    if (provider.requiresApiKey) {
      apiKey =
          await _ref.read(voicePreferencesProvider.notifier).getApiKey(config.providerId);
      if (apiKey == null || apiKey.trim().isEmpty) {
        throw SttException('STT provider API key is not configured');
      }
    }

    final file = File(path);
    if (!await file.exists()) {
      throw SttException('Recording file is missing');
    }
    final bytes = await file.readAsBytes();
    if (bytes.isEmpty) {
      throw SttException('Recording is empty');
    }

    final chunks = AudioChunker.chunkWav(
      bytes,
      maxSeconds: preset.maxAudioSeconds,
      overlapSeconds: preset.maxAudioSeconds <= 60 ? 1.0 : 2.0,
    );

    final parts = <String>[];
    try {
      for (final chunk in chunks) {
        final text = provider.sttProtocol == VoiceApiProtocol.dashScopeAsr
            ? await _transcribeDashScope(
                chunk: chunk,
                endpoint: endpoint,
                model: model,
                apiKey: apiKey,
                language: prefs.language,
                vocabulary: prefs.vocabulary,
                extraParams: config.extraParamsJson,
              )
            : await _transcribeOpenAi(
                chunk: chunk,
                endpoint: endpoint,
                model: model,
                apiKey: apiKey,
                language: prefs.language,
                vocabulary: prefs.vocabulary,
                supportsVocabularyHint: preset.supportsVocabularyHint,
                extraParams: config.extraParamsJson,
              );
        if (text.trim().isNotEmpty) parts.add(text.trim());
      }
    } finally {
      try {
        if (await file.exists()) await file.delete();
      } catch (_) {}
    }

    final merged = AudioChunker.mergeTranscripts(parts);
    if (merged.trim().isEmpty) {
      throw SttException('No speech detected');
    }
    return merged.trim();
  }

  Future<String> _transcribeOpenAi({
    required Uint8List chunk,
    required String endpoint,
    required String model,
    required String? apiKey,
    required String language,
    required List<String> vocabulary,
    required bool supportsVocabularyHint,
    required String extraParams,
  }) async {
    final request = http.MultipartRequest('POST', Uri.parse(endpoint));
    if (apiKey != null && apiKey.isNotEmpty) {
      request.headers['Authorization'] = 'Bearer $apiKey';
    }
    request.fields['model'] = model;
    request.fields['language'] =
        VoicePreferencesNotifier.languageToIso(language);
    request.fields['response_format'] = 'json';
    request.fields['temperature'] = '0';
    if (supportsVocabularyHint && vocabulary.isNotEmpty) {
      request.fields['prompt'] = 'Expected vocabulary: ${vocabulary.join(', ')}';
    }
    for (final entry in _parseExtraParams(extraParams).entries) {
      final value = entry.value;
      if (value is String || value is num || value is bool) {
        request.fields[entry.key] = value.toString();
      }
    }
    request.files.add(
      http.MultipartFile.fromBytes(
        'file',
        chunk,
        filename: 'audio.wav',
      ),
    );

    final streamed = await request.send();
    final response = await http.Response.fromStream(streamed);
    if (response.statusCode != 200) {
      throw SttException(
        'STT request failed (${response.statusCode}): ${_short(response.body)}',
      );
    }
    final json = _decodeJson(response.body);
    final text = json['text'];
    if (text is String) return text;
    throw SttException('STT response did not contain text');
  }

  Future<String> _transcribeDashScope({
    required Uint8List chunk,
    required String endpoint,
    required String model,
    required String? apiKey,
    required String language,
    required List<String> vocabulary,
    required String extraParams,
  }) async {
    final dataUri = 'data:audio/wav;base64,${base64Encode(chunk)}';
    final messages = <Map<String, dynamic>>[];
    if (vocabulary.isNotEmpty) {
      messages.add({
        'role': 'system',
        'content': [
          {'text': 'Expected vocabulary: ${vocabulary.join(', ')}'},
        ],
      });
    }
    messages.add({
      'role': 'user',
      'content': [
        {
          'type': 'input_audio',
          'input_audio': {'data': dataUri},
        },
      ],
    });

    final body = <String, dynamic>{
      'model': model,
      'messages': messages,
      'stream': false,
      'asr_options': {
        'language': VoicePreferencesNotifier.languageToIso(language),
        'enable_itn': false,
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

    if (response.statusCode != 200) {
      throw SttException(
        'STT request failed (${response.statusCode}): ${_short(response.body)}',
      );
    }

    final json = _decodeJson(response.body);
    final text = _extractText(json);
    if (text != null && text.trim().isNotEmpty) return text;
    throw SttException('STT response did not contain text');
  }

  static String? _extractText(Map<String, dynamic> json) {
    String? fromMessage(Object? message) {
      if (message is! Map) return null;
      final content = message['content'];
      if (content is String && content.trim().isNotEmpty) return content;
      if (content is List) {
        final buffer = StringBuffer();
        for (final part in content) {
          if (part is Map && part['text'] is String) {
            buffer.write(part['text']);
          }
        }
        if (buffer.toString().trim().isNotEmpty) return buffer.toString();
      }
      if (message['text'] is String) return message['text'] as String;
      return null;
    }

    final choices = json['choices'];
    if (choices is List && choices.isNotEmpty) {
      final first = choices.first;
      final text = fromMessage(first is Map ? first['message'] : null);
      if (text != null) return text;
    }

    final output = json['output'];
    if (output is Map) {
      final outputChoices = output['choices'];
      if (outputChoices is List && outputChoices.isNotEmpty) {
        final first = outputChoices.first;
        final text = fromMessage(first is Map ? first['message'] : null);
        if (text != null) return text;
      }
    }
    return null;
  }

  static Map<String, dynamic> _decodeJson(String body) {
    try {
      final decoded = jsonDecode(body);
      if (decoded is Map<String, dynamic>) return decoded;
      if (decoded is Map) return decoded.cast<String, dynamic>();
    } catch (e) {
      debugPrint('STT: invalid JSON response: $e');
    }
    throw SttException('STT returned an invalid response');
  }

  static Map<String, dynamic> _parseExtraParams(String raw) {
    if (raw.trim().isEmpty) return const {};
    try {
      final decoded = jsonDecode(raw);
      if (decoded is Map<String, dynamic>) return decoded;
      if (decoded is Map) return decoded.cast<String, dynamic>();
    } catch (e) {
      debugPrint('STT: ignoring invalid extra params JSON: $e');
    }
    return const {};
  }

  static String _short(String value) {
    final trimmed = value.trim();
    if (trimmed.length <= 300) return trimmed;
    return '${trimmed.substring(0, 300)}…';
  }
}

final sttServiceProvider = Provider<SttService>((ref) => SttService(ref));
