#pragma once

#include <string>
#include <vector>

// Keep in sync with shouldUseSharedEngineTabsInvocation in lib/main.dart.
// Normal Windows main uses one Flutter Engine for all supported Editor views.
// The independent legacy child process and explicit opt-out must keep their
// original engine/window lifecycle. Unrecognized command lines fail closed.
inline bool ShouldUseSharedEngineTabs(const std::vector<std::string>& args,
                                      bool detached_child) {
  if (detached_child) return false;
  return args.empty() ||
         (args.size() == 1 && args.front() == "--alera-shared-engine-tabs");
}
