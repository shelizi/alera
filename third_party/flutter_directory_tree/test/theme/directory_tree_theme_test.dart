import 'package:flutter/material.dart';
import 'package:flutter_directory_tree/src/theme/defaults.dart';
import 'package:flutter_directory_tree/src/theme/directory_tree_theme.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('DirectoryTreeTheme', () {
    testWidgets('provides data via inherited widget', (tester) async {
      const themeData = DirectoryTreeThemeData(rowHeight: 32, indent: 12);
      DirectoryTreeThemeData? observed;

      await tester.pumpWidget(
        DirectoryTreeTheme(
          data: themeData,
          child: Builder(
            builder: (context) {
              observed = DirectoryTreeTheme.of(context);
              return const SizedBox();
            },
          ),
        ),
      );

      expect(observed, same(themeData));
      expect(observed!.rowHeight, 32);
      expect(observed!.indent, 12);
    });

    testWidgets('DirectoryTreeDefaults.themed injects brightness aware colors',
        (tester) async {
      DirectoryTreeThemeData? light;
      DirectoryTreeThemeData? dark;

      Future<void> pumpWithBrightness(Brightness brightness) async {
        await tester.pumpWidget(
          Theme(
            data: ThemeData(brightness: brightness),
            child: Builder(
              builder: (context) {
                final themed = DirectoryTreeDefaults.themed(context);
                if (brightness == Brightness.light) {
                  light = themed;
                } else {
                  dark = themed;
                }
                return const SizedBox();
              },
            ),
          ),
        );
      }

      await pumpWithBrightness(Brightness.light);
      await pumpWithBrightness(Brightness.dark);

      expect(light, isNotNull);
      expect(dark, isNotNull);
      expect(light!.selectionColor, isNotNull);
      expect(dark!.selectionColor, isNot(light!.selectionColor));
      expect(light!.hoverColor, isNotNull);
      expect(dark!.hoverColor, isNotNull);
    });
  });
}
