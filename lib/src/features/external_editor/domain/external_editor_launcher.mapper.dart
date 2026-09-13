// coverage:ignore-file
// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
// ignore_for_file: type=lint
// ignore_for_file: invalid_use_of_protected_member
// ignore_for_file: unused_element, unnecessary_cast, override_on_non_overriding_member
// ignore_for_file: strict_raw_type, inference_failure_on_untyped_parameter

part of 'external_editor_launcher.dart';

class ExternalEditorKindMapper extends EnumMapper<ExternalEditorKind> {
  ExternalEditorKindMapper._();

  static ExternalEditorKindMapper? _instance;
  static ExternalEditorKindMapper ensureInitialized() {
    if (_instance == null) {
      MapperContainer.globals.use(_instance = ExternalEditorKindMapper._());
    }
    return _instance!;
  }

  static ExternalEditorKind fromValue(dynamic value) {
    ensureInitialized();
    return MapperContainer.globals.fromValue(value);
  }

  @override
  ExternalEditorKind decode(dynamic value) {
    switch (value) {
      case r'zed':
        return ExternalEditorKind.zed;
      case r'vscode':
        return ExternalEditorKind.vscode;
      default:
        throw MapperException.unknownEnumValue(value);
    }
  }

  @override
  dynamic encode(ExternalEditorKind self) {
    switch (self) {
      case ExternalEditorKind.zed:
        return r'zed';
      case ExternalEditorKind.vscode:
        return r'vscode';
    }
  }
}

extension ExternalEditorKindMapperExtension on ExternalEditorKind {
  String toValue() {
    ExternalEditorKindMapper.ensureInitialized();
    return MapperContainer.globals.toValue<ExternalEditorKind>(this) as String;
  }
}

class CodeOpenTargetMapper extends EnumMapper<CodeOpenTarget> {
  CodeOpenTargetMapper._();

  static CodeOpenTargetMapper? _instance;
  static CodeOpenTargetMapper ensureInitialized() {
    if (_instance == null) {
      MapperContainer.globals.use(_instance = CodeOpenTargetMapper._());
    }
    return _instance!;
  }

  static CodeOpenTarget fromValue(dynamic value) {
    ensureInitialized();
    return MapperContainer.globals.fromValue(value);
  }

  @override
  CodeOpenTarget decode(dynamic value) {
    switch (value) {
      case r'alera':
        return CodeOpenTarget.alera;
      case r'external':
        return CodeOpenTarget.external;
      default:
        throw MapperException.unknownEnumValue(value);
    }
  }

  @override
  dynamic encode(CodeOpenTarget self) {
    switch (self) {
      case CodeOpenTarget.alera:
        return r'alera';
      case CodeOpenTarget.external:
        return r'external';
    }
  }
}

extension CodeOpenTargetMapperExtension on CodeOpenTarget {
  String toValue() {
    CodeOpenTargetMapper.ensureInitialized();
    return MapperContainer.globals.toValue<CodeOpenTarget>(this) as String;
  }
}

class ExternalEditorWorkspaceModeMapper
    extends EnumMapper<ExternalEditorWorkspaceMode> {
  ExternalEditorWorkspaceModeMapper._();

  static ExternalEditorWorkspaceModeMapper? _instance;
  static ExternalEditorWorkspaceModeMapper ensureInitialized() {
    if (_instance == null) {
      MapperContainer.globals.use(
        _instance = ExternalEditorWorkspaceModeMapper._(),
      );
    }
    return _instance!;
  }

  static ExternalEditorWorkspaceMode fromValue(dynamic value) {
    ensureInitialized();
    return MapperContainer.globals.fromValue(value);
  }

  @override
  ExternalEditorWorkspaceMode decode(dynamic value) {
    switch (value) {
      case r'newWindow':
        return ExternalEditorWorkspaceMode.newWindow;
      case r'defaultWindow':
        return ExternalEditorWorkspaceMode.defaultWindow;
      default:
        throw MapperException.unknownEnumValue(value);
    }
  }

  @override
  dynamic encode(ExternalEditorWorkspaceMode self) {
    switch (self) {
      case ExternalEditorWorkspaceMode.newWindow:
        return r'newWindow';
      case ExternalEditorWorkspaceMode.defaultWindow:
        return r'defaultWindow';
    }
  }
}

extension ExternalEditorWorkspaceModeMapperExtension
    on ExternalEditorWorkspaceMode {
  String toValue() {
    ExternalEditorWorkspaceModeMapper.ensureInitialized();
    return MapperContainer.globals.toValue<ExternalEditorWorkspaceMode>(this)
        as String;
  }
}
