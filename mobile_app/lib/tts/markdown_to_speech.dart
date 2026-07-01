import 'package:markdown/markdown.dart' as md;

import 'speech_config.dart';

/// Converts markdown AST nodes into plain text suitable for TTS.
class MarkdownToSpeechConverter {
  MarkdownToSpeechConverter({SpeechConfig config = SpeechConfig.defaults})
      : _config = config;

  final SpeechConfig _config;

  static final _document = md.Document(
    extensionSet: md.ExtensionSet.gitHubFlavored,
  );

  /// Converts [markdown] to speech-friendly plain text.
  String convert(String markdown) {
    final trimmed = markdown.trim();
    if (trimmed.isEmpty) return '';

    final lines = trimmed.split('\n');
    final nodes = _document.parseLines(lines);
    final buffer = StringBuffer();
    _visitNodes(nodes, buffer, blockSep: '\n\n');
    return _postProcess(buffer.toString());
  }

  void _visitNodes(List<md.Node> nodes, StringBuffer buffer, {String blockSep = ' '}) {
    var first = true;
    for (final node in nodes) {
      if (!first) {
        buffer.write(blockSep);
      }
      final before = buffer.length;
      _visitNode(node, buffer);
      if (buffer.length > before) {
        first = false;
      } else {
        first = true;
      }
    }
  }

  void _visitNode(md.Node node, StringBuffer buffer) {
    if (node is md.Text) {
      buffer.write(node.text);
      return;
    }

    if (node is! md.Element) return;

    switch (node.tag) {
      case 'hr':
        return;
      case 'img':
        _visitImage(node, buffer);
      case 'a':
        _visitChildren(node, buffer);
      case 'pre':
        _visitCodeBlock(node, buffer);
      case 'code':
        _visitInlineCode(node, buffer);
      case 'table':
        _visitTable(node, buffer);
      case 'del':
        _visitChildren(node, buffer);
      case 'br':
        buffer.write(' ');
      case 'p':
      case 'h1':
      case 'h2':
      case 'h3':
      case 'h4':
      case 'h5':
      case 'h6':
      case 'li':
      case 'blockquote':
      case 'strong':
      case 'em':
      case 'thead':
      case 'tbody':
      case 'tr':
      case 'th':
      case 'td':
      case 'ul':
      case 'ol':
      case 'dl':
      case 'dt':
      case 'dd':
        _visitChildren(node, buffer);
      default:
        if (_isHtmlTag(node.tag)) {
          return;
        }
        _visitChildren(node, buffer);
    }
  }

  void _visitChildren(md.Element element, StringBuffer buffer) {
    final children = element.children;
    if (children == null || children.isEmpty) return;
    _visitNodes(children, buffer, blockSep: ' ');
  }

  void _visitImage(md.Element element, StringBuffer buffer) {
    if (!_config.imageAltOnly) return;
    final alt = element.attributes['alt']?.trim();
    if (alt != null && alt.isNotEmpty) {
      buffer.write(alt);
    }
  }

  void _visitCodeBlock(md.Element element, StringBuffer buffer) {
    if (_config.codeBlockHint.isNotEmpty) {
      buffer.write(_config.codeBlockHint);
    }
  }

  void _visitInlineCode(md.Element element, StringBuffer buffer) {
    if (!_config.speakInlineCode) return;
    final children = element.children;
    if (children == null) return;
    for (final child in children) {
      if (child is md.Text) {
        buffer.write(child.text);
      }
    }
  }

  void _visitTable(md.Element table, StringBuffer buffer) {
    final headerCells = <String>[];
    final bodyRows = <List<String>>[];

    for (final section in table.children ?? const <md.Node>[]) {
      if (section is! md.Element) continue;
      for (final row in section.children ?? const <md.Node>[]) {
        if (row is! md.Element || row.tag != 'tr') continue;
        final cells = _tableRowCells(row);
        if (cells.isEmpty) continue;
        if (_isSeparatorRow(cells)) continue;
        if (section.tag == 'thead') {
          headerCells
            ..clear()
            ..addAll(cells);
        } else {
          bodyRows.add(cells);
        }
      }
    }

    if (headerCells.isEmpty && bodyRows.isEmpty) return;

    buffer.write(_config.tablePrefix);
    buffer.write(' ');

    if (headerCells.isNotEmpty) {
      buffer.write(_config.tableRowPrefix);
      buffer.write(' ');
      buffer.write(_formatTableRow(headerCells, headerCells));
      buffer.write('.');
    }

    for (final row in bodyRows) {
      buffer.write(' ');
      buffer.write(_config.tableRowPrefix);
      buffer.write(' ');
      buffer.write(_formatTableRow(row, headerCells));
      buffer.write('.');
    }
  }

  List<String> _tableRowCells(md.Element row) {
    final cells = <String>[];
    for (final cell in row.children ?? const <md.Node>[]) {
      if (cell is! md.Element) continue;
      if (cell.tag != 'td' && cell.tag != 'th') continue;
      final cellBuffer = StringBuffer();
      _visitChildren(cell, cellBuffer);
      cells.add(cellBuffer.toString().trim());
    }
    return cells;
  }

  bool _isSeparatorRow(List<String> cells) {
    if (cells.isEmpty) return false;
    return cells.every((cell) {
      final stripped = cell.replaceAll('|', '').replaceAll(' ', '');
      return stripped.isNotEmpty && RegExp(r'^[-:]+$').hasMatch(stripped);
    });
  }

  String _formatTableRow(List<String> cells, List<String> headers) {
    final parts = <String>[];
    for (var i = 0; i < cells.length; i++) {
      final value = cells[i].trim();
      if (value.isEmpty) continue;
      if (headers.isNotEmpty && i < headers.length) {
        final header = headers[i].trim();
        if (header.isNotEmpty) {
          parts.add('$header $value');
          continue;
        }
      }
      parts.add(value);
    }
    return parts.join(', ');
  }

  bool _isHtmlTag(String tag) {
    return tag == 'html' ||
        tag == 'head' ||
        tag == 'body' ||
        tag == 'script' ||
        tag == 'style' ||
        tag == 'iframe' ||
        tag == 'svg';
  }

  String _postProcess(String text) {
    var result = text;

    result = result.replaceAll(
      RegExp(r'^\[[^\]]+\]:\s+\S+\s*$', multiLine: true),
      '',
    );
    result = result.replaceAll(RegExp(r'https?://\S+'), '');
    result = result.replaceAllMapped(
      RegExp(r'<(https?://[^>]+)>'),
      (_) => '',
    );

    result = result.replaceAllMapped(
      RegExp(r'\$(\d)\b'),
      (match) => 'group ${match.group(1)}',
    );
    result = result.replaceAllMapped(
      RegExp(r'\$(\d{2,}(?:\.\d+)?)\b'),
      (match) => '${match.group(1)} dollars',
    );
    result = result.replaceAllMapped(
      RegExp(r'\$([^$\s]+)\$'),
      (match) => match.group(1) ?? '',
    );

    result = result.replaceAll(RegExp(r'\n{3,}'), '\n\n');
    result = result.replaceAll(RegExp(r'[ \t]+'), ' ');
    result = result.replaceAll(RegExp(r' *\n *'), '\n');
    result = result.replaceAll(RegExp(r'\n+'), '. ');
    result = result.replaceAll(RegExp(r'\.(\s*\.)+'), '. ');
    result = result.replaceAll(RegExp(r'\s+\.'), '.');

    return result.trim();
  }
}
