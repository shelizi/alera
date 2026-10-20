#include "flutter_window.h"

#include <optional>

#include <flutter/standard_method_codec.h>

#include "flutter/generated_plugin_registrant.h"
#include "win32_app_menu.h"
#include "win32_dark_mode.h"
#include "win32_file_drag.h"
#include "win32_rich_clipboard.h"

FlutterWindow::FlutterWindow(const flutter::DartProject& project)
    : project_(project) {}

FlutterWindow::~FlutterWindow() {}

bool FlutterWindow::OnCreate() {
  if (!Win32Window::OnCreate()) {
    return false;
  }

  RECT frame = GetClientArea();

  // The size here must match the window dimensions to avoid unnecessary surface
  // creation / destruction in the startup path.
  flutter_controller_ = std::make_unique<flutter::FlutterViewController>(
      frame.right - frame.left, frame.bottom - frame.top, project_);
  // Ensure that basic setup of the controller was successful.
  if (!flutter_controller_->engine() || !flutter_controller_->view()) {
    return false;
  }
  RegisterPlugins(flutter_controller_->engine());
  SetChildContent(flutter_controller_->view()->GetNativeWindow());

  app_menu_channel_ =
      std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
          flutter_controller_->engine()->messenger(),
          "dev.leynier.alera/app_menu",
          &flutter::StandardMethodCodec::GetInstance());

  desktop_presence_channel_ =
      std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
          flutter_controller_->engine()->messenger(),
          "dev.leynier.alera/desktop_presence",
          &flutter::StandardMethodCodec::GetInstance());
  desktop_presence_.Attach(GetHandle(), desktop_presence_channel_.get());
  desktop_presence_channel_->SetMethodCallHandler(
      [this](const flutter::MethodCall<flutter::EncodableValue>& call,
             std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>>
                 result) {
        desktop_presence_.HandleMethodCall(call, std::move(result));
      });

  file_drag_channel_ =
      std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
          flutter_controller_->engine()->messenger(),
          "dev.leynier.alera/native_file_drag",
          &flutter::StandardMethodCodec::GetInstance());
  file_drag_channel_->SetMethodCallHandler(
      [this](const flutter::MethodCall<flutter::EncodableValue>& call,
             std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>>
                 result) {
        HandleAleraFileDragMethodCall(GetHandle(), call, std::move(result));
      });

  rich_clipboard_channel_ =
      std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
          flutter_controller_->engine()->messenger(),
          "dev.leynier.alera/rich_clipboard",
          &flutter::StandardMethodCodec::GetInstance());
  rich_clipboard_channel_->SetMethodCallHandler(
      [this](const flutter::MethodCall<flutter::EncodableValue>& call,
             std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>>
                 result) {
        HandleAleraRichClipboardMethodCall(GetHandle(), call, std::move(result));
      });

  // A child Flutter window can ask the primary process to verify an actual
  // native drag drop over this HWND. WindowFromPoint ignores Flutter's local
  // overlay coordinates and prevents docking when the pointer is still over
  // the detached window or an unrelated application.
  detached_tab_channel_ =
      std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
          flutter_controller_->engine()->messenger(),
          "dev.leynier.alera/detached_tabs",
          &flutter::StandardMethodCodec::GetInstance());
  detached_tab_channel_->SetMethodCallHandler(
      [this](const flutter::MethodCall<flutter::EncodableValue>& call,
             std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>>
                 result) {
        if (call.method_name() != "isCursorOverWindow") {
          result->NotImplemented();
          return;
        }
        POINT point{};
        const HWND hit = ::GetCursorPos(&point) ? ::WindowFromPoint(point)
                                               : nullptr;
        const bool on_this_window =
            hit != nullptr && ::GetAncestor(hit, GA_ROOT) == GetHandle();
        result->Success(flutter::EncodableValue(on_this_window));
      });

  flutter_controller_->engine()->SetNextFrameCallback([&]() {
    this->Show();
  });

  // Flutter can complete the first frame before the "show window" callback is
  // registered. The following call ensures a frame is pending to ensure the
  // window is shown. It is a no-op if the first frame hasn't completed yet.
  flutter_controller_->ForceRedraw();

  return true;
}

void FlutterWindow::OnDestroy() {
  detached_tab_channel_.reset();
  rich_clipboard_channel_.reset();
  file_drag_channel_.reset();
  desktop_presence_.Destroy();
  if (flutter_controller_) {
    flutter_controller_ = nullptr;
  }

  Win32Window::OnDestroy();
}

LRESULT
FlutterWindow::MessageHandler(HWND hwnd, UINT const message,
                              WPARAM const wparam,
                              LPARAM const lparam) noexcept {
  // Give Flutter, including plugins, an opportunity to handle window messages.
  if (flutter_controller_) {
    std::optional<LRESULT> result =
        flutter_controller_->HandleTopLevelWindowProc(hwnd, message, wparam,
                                                      lparam);
    if (result) {
      return *result;
    }
  }

  switch (message) {
    case WM_COMMAND:
      if (const char* method =
              Win32AppMenuMethodForCommand(LOWORD(wparam))) {
        if (app_menu_channel_) {
          app_menu_channel_->InvokeMethod(method, nullptr);
        }
        return 0;
      }
      break;
    case WM_MEASUREITEM:
      if (HandleAleraMenuBarMeasureItem(hwnd, lparam)) {
        return TRUE;
      }
      break;
    case WM_DRAWITEM:
      if (HandleAleraMenuBarDrawItem(hwnd, wparam, lparam)) {
        return TRUE;
      }
      break;
    case WM_DPICHANGED:
      RefreshAleraMenuBarForDpi(hwnd);
      break;
    case WM_FONTCHANGE:
      flutter_controller_->engine()->ReloadSystemFonts();
      break;
  }

  if (desktop_presence_.HandleMessage(hwnd, message, wparam, lparam)) {
    return 0;
  }

  return Win32Window::MessageHandler(hwnd, message, wparam, lparam);
}
