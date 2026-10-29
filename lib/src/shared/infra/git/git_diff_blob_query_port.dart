import 'dart:typed_data';

import 'package:alera/src/shared/infra/git/git_diff_models.dart';

/// Read-only capability for loading individual binary/text diff sides.
/// UI workers do not require branch mutations or the full GitBackend.
abstract interface class GitDiffBlobQueryPort {
  Future<Uint8List?> fileRevisionBlobBytes({
    required String path,
    required String filePath,
    required String revisionOid,
    required bool oldSide,
  });

  Future<Uint8List?> diffBlobBytes({
    required String path,
    required String filePath,
    String? oldPath,
    GitChangeArea? area,
    String? commitOid,
    String? parentOid,
    required bool oldSide,
  });
}
