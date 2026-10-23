#include "flutter_window.h"

#include <optional>

#include <flutter/plugin_registrar.h>
#include <flutter/plugin_registrar_windows.h>
#include <flutter/plugin_registry.h>
#include <flutter/standard_method_codec.h>
#include <multi_view_desktop/multi_view_desktop.h>

#include "flutter/generated_plugin_registrant.h"
#include "flutter_view_message_guard.h"
#include "win32_app_menu.h"
#include "win32_dark_mode.h"
#include "win32_file_drag.h"
#include "win32_rich_clipboard.h"

namespace {
// The shared engine is owned by the multiview plugin. Obtain a stable
// BinaryMessenger from a registrar scoped to that engine, rather than
// creating a second FlutterViewController or retaining a temporary wrapper.
flutter::BinaryMessenger* g_shared_native_messenger = nullptr;

void RegisterSharedEnginePlugins(flutter::PluginRegistry* registry) {
  RegisterPlugins(registry);
  auto registrar_ref =
      registry->GetRegistrarForPlugin("AleraRunnerNativeChannels");
  if (!registrar_ref) return;
  auto registrar = flutter::PluginRegistrarManager::GetInstance()
                       ->GetRegistrar<flutter::PluginRegistrarWindows>(
                           registrar_ref);
  g_shared_native_messenger = registrar->messenger();
}

std::wstring PreviewTitleFromUtf8(const std::string& utf8) {
  if (utf8.empty()) return L"Tab";
  const int size = ::MultiByteToWideChar(
      CP_UTF8, MB_ERR_INVALID_CHARS, utf8.data(),
      static_cast<int>(utf8.size()), nullptr, 0);
  if (size <= 0) return L"Tab";
  std::wstring title(size, L'\0');
  ::MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, utf8.data(),
                        static_cast<int>(utf8.size()), title.data(), size);
  return title;
}
}  // namespace

FlutterWindow::FlutterWindow(const flutter::DartProject& project,
                             bool shared_engine)
    : project_(project),
      shared_engine_(shared_engine) {}

FlutterWindow::~FlutterWindow() {}

bool FlutterWindow::OnCreate() {
  if (!Win32Window::OnCreate()) {
    return false;
  }

  RECT frame = GetClientArea();

  HWND flutter_hwnd = nullptr;
  flutter::BinaryMessenger* messenger = nullptr;
  if (shared_engine_) {
    // Exactly one Engine hosts all main/Editor Flutter Views by default.
    // App channels bind to main HWND; their messenger is shared in the isolate.
    g_shared_native_messenger = nullptr;
    MultiViewDesktopPrepareEngine(project_, GetHandle());
    MultiViewDesktopCreateMainView(
        GetHandle(), frame.right - frame.left, frame.bottom - frame.top,
        RegisterSharedEnginePlugins);
    flutter_hwnd =
        MultiViewDesktopGetFlutterHwnd(MultiViewDesktopGetMainViewId());
    messenger = g_shared_native_messenger;
  } else {
    // Original single-view runner for detached child processes and the
    // explicit --alera-legacy-detached-tabs recovery option.
    flutter_controller_ = std::make_unique<flutter::FlutterViewController>(
        frame.right - frame.left, frame.bottom - frame.top, project_);
    if (!flutter_controller_->engine() || !flutter_controller_->view()) {
      return false;
    }
    RegisterPlugins(flutter_controller_->engine());
    flutter_hwnd = flutter_controller_->view()->GetNativeWindow();
    messenger = flutter_controller_->engine()->messenger();
  }
  if (!flutter_hwnd || !::IsWindow(flutter_hwnd) || !messenger) {
    return false;
  }
  SetChildContent(flutter_hwnd);
  flutter_view_hwnd_ = flutter_hwnd;
  flutter_view_ready_ = true;

  app_menu_channel_ =
      std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
          messenger,
          "dev.leynier.alera/app_menu",
          &flutter::StandardMethodCodec::GetInstance());

  desktop_presence_channel_ =
      std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
          messenger,
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
          messenger,
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
          messenger,
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
          messenger,
          "dev.leynier.alera/detached_tabs",
          &flutter::StandardMethodCodec::GetInstance());
  detached_tab_channel_->SetMethodCallHandler(
      [this](const flutter::MethodCall<flutter::EncodableValue>& call,
             std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>>
                 result) {
        if (call.method_name() == "startDragPreview") {
          std::wstring title = L"Tab";
          if (const auto* arguments =
                  std::get_if<flutter::EncodableMap>(call.arguments())) {
            auto found = arguments->find(flutter::EncodableValue("title"));
            if (found != arguments->end()) {
              if (const auto* utf8 =
                      std::get_if<std::string>(&found->second)) {
                title = PreviewTitleFromUtf8(*utf8);
              }
            }
          }
          tab_drag_preview_.Start(GetHandle(), title);
          result->Success();
          return;
        }
        if (call.method_name() == "stopDragPreview") {
          tab_drag_preview_.Stop();
          result->Success();
          return;
        }
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

  if (!shared_engine_) {
    flutter_controller_->engine()->SetNextFrameCallback([&]() {
      this->Show();
    });
    // Flutter can complete the first frame before the "show window" callback
    // is registered; request redraw using only the default controller.
    flutter_controller_->ForceRedraw();
  } else {
    // Shared Engine owns every FlutterViewController. The multiview plugin
    // drives first frames without a second runner-owned controller.
    this->Show();
  }

  return true;
}

void FlutterWindow::OnDestroy() {
  tab_drag_preview_.Stop();
  flutter_view_ready_ = false;
  flutter_view_hwnd_ = nullptr;
  detached_tab_channel_.reset();
  rich_clipboard_channel_.reset();
  file_drag_channel_.reset();
  desktop_presence_.Destroy();
  if (flutter_controller_) {
    flutter_controller_ = nullptr;
  }
  if (shared_engine_) {
    // Only the plugin owns the shared engine's native lifetime.
    g_shared_native_messenger = nullptr;
  }

  Win32Window::OnDestroy();
}

LRESULT
FlutterWindow::MessageHandler(HWND hwnd, UINT const message,
                              WPARAM const wparam,
                              LPARAM const lparam) noexcept {
  if (shared_engine_ && message == WM_CLOSE) {
    // The multi-view plugin would otherwise destroy the main engine's HWND
    // (and all Editor windows) before Alera's async quit/save gate runs.
    // Route only the native *main* close request back to the Dart lifecycle
    // coordinator; its committed destroy uses WM_DESTROY, not WM_CLOSE.
    if (detached_tab_channel_) {
      detached_tab_channel_->InvokeMethod("mainCloseRequested", nullptr);
    }
    return 0;
  }
  // A WM_PARENTNOTIFY/WM_DESTROY arrives while Flutter's child HWND is being
  // removed. Its embedder view may already be null even though the runner's
  // FlutterViewController still exists (GetEngine(nullptr) AV at +0x1e240).
  if ((message == WM_PARENTNOTIFY && LOWORD(wparam) == WM_DESTROY &&
       reinterpret_cast<HWND>(lparam) == flutter_view_hwnd_) ||
      message == WM_DESTROY || message == WM_NCDESTROY) {
    flutter_view_ready_ = false;
  }
  if (shared_engine_) {
    LRESULT result = 0;
    if (MultiViewDesktopHandleWindowProc(hwnd, message, wparam, lparam,
                                         &result)) {
      return result;
    }
  }
  const bool can_forward =
      flutter_controller_ &&
      ShouldForwardToFlutterView(
          flutter_view_ready_ && ::IsWindow(flutter_view_hwnd_), message, wparam);
  // Give Flutter, including plugins, an opportunity to handle window messages.
  if (can_forward) {
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
      if (shared_engine_ && MultiViewDesktopGetEngineRef()) {
        FlutterDesktopEngineReloadSystemFonts(MultiViewDesktopGetEngineRef());
      } else if (can_forward && flutter_controller_->engine()) {
        flutter_controller_->engine()->ReloadSystemFonts();
      }
      break;
  }

  if (desktop_presence_.HandleMessage(hwnd, message, wparam, lparam)) {
    return 0;
  }

  return Win32Window::MessageHandler(hwnd, message, wparam, lparam);
}
