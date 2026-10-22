#include "flutter_window.h"
#include <flutter_windows.h>
#include <multiview_desktop/multi_view_desktop_plugin.h>
#include "flutter/generated_plugin_registrant.h"

// Standalone proof-of-concept only. This is NOT Alera's production runner.
FlutterWindow::FlutterWindow(const flutter::DartProject& project)
    : project_(project) {}
FlutterWindow::~FlutterWindow() {}

bool FlutterWindow::OnCreate() {
  if (!Win32Window::OnCreate()) return false;
  const RECT frame = GetClientArea();
  MultiViewDesktopPrepareEngine(project_, GetHandle());
  MultiViewDesktopCreateMainView(GetHandle(), frame.right - frame.left,
                                  frame.bottom - frame.top, RegisterPlugins);
  const HWND flutter_hwnd =
      MultiViewDesktopGetFlutterHwnd(MultiViewDesktopGetMainViewId());
  if (!flutter_hwnd || !::IsWindow(flutter_hwnd)) return false;
  SetChildContent(flutter_hwnd);
  return true;
}

void FlutterWindow::OnDestroy() { Win32Window::OnDestroy(); }

LRESULT FlutterWindow::MessageHandler(HWND hwnd, UINT const message,
                                      WPARAM const wparam,
                                      LPARAM const lparam) noexcept {
  if (message == WM_FONTCHANGE) {
    FlutterDesktopEngineReloadSystemFonts(MultiViewDesktopGetEngineRef());
  }
  LRESULT result = 0;
  if (MultiViewDesktopHandleWindowProc(hwnd, message, wparam, lparam, &result))
    return result;
  return Win32Window::MessageHandler(hwnd, message, wparam, lparam);
}
