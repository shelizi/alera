import 'package:alera/src/features/workbench/application/workbench_view_pref_filters.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('project filter mutations update only selected project ids', () {
    final prefs = WorkbenchViewPrefs.defaults.copyWith(
      selectedProjectIds: const <String>{'project-a'},
      selectedTagIds: const <String>{'tag-a'},
    );

    final added = nextWorkbenchProjectFilterPrefs(
      prefs: prefs,
      id: 'project-b',
      mutation: WorkbenchFilterMutation.add,
    );
    expect(added?.selectedProjectIds, <String>{'project-a', 'project-b'});
    expect(added?.selectedTagIds, same(prefs.selectedTagIds));

    final toggled = nextWorkbenchProjectFilterPrefs(
      prefs: prefs,
      id: 'project-a',
      mutation: WorkbenchFilterMutation.toggle,
    );
    expect(toggled?.selectedProjectIds, isEmpty);
    expect(toggled?.selectedTagIds, same(prefs.selectedTagIds));

    final cleared = nextWorkbenchProjectFilterPrefs(
      prefs: prefs,
      mutation: WorkbenchFilterMutation.clear,
    );
    expect(cleared?.selectedProjectIds, isEmpty);
  });

  test('project filter mutations return null when state would not change', () {
    final prefs = WorkbenchViewPrefs.defaults.copyWith(
      selectedProjectIds: const <String>{'project-a'},
    );

    expect(
      nextWorkbenchProjectFilterPrefs(
        prefs: prefs,
        id: 'project-a',
        mutation: WorkbenchFilterMutation.add,
      ),
      isNull,
    );
    expect(
      nextWorkbenchProjectFilterPrefs(
        prefs: prefs,
        id: 'missing',
        mutation: WorkbenchFilterMutation.remove,
      ),
      isNull,
    );
    expect(
      nextWorkbenchProjectFilterPrefs(
        prefs: WorkbenchViewPrefs.defaults,
        mutation: WorkbenchFilterMutation.clear,
      ),
      isNull,
    );
  });

  test('tag filter mutations update only selected tag ids', () {
    final prefs = WorkbenchViewPrefs.defaults.copyWith(
      selectedProjectIds: const <String>{'project-a'},
      selectedTagIds: const <String>{'tag-a'},
    );

    final added = nextWorkbenchTagFilterPrefs(
      prefs: prefs,
      id: 'tag-b',
      mutation: WorkbenchFilterMutation.add,
    );
    expect(added?.selectedTagIds, <String>{'tag-a', 'tag-b'});
    expect(added?.selectedProjectIds, same(prefs.selectedProjectIds));

    final toggled = nextWorkbenchTagFilterPrefs(
      prefs: prefs,
      id: 'tag-a',
      mutation: WorkbenchFilterMutation.toggle,
    );
    expect(toggled?.selectedTagIds, isEmpty);
    expect(toggled?.selectedProjectIds, same(prefs.selectedProjectIds));

    final cleared = nextWorkbenchTagFilterPrefs(
      prefs: prefs,
      mutation: WorkbenchFilterMutation.clear,
    );
    expect(cleared?.selectedTagIds, isEmpty);
  });

  test('tag filter mutations return null when state would not change', () {
    final prefs = WorkbenchViewPrefs.defaults.copyWith(
      selectedTagIds: const <String>{'tag-a'},
    );

    expect(
      nextWorkbenchTagFilterPrefs(
        prefs: prefs,
        id: 'tag-a',
        mutation: WorkbenchFilterMutation.add,
      ),
      isNull,
    );
    expect(
      nextWorkbenchTagFilterPrefs(
        prefs: prefs,
        id: 'missing',
        mutation: WorkbenchFilterMutation.remove,
      ),
      isNull,
    );
    expect(
      nextWorkbenchTagFilterPrefs(
        prefs: WorkbenchViewPrefs.defaults,
        mutation: WorkbenchFilterMutation.clear,
      ),
      isNull,
    );
  });
}
