import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/design_system/menus/alera_dropdown_entry.dart';
import 'package:alera/src/design_system/menus/alera_dropdown_submenu_entry.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('submenu entry exposes the expected height and selection match', () {
    const entry = AleraDropdownSubmenuEntry<String, String>(
      primaryValue: 'default',
      label: 'Open',
      childResult: _identity,
      children: <PopupMenuEntry<String>>[
        AleraDropdownEntry<String>(value: 'a', label: 'A'),
      ],
    );
    const disabled = AleraDropdownSubmenuEntry<String, String>(
      primaryValue: 'default',
      label: 'Open',
      enabled: false,
      childResult: _identity,
      children: <PopupMenuEntry<String>>[
        AleraDropdownEntry<String>(value: 'a', label: 'A'),
      ],
    );

    expect(entry.height, 36);
    expect(entry.represents('default'), isTrue);
    expect(disabled.represents('default'), isFalse);
  });

  testWidgets('tapping the label pops the primary value', (tester) async {
    final results = _Results();
    await tester.pumpWidget(
      _SubmenuHarness(
        results: results,
        children: <PopupMenuEntry<String>>[
          const AleraDropdownEntry<String>(value: 'a', label: 'A'),
          const AleraDropdownEntry<String>(value: 'b', label: 'B'),
        ],
      ),
    );
    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Pick Default'));
    await tester.pumpAndSettle();

    expect(results.result, 'default');
    expect(results.childPicked, isNull);
  });

  testWidgets('chevron opens the submenu and child pick maps back', (
    tester,
  ) async {
    final results = _Results();
    await tester.pumpWidget(
      _SubmenuHarness(
        results: results,
        children: <PopupMenuEntry<String>>[
          const AleraDropdownEntry<String>(value: 'a', label: 'A'),
          const AleraDropdownEntry<String>(value: 'b', label: 'B'),
        ],
      ),
    );
    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();

    await tester.tap(_chevron());
    await tester.pumpAndSettle();
    await tester.tap(find.text('B'));
    await tester.pumpAndSettle();

    expect(results.childPicked, 'b');
    expect(results.result, 'b');
  });

  testWidgets('hovering the chevron region opens the submenu', (tester) async {
    await tester.pumpWidget(
      const _SubmenuHarness(
        children: <PopupMenuEntry<String>>[
          AleraDropdownEntry<String>(value: 'a', label: 'A'),
        ],
      ),
    );
    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();

    final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await gesture.addPointer(location: Offset.zero);
    addTearDown(gesture.removePointer);
    await gesture.moveTo(tester.getCenter(_chevron()));
    await tester.pump(const Duration(milliseconds: 400));

    expect(find.text('A'), findsOneWidget);
  });

  testWidgets('renders without the chevron when there are no children', (
    tester,
  ) async {
    final results = _Results();
    await tester.pumpWidget(
      _SubmenuHarness(
        results: results,
        children: const <PopupMenuEntry<String>>[],
      ),
    );
    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();

    expect(find.text('Pick Default'), findsOneWidget);
    expect(_chevron(), findsNothing);

    await tester.tap(find.text('Pick Default'));
    await tester.pumpAndSettle();
    expect(results.result, 'default');
  });

  testWidgets('a disabled entry ignores primary and chevron taps', (
    tester,
  ) async {
    final results = _Results();
    await tester.pumpWidget(
      _SubmenuHarness(
        results: results,
        enabled: false,
        children: <PopupMenuEntry<String>>[
          const AleraDropdownEntry<String>(value: 'a', label: 'A'),
        ],
      ),
    );
    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();

    await tester.tap(find.text('Pick Default'));
    await tester.pumpAndSettle();
    await tester.tap(_chevron());
    await tester.pumpAndSettle();

    expect(find.text('A'), findsNothing);
    expect(results.result, isNull);
    expect(results.childPicked, isNull);
  });
}

String _identity(String value) => value;

Finder _chevron() => find.byWidgetPredicate(
  (widget) => widget is Icon && widget.icon == AleraIcons.chevronRight,
);

class _Results {
  String? result;
  String? childPicked;
}

class _SubmenuHarness extends StatefulWidget {
  const _SubmenuHarness({
    required this.children,
    this.enabled = true,
    this.results,
  });

  final List<PopupMenuEntry<String>> children;
  final bool enabled;
  final _Results? results;

  @override
  State<_SubmenuHarness> createState() => _SubmenuHarnessState();
}

class _SubmenuHarnessState extends State<_SubmenuHarness> {
  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      home: Scaffold(
        body: Center(
          child: Builder(
            builder: (context) => FilledButton(
              onPressed: () {
                showMenu<String>(
                  context: context,
                  position: const RelativeRect.fromLTRB(0, 0, 0, 0),
                  items: <PopupMenuEntry<String>>[
                    AleraDropdownSubmenuEntry<String, String>(
                      primaryValue: 'default',
                      label: 'Pick Default',
                      enabled: widget.enabled,
                      childResult: _identity,
                      onChildResult: (value) =>
                          widget.results?.childPicked = value,
                      children: widget.children,
                    ),
                  ],
                ).then((value) => widget.results?.result = value);
              },
              child: const Text('Open'),
            ),
          ),
        ),
      ),
    );
  }
}
