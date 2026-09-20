// coverage:ignore-file
// GENERATED CODE - DO NOT MODIFY BY HAND
// dart format off
// ignore_for_file: type=lint
// ignore_for_file: invalid_use_of_protected_member
// ignore_for_file: unused_element, unnecessary_cast, override_on_non_overriding_member
// ignore_for_file: strict_raw_type, inference_failure_on_untyped_parameter

part of 'language_intelligence_settings.dart';

class LanguageActivationSettingsMapper
    extends ClassMapperBase<LanguageActivationSettings> {
  LanguageActivationSettingsMapper._();

  static LanguageActivationSettingsMapper? _instance;
  static LanguageActivationSettingsMapper ensureInitialized() {
    if (_instance == null) {
      MapperContainer.globals.use(
        _instance = LanguageActivationSettingsMapper._(),
      );
    }
    return _instance!;
  }

  @override
  final String id = 'LanguageActivationSettings';

  static bool _$enabled(LanguageActivationSettings v) => v.enabled;
  static const Field<LanguageActivationSettings, bool> _f$enabled = Field(
    'enabled',
    _$enabled,
    opt: true,
    def: false,
  );
  static bool _$structuralParserEnabled(LanguageActivationSettings v) =>
      v.structuralParserEnabled;
  static const Field<LanguageActivationSettings, bool>
  _f$structuralParserEnabled = Field(
    'structuralParserEnabled',
    _$structuralParserEnabled,
    opt: true,
    def: false,
  );
  static String? _$semanticProviderId(LanguageActivationSettings v) =>
      v.semanticProviderId;
  static const Field<LanguageActivationSettings, String> _f$semanticProviderId =
      Field('semanticProviderId', _$semanticProviderId, opt: true);
  static String? _$executablePath(LanguageActivationSettings v) =>
      v.executablePath;
  static const Field<LanguageActivationSettings, String> _f$executablePath =
      Field('executablePath', _$executablePath, opt: true);
  static List<String> _$extraArgs(LanguageActivationSettings v) => v.extraArgs;
  static const Field<LanguageActivationSettings, List<String>> _f$extraArgs =
      Field('extraArgs', _$extraArgs, opt: true, def: const <String>[]);

  @override
  final MappableFields<LanguageActivationSettings> fields = const {
    #enabled: _f$enabled,
    #structuralParserEnabled: _f$structuralParserEnabled,
    #semanticProviderId: _f$semanticProviderId,
    #executablePath: _f$executablePath,
    #extraArgs: _f$extraArgs,
  };

  static LanguageActivationSettings _instantiate(DecodingData data) {
    return LanguageActivationSettings(
      enabled: data.dec(_f$enabled),
      structuralParserEnabled: data.dec(_f$structuralParserEnabled),
      semanticProviderId: data.dec(_f$semanticProviderId),
      executablePath: data.dec(_f$executablePath),
      extraArgs: data.dec(_f$extraArgs),
    );
  }

  @override
  final Function instantiate = _instantiate;

  static LanguageActivationSettings fromMap(Map<String, dynamic> map) {
    return ensureInitialized().decodeMap<LanguageActivationSettings>(map);
  }

  static LanguageActivationSettings fromJson(String json) {
    return ensureInitialized().decodeJson<LanguageActivationSettings>(json);
  }
}

mixin LanguageActivationSettingsMappable {
  String toJson() {
    return LanguageActivationSettingsMapper.ensureInitialized()
        .encodeJson<LanguageActivationSettings>(
          this as LanguageActivationSettings,
        );
  }

  Map<String, dynamic> toMap() {
    return LanguageActivationSettingsMapper.ensureInitialized()
        .encodeMap<LanguageActivationSettings>(
          this as LanguageActivationSettings,
        );
  }

  LanguageActivationSettingsCopyWith<
    LanguageActivationSettings,
    LanguageActivationSettings,
    LanguageActivationSettings
  >
  get copyWith =>
      _LanguageActivationSettingsCopyWithImpl<
        LanguageActivationSettings,
        LanguageActivationSettings
      >(this as LanguageActivationSettings, $identity, $identity);
  @override
  String toString() {
    return LanguageActivationSettingsMapper.ensureInitialized().stringifyValue(
      this as LanguageActivationSettings,
    );
  }

  @override
  bool operator ==(Object other) {
    return LanguageActivationSettingsMapper.ensureInitialized().equalsValue(
      this as LanguageActivationSettings,
      other,
    );
  }

  @override
  int get hashCode {
    return LanguageActivationSettingsMapper.ensureInitialized().hashValue(
      this as LanguageActivationSettings,
    );
  }
}

extension LanguageActivationSettingsValueCopy<$R, $Out>
    on ObjectCopyWith<$R, LanguageActivationSettings, $Out> {
  LanguageActivationSettingsCopyWith<$R, LanguageActivationSettings, $Out>
  get $asLanguageActivationSettings => $base.as(
    (v, t, t2) => _LanguageActivationSettingsCopyWithImpl<$R, $Out>(v, t, t2),
  );
}

abstract class LanguageActivationSettingsCopyWith<
  $R,
  $In extends LanguageActivationSettings,
  $Out
>
    implements ClassCopyWith<$R, $In, $Out> {
  ListCopyWith<$R, String, ObjectCopyWith<$R, String, String>> get extraArgs;
  $R call({
    bool? enabled,
    bool? structuralParserEnabled,
    String? semanticProviderId,
    String? executablePath,
    List<String>? extraArgs,
  });
  LanguageActivationSettingsCopyWith<$R2, $In, $Out2> $chain<$R2, $Out2>(
    Then<$Out2, $R2> t,
  );
}

class _LanguageActivationSettingsCopyWithImpl<$R, $Out>
    extends ClassCopyWithBase<$R, LanguageActivationSettings, $Out>
    implements
        LanguageActivationSettingsCopyWith<
          $R,
          LanguageActivationSettings,
          $Out
        > {
  _LanguageActivationSettingsCopyWithImpl(super.value, super.then, super.then2);

  @override
  late final ClassMapperBase<LanguageActivationSettings> $mapper =
      LanguageActivationSettingsMapper.ensureInitialized();
  @override
  ListCopyWith<$R, String, ObjectCopyWith<$R, String, String>> get extraArgs =>
      ListCopyWith(
        $value.extraArgs,
        (v, t) => ObjectCopyWith(v, $identity, t),
        (v) => call(extraArgs: v),
      );
  @override
  $R call({
    bool? enabled,
    bool? structuralParserEnabled,
    Object? semanticProviderId = $none,
    Object? executablePath = $none,
    List<String>? extraArgs,
  }) => $apply(
    FieldCopyWithData({
      if (enabled != null) #enabled: enabled,
      if (structuralParserEnabled != null)
        #structuralParserEnabled: structuralParserEnabled,
      if (semanticProviderId != $none) #semanticProviderId: semanticProviderId,
      if (executablePath != $none) #executablePath: executablePath,
      if (extraArgs != null) #extraArgs: extraArgs,
    }),
  );
  @override
  LanguageActivationSettings $make(CopyWithData data) =>
      LanguageActivationSettings(
        enabled: data.get(#enabled, or: $value.enabled),
        structuralParserEnabled: data.get(
          #structuralParserEnabled,
          or: $value.structuralParserEnabled,
        ),
        semanticProviderId: data.get(
          #semanticProviderId,
          or: $value.semanticProviderId,
        ),
        executablePath: data.get(#executablePath, or: $value.executablePath),
        extraArgs: data.get(#extraArgs, or: $value.extraArgs),
      );

  @override
  LanguageActivationSettingsCopyWith<$R2, LanguageActivationSettings, $Out2>
  $chain<$R2, $Out2>(Then<$Out2, $R2> t) =>
      _LanguageActivationSettingsCopyWithImpl<$R2, $Out2>($value, $cast, t);
}

class LanguageIntelligenceSettingsMapper
    extends ClassMapperBase<LanguageIntelligenceSettings> {
  LanguageIntelligenceSettingsMapper._();

  static LanguageIntelligenceSettingsMapper? _instance;
  static LanguageIntelligenceSettingsMapper ensureInitialized() {
    if (_instance == null) {
      MapperContainer.globals.use(
        _instance = LanguageIntelligenceSettingsMapper._(),
      );
      LanguageActivationSettingsMapper.ensureInitialized();
    }
    return _instance!;
  }

  @override
  final String id = 'LanguageIntelligenceSettings';

  static Map<String, LanguageActivationSettings> _$languages(
    LanguageIntelligenceSettings v,
  ) => v.languages;
  static const Field<
    LanguageIntelligenceSettings,
    Map<String, LanguageActivationSettings>
  >
  _f$languages = Field(
    'languages',
    _$languages,
    opt: true,
    def: const <String, LanguageActivationSettings>{},
  );

  @override
  final MappableFields<LanguageIntelligenceSettings> fields = const {
    #languages: _f$languages,
  };

  static LanguageIntelligenceSettings _instantiate(DecodingData data) {
    return LanguageIntelligenceSettings(languages: data.dec(_f$languages));
  }

  @override
  final Function instantiate = _instantiate;

  static LanguageIntelligenceSettings fromMap(Map<String, dynamic> map) {
    return ensureInitialized().decodeMap<LanguageIntelligenceSettings>(map);
  }

  static LanguageIntelligenceSettings fromJson(String json) {
    return ensureInitialized().decodeJson<LanguageIntelligenceSettings>(json);
  }
}

mixin LanguageIntelligenceSettingsMappable {
  String toJson() {
    return LanguageIntelligenceSettingsMapper.ensureInitialized()
        .encodeJson<LanguageIntelligenceSettings>(
          this as LanguageIntelligenceSettings,
        );
  }

  Map<String, dynamic> toMap() {
    return LanguageIntelligenceSettingsMapper.ensureInitialized()
        .encodeMap<LanguageIntelligenceSettings>(
          this as LanguageIntelligenceSettings,
        );
  }

  LanguageIntelligenceSettingsCopyWith<
    LanguageIntelligenceSettings,
    LanguageIntelligenceSettings,
    LanguageIntelligenceSettings
  >
  get copyWith =>
      _LanguageIntelligenceSettingsCopyWithImpl<
        LanguageIntelligenceSettings,
        LanguageIntelligenceSettings
      >(this as LanguageIntelligenceSettings, $identity, $identity);
  @override
  String toString() {
    return LanguageIntelligenceSettingsMapper.ensureInitialized()
        .stringifyValue(this as LanguageIntelligenceSettings);
  }

  @override
  bool operator ==(Object other) {
    return LanguageIntelligenceSettingsMapper.ensureInitialized().equalsValue(
      this as LanguageIntelligenceSettings,
      other,
    );
  }

  @override
  int get hashCode {
    return LanguageIntelligenceSettingsMapper.ensureInitialized().hashValue(
      this as LanguageIntelligenceSettings,
    );
  }
}

extension LanguageIntelligenceSettingsValueCopy<$R, $Out>
    on ObjectCopyWith<$R, LanguageIntelligenceSettings, $Out> {
  LanguageIntelligenceSettingsCopyWith<$R, LanguageIntelligenceSettings, $Out>
  get $asLanguageIntelligenceSettings => $base.as(
    (v, t, t2) => _LanguageIntelligenceSettingsCopyWithImpl<$R, $Out>(v, t, t2),
  );
}

abstract class LanguageIntelligenceSettingsCopyWith<
  $R,
  $In extends LanguageIntelligenceSettings,
  $Out
>
    implements ClassCopyWith<$R, $In, $Out> {
  MapCopyWith<
    $R,
    String,
    LanguageActivationSettings,
    LanguageActivationSettingsCopyWith<
      $R,
      LanguageActivationSettings,
      LanguageActivationSettings
    >
  >
  get languages;
  $R call({Map<String, LanguageActivationSettings>? languages});
  LanguageIntelligenceSettingsCopyWith<$R2, $In, $Out2> $chain<$R2, $Out2>(
    Then<$Out2, $R2> t,
  );
}

class _LanguageIntelligenceSettingsCopyWithImpl<$R, $Out>
    extends ClassCopyWithBase<$R, LanguageIntelligenceSettings, $Out>
    implements
        LanguageIntelligenceSettingsCopyWith<
          $R,
          LanguageIntelligenceSettings,
          $Out
        > {
  _LanguageIntelligenceSettingsCopyWithImpl(
    super.value,
    super.then,
    super.then2,
  );

  @override
  late final ClassMapperBase<LanguageIntelligenceSettings> $mapper =
      LanguageIntelligenceSettingsMapper.ensureInitialized();
  @override
  MapCopyWith<
    $R,
    String,
    LanguageActivationSettings,
    LanguageActivationSettingsCopyWith<
      $R,
      LanguageActivationSettings,
      LanguageActivationSettings
    >
  >
  get languages => MapCopyWith(
    $value.languages,
    (v, t) => v.copyWith.$chain(t),
    (v) => call(languages: v),
  );
  @override
  $R call({Map<String, LanguageActivationSettings>? languages}) =>
      $apply(FieldCopyWithData({if (languages != null) #languages: languages}));
  @override
  LanguageIntelligenceSettings $make(CopyWithData data) =>
      LanguageIntelligenceSettings(
        languages: data.get(#languages, or: $value.languages),
      );

  @override
  LanguageIntelligenceSettingsCopyWith<$R2, LanguageIntelligenceSettings, $Out2>
  $chain<$R2, $Out2>(Then<$Out2, $R2> t) =>
      _LanguageIntelligenceSettingsCopyWithImpl<$R2, $Out2>($value, $cast, t);
}
