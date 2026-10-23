#include "../shared_engine_startup_mode.h"

#include <string>
#include <vector>

int main() {
  using Args = std::vector<std::string>;

  if (!ShouldUseSharedEngineTabs(Args{}, false)) return 1;
  if (!ShouldUseSharedEngineTabs(Args{"--alera-shared-engine-tabs"}, false)) {
    return 2;  // Backward compatible explicit enable switch.
  }
  if (ShouldUseSharedEngineTabs(Args{"--alera-legacy-detached-tabs"}, false)) {
    return 3;  // Explicit user recovery option always wins.
  }
  if (ShouldUseSharedEngineTabs(Args{"--alera-detached-tab", "5123", "token"}, true)) {
    return 4;  // Child already owns an independent Engine.
  }
  if (ShouldUseSharedEngineTabs(Args{}, true)) return 5;
  if (ShouldUseSharedEngineTabs(Args{"--alera-shared-engine-tabs=1"}, false)) {
    return 6;
  }
  if (ShouldUseSharedEngineTabs(Args{"--alera-shared-engine-tabs", "--extra"}, false)) {
    return 7;
  }
  if (ShouldUseSharedEngineTabs(Args{"--unknown-flag"}, false)) return 8;
  return 0;
}
