import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_sound/flutter_sound.dart';
import 'package:path_provider/path_provider.dart';
import 'package:permission_handler/permission_handler.dart';

class VoiceRecorderException implements Exception {
  VoiceRecorderException(this.message);
  final String message;

  @override
  String toString() => message;
}

/// Records 16 kHz mono PCM16 WAV — accepted by all catalog STT providers.
class VoiceRecorderService {
  VoiceRecorderService();

  final FlutterSoundRecorder _recorder = FlutterSoundRecorder();
  bool _opened = false;
  String? _currentPath;

  bool get isRecording => _opened && _recorder.isRecording;

  Future<void> _ensureOpen() async {
    if (_opened) return;
    await _recorder.openRecorder();
    _opened = true;
  }

  Future<String> start() async {
    final status = await Permission.microphone.request();
    if (!status.isGranted) {
      throw VoiceRecorderException('Microphone permission denied');
    }

    await _ensureOpen();
    if (_recorder.isRecording) {
      await _recorder.stopRecorder();
    }

    final dir = await getTemporaryDirectory();
    final path =
        '${dir.path}/luna_voice_${DateTime.now().millisecondsSinceEpoch}.wav';

    try {
      await _recorder.startRecorder(
        toFile: path,
        codec: Codec.pcm16WAV,
        sampleRate: 16000,
        numChannels: 1,
        audioSource: AudioSource.microphone,
        enableNoiseSuppression: true,
        enableEchoCancellation: true,
      );
    } catch (e) {
      debugPrint('VoiceRecorder: start failed: $e');
      throw VoiceRecorderException('Could not start recording');
    }

    _currentPath = path;
    return path;
  }

  Future<String?> stop() async {
    final path = _currentPath;
    try {
      final result = await _recorder.stopRecorder();
      _currentPath = null;
      return result ?? path;
    } catch (e) {
      debugPrint('VoiceRecorder: stop failed: $e');
      _currentPath = null;
      return path;
    }
  }

  Future<void> cancel() async {
    final path = _currentPath;
    _currentPath = null;
    try {
      if (_opened && _recorder.isRecording) {
        await _recorder.stopRecorder();
      }
    } catch (_) {}
    if (path != null) {
      try {
        final file = File(path);
        if (await file.exists()) await file.delete();
      } catch (_) {}
    }
  }

  Future<void> dispose() async {
    try {
      if (_opened && _recorder.isRecording) {
        await _recorder.stopRecorder();
      }
      if (_opened) {
        await _recorder.closeRecorder();
      }
    } catch (_) {}
    _opened = false;
    _currentPath = null;
  }
}
