import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/settings/presentation/panes/application_pane.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

const Locale _en = Locale('en');
const Locale _zhTw = Locale('zh', 'TW');
const Locale _zhCn = Locale('zh', 'CN');
const Locale _jaJp = Locale('ja', 'JP');

Locale _system(Locale? locale) =>
    resolveAleraLocale(AppLanguage.system, locale);

void main() {
  group('Alera locale resolution', () {
    test('explicit languages map to their own locale', () {
      expect(resolveAleraLocale(AppLanguage.english, _zhCn), _en);
      expect(resolveAleraLocale(AppLanguage.traditionalChinese, _en), _zhTw);
      expect(resolveAleraLocale(AppLanguage.simplifiedChinese, _en), _zhCn);
      expect(resolveAleraLocale(AppLanguage.japanese, _en), _jaJp);
    });

    test('system Chinese resolves by script before region', () {
      expect(
        _system(
          const Locale.fromSubtags(languageCode: 'zh', scriptCode: 'Hans'),
        ),
        _zhCn,
      );
      expect(
        _system(
          const Locale.fromSubtags(languageCode: 'zh', scriptCode: 'Hant'),
        ),
        _zhTw,
      );
      // Script wins even when the region points the other way.
      expect(
        _system(
          const Locale.fromSubtags(
            languageCode: 'zh',
            scriptCode: 'Hans',
            countryCode: 'TW',
          ),
        ),
        _zhCn,
      );
      expect(
        _system(
          const Locale.fromSubtags(
            languageCode: 'zh',
            scriptCode: 'Hant',
            countryCode: 'CN',
          ),
        ),
        _zhTw,
      );
    });

    test('system Chinese resolves by region when no script is present', () {
      expect(_system(const Locale('zh', 'CN')), _zhCn);
      expect(_system(const Locale('zh', 'SG')), _zhCn);
      expect(_system(const Locale('zh', 'TW')), _zhTw);
      expect(_system(const Locale('zh', 'HK')), _zhTw);
      expect(_system(const Locale('zh', 'MO')), _zhTw);
    });

    test('bare Chinese keeps the Traditional Chinese default', () {
      expect(_system(const Locale('zh')), _zhTw);
      expect(_system(const Locale('zh', 'XX')), _zhTw);
    });

    test('system Japanese resolves to ja-JP', () {
      expect(_system(const Locale('ja')), _jaJp);
      expect(_system(const Locale('ja', 'JP')), _jaJp);
      expect(_system(const Locale('JA', 'JP')), _jaJp);
    });

    test('unsupported and missing system locales fall back to English', () {
      expect(_system(null), _en);
      expect(_system(const Locale('ko', 'KR')), _en);
      expect(_system(const Locale('de')), _en);
      expect(_system(const Locale('en', 'GB')), _en);
    });

    test('supported locales cover every shipped language', () {
      expect(supportedAleraLocales, <Locale>[_en, _zhTw, _zhCn, _jaJp]);
    });

    test('the delegate supports en, zh and ja', () {
      const delegate = AleraLocalizationsDelegate();
      expect(delegate.isSupported(_en), isTrue);
      expect(delegate.isSupported(_zhTw), isTrue);
      expect(delegate.isSupported(_zhCn), isTrue);
      expect(delegate.isSupported(_jaJp), isTrue);
      expect(delegate.isSupported(const Locale('ko')), isFalse);
    });
  });

  group('Table selection', () {
    test('locale picks the matching table', () {
      expect(const AleraLocalizations(_zhTw).isTraditionalChinese, isTrue);
      expect(const AleraLocalizations(_zhCn).isSimplifiedChinese, isTrue);
      expect(const AleraLocalizations(_jaJp).isJapanese, isTrue);
      expect(
        const AleraLocalizations(
          Locale.fromSubtags(languageCode: 'zh', scriptCode: 'Hans'),
        ).isSimplifiedChinese,
        isTrue,
      );
      expect(
        const AleraLocalizations(
          Locale.fromSubtags(languageCode: 'zh', scriptCode: 'Hant'),
        ).isTraditionalChinese,
        isTrue,
      );
      // A bare `zh` renders Traditional Chinese, matching the resolver.
      expect(
        const AleraLocalizations(Locale('zh')).isTraditionalChinese,
        isTrue,
      );
    });
  });

  group('Simplified Chinese', () {
    const l10n = AleraLocalizations(_zhCn);

    test('translates common UI strings', () {
      expect(l10n.translate('Settings'), '设置');
      expect(l10n.translate('Save'), '保存');
      expect(l10n.translate('Search'), '搜索');
      expect(l10n.translate('Add Project'), '添加项目');
      expect(l10n.translate('New Workspace'), '新建工作区');
      expect(l10n.translate('Explorer'), '资源管理器');
      expect(l10n.translate('Command Palette'), '命令面板');
      expect(l10n.translate('Key'), '密钥');
      expect(l10n.translate('Password'), '密码');
      expect(l10n.translate('Storage'), '存储空间');
      expect(l10n.translate('Credentials'), '凭据');
    });

    test('keeps product, provider and model names intact', () {
      expect(l10n.translate('Pull Request'), 'Pull Request');
      expect(l10n.translate('Commit'), 'Commit');
      expect(l10n.translate('Fetch'), 'Fetch');
      expect(l10n.translate('Stash'), 'Stash');
      expect(l10n.translate('Cherry Pick'), 'Cherry Pick');
      expect(l10n.translate('Open in Zed'), '在 Zed 中打开');
      expect(l10n.translate('Local Whisper'), '本地 Whisper');
      expect(l10n.translate('OpenAI-Compatible API'), 'OpenAI 兼容 API');
      expect(l10n.translate('Claude CCS Profiles'), 'Claude CCS Profiles');
    });

    test('keeps language labels in their own script', () {
      expect(l10n.translate('English'), 'English');
      expect(l10n.translate('繁體中文'), '繁體中文');
    });

    test('translates dynamic patterns and passes identifiers through', () {
      expect(
        l10n.translate('Rebase Current Branch onto feature/demo'),
        '将当前分支 Rebase 到 feature/demo',
      );
      expect(
        l10n.translate('The generated branch "wip/alera" already exists.'),
        '生成的 Branch“wip/alera”已存在。',
      );
      expect(
        l10n.translate('README.md has unsaved changes.'),
        '“README.md”有尚未保存的变更。',
      );
      expect(
        l10n.translate('3 editor tabs have unsaved changes.'),
        '有 3 个编辑器标签页包含尚未保存的变更。',
      );
      expect(l10n.translate('Chunk 1 of 2'), '区块 1/2');
      expect(l10n.translate('Checks (3)'), '检查（3）');
      expect(l10n.translate('2 failing Checks'), '2 个失败检查');
      expect(l10n.translate('Selected: Alera Dark'), '已选择：Alera Dark');
      expect(l10n.translate('Showing 4 of 38'), '显示 4 / 38');
      expect(
        l10n.translate('Current version 1.2.3 (build 45)'),
        '当前版本 1.2.3（Build 45）',
      );
      expect(l10n.translate('Global (Claude)'), '全局（Claude）');
      expect(l10n.translate('Zed is available: 0.210.0'), 'Zed 可使用：0.210.0');
      expect(
        l10n.translate('Install `gh` and ensure it is on your PATH.'),
        '请安装 `gh`，并确认它位于 PATH 中。',
      );
      expect(l10n.translate('Expires in 2m 5s'), '2 分 5 秒后到期');
      expect(l10n.translate('12.5 MiB of 42.0 MiB'), '12.5 MiB / 42.0 MiB');
      expect(
        l10n.translate('Unknown prompt variable: {{workspace.foo}}'),
        '未知的提示词变量：{{workspace.foo}}',
      );
    });

    test('resolves the embedded label of a composed string', () {
      expect(l10n.translate('Branch Name is required'), '分支名称 为必填。');
      expect(l10n.translate('All Project'), '所有项目');
      expect(l10n.translate('Resize Projects List'), '调整项目列表大小');
      expect(
        l10n.translate(
          'Ctrl+W is assigned to "Close Tab". Reassign it to "New Terminal Tab"?',
        ),
        'Ctrl+W 已分配给“关闭标签页”。要重新分配给“新建终端标签页”吗？',
      );
    });

    test('translates runtime busy summaries', () {
      expect(
        l10n.translate(
          'The runtime has 2 open agent(s) and 1 active terminal session(s). Force stop terminates them.',
        ),
        '运行时当前有 2 个打开中的代理、1 个活动的终端会话。强制停止会终止这些工作。',
      );
    });
  });

  group('Japanese', () {
    const l10n = AleraLocalizations(_jaJp);

    test('translates common UI strings', () {
      expect(l10n.translate('Settings'), '設定');
      expect(l10n.translate('Cancel'), 'キャンセル');
      expect(l10n.translate('Search'), '検索');
      expect(l10n.translate('Add Project'), 'プロジェクトを追加');
      expect(l10n.translate('New Workspace'), '新規ワークスペース');
      expect(l10n.translate('Explorer'), 'エクスプローラー');
      expect(l10n.translate('Command Palette'), 'コマンドパレット');
      expect(l10n.translate('Key'), 'キー');
      expect(l10n.translate('Password'), 'パスワード');
      expect(l10n.translate('Storage'), 'ストレージ');
      expect(l10n.translate('Credentials'), '認証情報');
    });

    test('keeps product, provider and model names intact', () {
      expect(l10n.translate('Pull Request'), 'Pull Request');
      expect(l10n.translate('Commit'), 'Commit');
      expect(l10n.translate('Fetch'), 'Fetch');
      expect(l10n.translate('Stash'), 'Stash');
      expect(l10n.translate('Cherry Pick'), 'Cherry Pick');
      expect(l10n.translate('Open in Zed'), 'Zed で開く');
      expect(l10n.translate('Local Whisper'), 'ローカル Whisper');
      expect(l10n.translate('OpenAI-Compatible API'), 'OpenAI 互換 API');
      expect(l10n.translate('Claude CCS Profiles'), 'Claude CCS Profiles');
    });

    test('keeps language labels in their own script', () {
      expect(l10n.translate('English'), 'English');
      expect(l10n.translate('繁體中文'), '繁體中文');
    });

    test('translates dynamic patterns and passes identifiers through', () {
      expect(
        l10n.translate('Rebase Current Branch onto feature/demo'),
        '現在のブランチを feature/demo に Rebase',
      );
      expect(
        l10n.translate('The generated branch "wip/alera" already exists.'),
        '生成された Branch「wip/alera」はすでに存在します。',
      );
      expect(
        l10n.translate('README.md has unsaved changes.'),
        '「README.md」に保存していない変更があります。',
      );
      expect(
        l10n.translate('3 editor tabs have unsaved changes.'),
        '3 個のエディタータブに保存していない変更があります。',
      );
      expect(l10n.translate('Chunk 1 of 2'), 'チャンク 1/2');
      expect(l10n.translate('Checks (3)'), 'チェック（3）');
      expect(l10n.translate('2 failing Checks'), '失敗 のチェック 2 件');
      expect(l10n.translate('Selected: Alera Dark'), '選択中: Alera Dark');
      expect(l10n.translate('Showing 4 of 38'), '38 件中 4 件を表示');
      expect(
        l10n.translate('Current version 1.2.3 (build 45)'),
        '現在のバージョン 1.2.3（Build 45）',
      );
      expect(l10n.translate('Global (Claude)'), 'グローバル（Claude）');
      expect(
        l10n.translate('Zed is available: 0.210.0'),
        'Zed は利用できます: 0.210.0',
      );
      expect(
        l10n.translate('Install `gh` and ensure it is on your PATH.'),
        '`gh` をインストールし、PATH に含まれていることを確認してください。',
      );
      expect(l10n.translate('Expires in 2m 5s'), '2 分 5 秒後に期限切れ');
      expect(l10n.translate('12.5 MiB of 42.0 MiB'), '12.5 MiB / 42.0 MiB');
      expect(
        l10n.translate('Unknown prompt variable: {{workspace.foo}}'),
        '不明なプロンプト変数: {{workspace.foo}}',
      );
    });

    test('resolves the embedded label of a composed string', () {
      expect(l10n.translate('Branch Name is required'), 'ブランチ名 は必須です。');
      expect(l10n.translate('All Project'), 'すべてのプロジェクト');
      expect(l10n.translate('Resize Projects List'), 'プロジェクト リストのサイズを変更');
      expect(
        l10n.translate(
          'Ctrl+W is assigned to "Close Tab". Reassign it to "New Terminal Tab"?',
        ),
        'Ctrl+W は「タブを閉じる」に割り当てられています。'
        '「新規ターミナルタブ」に割り当て直しますか？',
      );
    });

    test('translates runtime busy summaries', () {
      expect(
        l10n.translate(
          'The runtime has 2 open agent(s) and 1 active terminal session(s). Force stop terminates them.',
        ),
        'Runtime には現在 2 件の実行中の Agent、1 件のアクティブな'
        'ターミナルセッションがあります。強制停止するとそれらは終了します。',
      );
    });
  });

  group('Fallbacks', () {
    test('unknown strings fall back to the English source', () {
      const sources = <String>[
        'Unmapped String',
        'A sentence that no table covers at all.',
        '',
      ];
      for (final locale in <Locale>[_en, _zhTw, _zhCn, _jaJp]) {
        final l10n = AleraLocalizations(locale);
        for (final source in sources) {
          expect(
            l10n.translate(source),
            source,
            reason: 'Expected $locale to pass "$source" through untouched',
          );
        }
      }
    });

    test('English never translates a known key', () {
      const l10n = AleraLocalizations(_en);
      expect(l10n.translate('Settings'), 'Settings');
      expect(l10n.translate('Chunk 1 of 2'), 'Chunk 1 of 2');
    });
  });

  group('Static table parity', () {
    final traditional = AleraLocalizations.staticSourceKeysFor(
      AppLanguage.traditionalChinese,
    );
    final simplified = AleraLocalizations.staticSourceKeysFor(
      AppLanguage.simplifiedChinese,
    );
    final japanese = AleraLocalizations.staticSourceKeysFor(
      AppLanguage.japanese,
    );

    test('zh-TW is not empty', () {
      expect(traditional, isNotEmpty);
    });

    test('zh-CN covers exactly the zh-TW source keys', () {
      expect(simplified.difference(traditional), isEmpty);
      expect(traditional.difference(simplified), isEmpty);
    });

    test('ja-JP covers exactly the zh-TW source keys', () {
      expect(japanese.difference(traditional), isEmpty);
      expect(traditional.difference(japanese), isEmpty);
    });

    test('every source key resolves in every language', () {
      const zhTw = AleraLocalizations(_zhTw);
      const zhCn = AleraLocalizations(_zhCn);
      const ja = AleraLocalizations(_jaJp);
      for (final key in traditional) {
        for (final entry in <MapEntry<String, AleraLocalizations>>[
          const MapEntry('zh-TW', zhTw),
          const MapEntry('zh-CN', zhCn),
          const MapEntry('ja-JP', ja),
        ]) {
          expect(
            entry.value.translate(key),
            isNotEmpty,
            reason: 'Empty ${entry.key} translation for "$key"',
          );
        }
      }
    });
  });

  group('Language settings', () {
    test('the dropdown exposes every AppLanguage exactly once', () {
      expect(
        appLanguageEntries.map((entry) => entry.value).toList(),
        AppLanguage.values,
      );
    });

    test('language labels are written in their own language', () {
      String labelFor(AppLanguage language) => appLanguageEntries
          .firstWhere((entry) => entry.value == language)
          .label;
      expect(labelFor(AppLanguage.system), 'Follow System');
      expect(labelFor(AppLanguage.english), 'English');
      expect(labelFor(AppLanguage.traditionalChinese), '繁體中文');
      expect(labelFor(AppLanguage.simplifiedChinese), '简体中文');
      expect(labelFor(AppLanguage.japanese), '日本語');
    });

    test('every AppLanguage round-trips through the mapper', () {
      for (final language in AppLanguage.values) {
        expect(AppLanguageMapper.fromValue(language.toValue()), language);
      }
      expect(AppLanguage.simplifiedChinese.toValue(), 'simplifiedChinese');
      expect(AppLanguage.japanese.toValue(), 'japanese');
    });
  });
}
