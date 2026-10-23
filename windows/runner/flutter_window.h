#ifndef RUNNER_FLUTTER_WINDOW_H_
#define RUNNER_FLUTTER_WINDOW_H_

#include <flutter/dart_project.h>
#include <flutter/encodable_value.h>
#include <flutter/flutter_view_controller.h>
#include <flutter/method_channel.h>

#include <memory>

#include "win32_desktop_presence.h"
#include "win32_tab_drag_preview.h"
#include "win32_window.h"

// A window that does nothing but host a Flutter view.
class FlutterWindow : public Win32Window {
 public:
  // Creates a new FlutterWindow hosting a Flutter view running |project|.
  explicit FlutterWindow(const flutter::DartProject& project,
                         bool shared_engine = false);
  virtual ~FlutterWindow();

 protected:
  // Win32Window:
  bool OnCreate() override;
  void OnDestroy() override;
  LRESULT MessageHandler(HWND window, UINT const message, WPARAM const wparam,
                         LPARAM const lparam) noexcept override;

 private:
  // The project to run.
  flutter::DartProject project_;
  // Normal Windows main uses a shared Engine. An independent detached child
  // or --alera-legacy-detached-tabs retains FlutterViewController.
  bool shared_engine_ = false;

  // The Flutter instance hosted by this window.
  std::unique_ptr<flutter::FlutterViewController> flutter_controller_;

  // The Flutter HWND must be attached and alive before native message forwarding.
  HWND flutter_view_hwnd_ = nullptr;
  bool flutter_view_ready_ = false;

  // Channel used to forward native app-menu activation into Dart.
  std::unique_ptr<flutter::MethodChannel<flutter::EncodableValue>>
      app_menu_channel_;

  std::unique_ptr<flutter::MethodChannel<flutter::EncodableValue>>
      desktop_presence_channel_;
  Win32DesktopPresence desktop_presence_;
  std::unique_ptr<flutter::MethodChannel<flutter::EncodableValue>>
      file_drag_channel_;
  std::unique_ptr<flutter::MethodChannel<flutter::EncodableValue>>
      rich_clipboard_channel_;
  // Native hit testing for docking a detached tab over the primary HWND.
  std::unique_ptr<flutter::MethodChannel<flutter::EncodableValue>>
      detached_tab_channel_;
  Win32TabDragPreview tab_drag_preview_;
};

#endif  // RUNNER_FLUTTER_WINDOW_H_
