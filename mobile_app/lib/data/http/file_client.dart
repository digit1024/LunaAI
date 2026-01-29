import 'dart:convert';
import 'dart:io';

import 'package:http/http.dart' as http;
import 'package:flutter/foundation.dart';

import '../../core/config/server_config.dart';

class FileAttachment {
  final String fileId;
  final String fileName;
  final String mimeType;
  final int fileSize;
  final File file;

  FileAttachment({
    required this.fileId,
    required this.fileName,
    required this.mimeType,
    required this.fileSize,
    required this.file,
  });
}

/// Response from POST /api/attach-file.
class UploadResult {
  final String uid;
  final String originalName;
  /// Full path on server (config_dir/uploads/{uid}.{ext})
  final String storedPath;

  UploadResult({
    required this.uid,
    required this.originalName,
    required this.storedPath,
  });
}

class FileClient {
  final ServerConfig config;

  FileClient(this.config);

  /// HTTP API – same port as WebSocket
  Uri get baseUrl => Uri(
        scheme: 'http',
        host: config.host,
        port: config.port,
      );

  /// Upload a file; returns uid, original_name, stored_path for the upload-notification message.
  /// [conversationId] optional; uploads go under uploads/{conversationId}/{uid}.{ext}.
  Future<UploadResult> uploadFile(
    File file, {
    String? conversationId,
  }) async {
    try {
      final uri = baseUrl.resolve('/api/attach-file');

      final request = http.MultipartRequest('POST', uri);
      request.headers['x-api-key'] = config.apiKey;
      request.headers['authorization'] = 'Bearer ${config.apiKey}';

      if (conversationId != null && conversationId.isNotEmpty) {
        request.fields['conversation_id'] = conversationId;
      }

      final fileStream = http.ByteStream(file.openRead());
      final fileLength = await file.length();
      final multipartFile = http.MultipartFile(
        'file',
        fileStream,
        fileLength,
        filename: file.path.split(Platform.pathSeparator).last,
      );
      request.files.add(multipartFile);

      final response = await request.send();
      final responseBody = await response.stream.bytesToString();

      if (response.statusCode == 200) {
        final json = jsonDecode(responseBody) as Map<String, dynamic>;
        final uid = json['uid'] as String?;
        final originalName = json['original_name'] as String? ??
            file.path.split(Platform.pathSeparator).last;
        final storedPath = json['stored_path'] as String?;
        if (uid == null || storedPath == null) {
          throw Exception('Invalid response: missing uid or stored_path');
        }
        return UploadResult(
          uid: uid,
          originalName: originalName,
          storedPath: storedPath,
        );
      } else {
        throw Exception(
          'File upload failed: ${response.statusCode} - $responseBody',
        );
      }
    } catch (e) {
      debugPrint('Error uploading file: $e');
      rethrow;
    }
  }

  /// Remove an uploaded file by uid
  Future<void> removeFile(String uid) async {
    try {
      final uri = baseUrl.resolve('/api/attach-file/$uid');
      
      final response = await http.delete(
        uri,
        headers: {
          'x-api-key': config.apiKey,
          'authorization': 'Bearer ${config.apiKey}',
        },
      );

      if (response.statusCode != 200) {
        throw Exception(
          'File removal failed: ${response.statusCode} - ${response.body}',
        );
      }
    } catch (e) {
      debugPrint('Error removing file: $e');
      rethrow;
    }
  }
}

