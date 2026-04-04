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

  /// Whether [e] is an HTTP-level failure we should not mask with an HTTP fallback.
  static bool _isHttpLevelFailure(Object e) {
    final s = e.toString();
    return s.contains('File upload failed:') ||
        s.contains('File removal failed:') ||
        s.contains('Invalid response:');
  }

  Future<UploadResult> _multipartUpload(
    Uri apiBase,
    File file,
    String? conversationId,
  ) async {
    final uri = apiBase.resolve('/api/attach-file');
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
    }
    throw Exception(
      'File upload failed: ${response.statusCode} - $responseBody',
    );
  }

  Future<void> _deleteUpload(Uri apiBase, String uid) async {
    final uri = apiBase.resolve('/api/attach-file/$uid');
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
  }

  /// Upload a file; returns uid, original_name, stored_path for the upload-notification message.
  /// [conversationId] optional; uploads go under uploads/{conversationId}/{uid}.{ext}.
  ///
  /// Tries HTTPS first (same host/port as WSS), then HTTP for local servers — avoids sending
  /// plain HTTP to TLS-only fronts (e.g. Cloudflare).
  Future<UploadResult> uploadFile(
    File file, {
    String? conversationId,
  }) async {
    try {
      return await _multipartUpload(
        config.httpBaseUriSecure(),
        file,
        conversationId,
      );
    } catch (e) {
      if (_isHttpLevelFailure(e)) {
        debugPrint('Error uploading file: $e');
        rethrow;
      }
      debugPrint('HTTPS upload failed ($e), trying HTTP...');
      try {
        return await _multipartUpload(
          config.httpBaseUriInsecure(),
          file,
          conversationId,
        );
      } catch (e2) {
        debugPrint('Error uploading file: $e2');
        rethrow;
      }
    }
  }

  /// Remove an uploaded file by uid
  Future<void> removeFile(String uid) async {
    try {
      await _deleteUpload(config.httpBaseUriSecure(), uid);
    } catch (e) {
      if (_isHttpLevelFailure(e)) {
        debugPrint('Error removing file: $e');
        rethrow;
      }
      debugPrint('HTTPS delete failed ($e), trying HTTP...');
      try {
        await _deleteUpload(config.httpBaseUriInsecure(), uid);
      } catch (e2) {
        debugPrint('Error removing file: $e2');
        rethrow;
      }
    }
  }
}

