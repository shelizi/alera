#include <flutter/dart_project.h>
#include <flutter/flutter_view_controller.h>
#include <windows.h>

#include <algorithm>
#include <string>
#include <vector>
#include "flutter_window.h"
#include "utils.h"
#include "win32_dark_mode.h"
#include "win32_single_instance.h"

// A malformed or merely substring-matching flag must never bypass the
// primary Alera single-instance gate.
static bool IsDetachedChildInvocation(const std::vector<std::string>& args) {
  if (args.size() != 3 || args[0] != "--alera-detached-tab") return false;
  const auto& port = args[1];
  if (port.empty() || port.size() > 5 ||
      !std::all_of(port.begin(), port.end(),
                   [](char c) { return c >= '0' && c <= '9'; })) {
    return false;
  }
  const int port_number = std::stoi(port);
  if (port_number < 1 || port_number > 65535) return false;
  const auto& capability = args[2];
  return capability.size() == 40 &&
         std::all_of(capability.begin(), capability.end(), [](char c) {
           return (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f');
         });
}

int APIENTRY wWinMain(_In_ HINSTANCE instance, _In_opt_ HINSTANCE prev,
                      _In_ wchar_t *command_line, _In_ int show_command) {
  // Detached tab children have a one-time loopback capability and must not
  // activate (or replace) the primary Alera window.
  std::vector<std::string> command_line_arguments = GetCommandLineArguments();
  const bool detached_tab = IsDetachedChildInvocation(command_line_arguments);
  // An exact, explicit argument only. Still participates in the normal
  // primary instance gate and never overrides a detached child invocation.
  const bool experimental_shared_engine =
      !detached_tab && command_line_arguments.size() == 1 &&
      command_line_arguments[0] == "--alera-shared-engine-tabs";
  Win32SingleInstance single_instance(
      detached_tab ? std::wstring(ALERA_APP_ID) + L".detached." +
                         std::to_wstring(GetCurrentProcessId())
                   : std::wstring(ALERA_APP_ID));
  const auto start = single_instance.Start();
  if (start == Win32SingleInstance::StartResult::forwarded) {
    return EXIT_SUCCESS;
  }
  if (start == Win32SingleInstance::StartResult::failed) {
    ::MessageBoxW(nullptr, L"Unable to activate the Alera application instance.",
                  ALERA_APP_NAME, MB_OK | MB_ICONERROR);
    return EXIT_FAILURE;
  }

  // Alera is dark-only; must run before any menu or window is created so the
  // OS renders dark popup menus for the whole process.
  EnableAleraDarkMode();

  // Attach to console when present (e.g., 'flutter run') or create a
  // new console when running with a debugger.
  if (!::AttachConsole(ATTACH_PARENT_PROCESS) && ::IsDebuggerPresent()) {
    CreateAndAttachConsole();
  }

  // Initialize COM, so that it is available for use in the library and/or
  // plugins.
  ::CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);

  flutter::DartProject project(L"data");

  project.set_dart_entrypoint_arguments(std::move(command_line_arguments));

  FlutterWindow window(project, experimental_shared_engine);
  Win32Window::Point origin(10, 10);
  Win32Window::Size size(1280, 720);
  if (detached_tab) {
    // A torn-off tab opens near its drop point instead of hiding the primary
    // editor under another 1280px window anchored to (10, 10). Clamp to the
    // nearest monitor's usable region, including negative monitor origins.
    POINT pointer{};
    if (::GetCursorPos(&pointer)) {
      const HMONITOR monitor =
          ::MonitorFromPoint(pointer, MONITOR_DEFAULTTONEAREST);
      MONITORINFO info{sizeof(MONITORINFO)};
      if (::GetMonitorInfoW(monitor, &info)) {
        const int left = static_cast<int>(info.rcWork.left);
        const int top = static_cast<int>(info.rcWork.top);
        const int right = static_cast<int>(info.rcWork.right);
        const int bottom = static_cast<int>(info.rcWork.bottom);
        const int width = std::min(1024, right - left);
        const int height = std::min(720, bottom - top);
        if (width > 0 && height > 0) {
          size = Win32Window::Size(width, height);
          origin = Win32Window::Point(
              std::clamp(static_cast<int>(pointer.x) - 120, left,
                         std::max(left, right - width)),
              std::clamp(static_cast<int>(pointer.y) - 40, top,
                         std::max(top, bottom - height)));
        }
      }
    }
  }
  if (!window.Create(ALERA_APP_NAME, origin, size)) {
    return EXIT_FAILURE;
  }
  window.SetQuitOnClose(true);

  const int exit_code = single_instance.RunMessageLoop(window.GetHandle());

  ::CoUninitialize();
  return exit_code;
}
