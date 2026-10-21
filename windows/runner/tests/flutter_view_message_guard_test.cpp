#include "flutter_view_message_guard.h"

#include <iostream>

namespace {
bool Expect(bool condition, const char* label) {
  std::cout << (condition ? "PASS: " : "FAIL: ") << label << '\n';
  return condition;
}
}  // namespace

int main() {
  bool ok = true;
  ok = Expect(!ShouldForwardToFlutterView(false, WM_SIZE, 0),
              "do not dispatch before Flutter HWND is attached") && ok;
  ok = Expect(!ShouldForwardToFlutterView(false, WM_FONTCHANGE, 0),
              "do not reload fonts before Flutter engine is ready") && ok;
  ok = Expect(ShouldForwardToFlutterView(true, WM_SIZE, 0),
              "forward ordinary live-view window messages") && ok;
  ok = Expect(ShouldForwardToFlutterView(
                  true, WM_PARENTNOTIFY, MAKEWPARAM(WM_CREATE, 0)),
              "preserve child-create parent notification") && ok;
  ok = Expect(!ShouldForwardToFlutterView(
                  true, WM_PARENTNOTIFY, MAKEWPARAM(WM_DESTROY, 0)),
              "never dispatch child-destroy into Flutter's removed view") && ok;
  ok = Expect(!ShouldForwardToFlutterView(true, WM_DESTROY, 0),
              "do not re-enter Flutter on top-level teardown") && ok;
  ok = Expect(!ShouldForwardToFlutterView(true, WM_NCDESTROY, 0),
              "do not re-enter Flutter after HWND destruction") && ok;
  ok = Expect(ShouldForwardToFlutterView(true, WM_FONTCHANGE, 0),
              "allow font reload while native view is alive") && ok;
  return ok ? 0 : 1;
}
