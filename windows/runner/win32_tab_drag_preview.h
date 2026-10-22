#ifndef RUNNER_WIN32_TAB_DRAG_PREVIEW_H_
#define RUNNER_WIN32_TAB_DRAG_PREVIEW_H_

#include <windows.h>

#include <string>

// Flutter drag avatars cannot paint beyond their native HWND. This small,
// click-through desktop window follows the cursor only while it is outside
// the primary Alera window, making a torn-off tab visible on other monitors.
class Win32TabDragPreview {
 public:
  void Start(HWND source_window, const std::wstring& title);
  void Stop();

 private:
  void Update();
  void Paint(HWND hwnd);
  static LRESULT CALLBACK WindowProcedure(HWND hwnd, UINT message,
                                          WPARAM wparam, LPARAM lparam);

  HWND source_window_ = nullptr;
  HWND preview_window_ = nullptr;
  std::wstring title_;
  int width_ = 240;
  int height_ = 62;
  UINT dpi_ = 96;
};

#endif  // RUNNER_WIN32_TAB_DRAG_PREVIEW_H_
