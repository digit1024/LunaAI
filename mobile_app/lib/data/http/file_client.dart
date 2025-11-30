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

class FileClient {
  final ServerConfig config;

  FileClient(this.config);

  /// Get HTTP base URL (port + 1 from WebSocket port)
  Uri get baseUrl => Uri(
        scheme: 'http',
        host: config.host,
        port: config.port + 1,
      );

  /// Upload a file and get its attachment ID
  Future<FileAttachment> uploadFile(File file) async {
    try {
      final uri = baseUrl.resolve('/api/attach-file');
      
      final request = http.MultipartRequest('POST', uri);
      request.headers['x-api-key'] = config.apiKey;
      request.headers['authorization'] = 'Bearer ${config.apiKey}';
      
      // Add file to request
      final fileStream = http.ByteStream(file.openRead());
      final fileLength = await file.length();
      final multipartFile = http.MultipartFile(
        'file',
        fileStream,
        fileLength,
        filename: file.path.split('/').last,
      );
      request.files.add(multipartFile);

      final response = await request.send();
      final responseBody = await response.stream.bytesToString();

      if (response.statusCode == 200) {
        // Parse JSON response
        final json = jsonDecode(responseBody) as Map<String, dynamic>;
        
        final fileId = json['file_id'] as String?;
        if (fileId == null) {
          throw Exception('Invalid response: missing file_id');
        }

        return FileAttachment(
          fileId: fileId,
          fileName: json['file_name'] as String? ?? file.path.split('/').last,
          mimeType: json['mime_type'] as String? ?? 'application/octet-stream',
          fileSize: (json['file_size'] as num?)?.toInt() ?? fileLength,
          file: file,
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

  /// Remove an attached file
  Future<void> removeFile(String fileId) async {
    try {
      final uri = baseUrl.resolve('/api/attach-file/$fileId');
      
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

