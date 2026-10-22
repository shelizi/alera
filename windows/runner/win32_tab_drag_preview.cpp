#include "win32_tab_drag_preview.h"

#include <algorithm>

namespace {
constexpr wchar_t kClassName[] = L"AleraDetachedTabDragPreview";
constexpr UINT_PTR kFollowCursorTimer = 1;

int DpiScale(int value, UINT dpi) {
  return ::MulDiv(value, static_cast<int>(dpi), 96);
}
}  // namespace

void Win32TabDragPreview::Start(HWND source_window,
                               const std::wstring& title) {
  Stop();
  if (!source_window || !::IsWindow(source_window)) return;
  source_window_ = source_window;
  title_ = title.empty() ? L"Tab" : title;
  dpi_ = ::GetDpiForWindow(source_window_);
  if (dpi_ == 0) dpi_ = 96;
  width_ = DpiScale(240, dpi_);
  height_ = DpiScale(62, dpi_);

  WNDCLASSW window_class{};
  window_class.lpfnWndProc = WindowProcedure;
  window_class.hInstance = ::GetModuleHandleW(nullptr);
  window_class.lpszClassName = kClassName;
  window_class.hCursor = ::LoadCursorW(nullptr, IDC_ARROW);
  if (!::RegisterClassW(&window_class) &&
      ::GetLastError() != ERROR_CLASS_ALREADY_EXISTS) {
    return;
  }

  // Do not activate, take mouse input, appear in Alt+Tab, or cover the real
  // drop target in WindowFromPoint. The pointer sits outside this bubble.
  preview_window_ = ::CreateWindowExW(
      WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW |
          WS_EX_NOACTIVATE,
      kClassName, L"", WS_POPUP, 0, 0, width_, height_, nullptr, nullptr,
      ::GetModuleHandleW(nullptr), this);
  if (!preview_window_) return;
  ::SetLayeredWindowAttributes(preview_window_, 0, 232, LWA_ALPHA);
  ::SetTimer(preview_window_, kFollowCursorTimer, 24, nullptr);
  Update();
}

void Win32TabDragPreview::Stop() {
  if (preview_window_) {
    HWND hwnd = preview_window_;
    preview_window_ = nullptr;
    ::KillTimer(hwnd, kFollowCursorTimer);
    ::DestroyWindow(hwnd);
  }
  source_window_ = nullptr;
  title_.clear();
}

void Win32TabDragPreview::Update() {
  if (!preview_window_) return;
  if (!source_window_ || !::IsWindow(source_window_)) {
    Stop();
    return;
  }
  POINT cursor{};
  if (!::GetCursorPos(&cursor)) return;
  const HWND hit = ::WindowFromPoint(cursor);
  const bool over_primary =
      hit && ::GetAncestor(hit, GA_ROOT) == source_window_;
  if (over_primary) {
    ::ShowWindow(preview_window_, SW_HIDE);
    return;
  }
  const int offset = DpiScale(20, dpi_);
  int x = cursor.x + offset;
  int y = cursor.y + offset;
  // Follow across monitors (including negative screen coordinates), but keep
  // the bubble visible near each monitor's bottom/right work-area edges.
  MONITORINFO monitor_info{sizeof(MONITORINFO)};
  if (::GetMonitorInfoW(
          ::MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST),
          &monitor_info)) {
    const RECT& work = monitor_info.rcWork;
    if (x + width_ > work.right) x = cursor.x - width_ - offset;
    if (y + height_ > work.bottom) y = cursor.y - height_ - offset;
    x = std::clamp(x, static_cast<int>(work.left),
                   std::max(static_cast<int>(work.left),
                            static_cast<int>(work.right) - width_));
    y = std::clamp(y, static_cast<int>(work.top),
                   std::max(static_cast<int>(work.top),
                            static_cast<int>(work.bottom) - height_));
  }
  ::SetWindowPos(preview_window_, HWND_TOPMOST, x, y, 0, 0,
                 SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW);
}

void Win32TabDragPreview::Paint(HWND hwnd) {
  PAINTSTRUCT ps{};
  HDC dc = ::BeginPaint(hwnd, &ps);
  RECT rect{0, 0, width_, height_};
  HBRUSH background = ::CreateSolidBrush(RGB(36, 40, 49));
  HPEN border = ::CreatePen(PS_SOLID, DpiScale(1, dpi_), RGB(120, 161, 255));
  HGDIOBJ old_brush = ::SelectObject(dc, background);
  HGDIOBJ old_pen = ::SelectObject(dc, border);
  ::RoundRect(dc, 0, 0, width_ - 1, height_ - 1, DpiScale(12, dpi_),
              DpiScale(12, dpi_));
  ::SelectObject(dc, old_pen);
  ::SelectObject(dc, old_brush);
  ::DeleteObject(border);
  ::DeleteObject(background);

  ::SetBkMode(dc, TRANSPARENT);
  HFONT font = ::CreateFontW(-DpiScale(13, dpi_), 0, 0, 0, FW_SEMIBOLD,
                            FALSE, FALSE, FALSE, DEFAULT_CHARSET,
                            OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
                            CLEARTYPE_QUALITY, DEFAULT_PITCH, L"Segoe UI");
  HGDIOBJ old_font = ::SelectObject(dc, font);
  ::SetTextColor(dc, RGB(244, 247, 255));
  RECT title_rect{DpiScale(14, dpi_), DpiScale(8, dpi_),
                  width_ - DpiScale(14, dpi_), DpiScale(30, dpi_)};
  ::DrawTextW(dc, title_.c_str(), -1, &title_rect,
              DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX);
  ::SelectObject(dc, old_font);
  ::DeleteObject(font);

  ::SetTextColor(dc, RGB(176, 199, 240));
  HFONT hint_font = ::CreateFontW(
      -DpiScale(11, dpi_), 0, 0, 0, FW_NORMAL, FALSE, FALSE, FALSE,
      DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
      CLEARTYPE_QUALITY, DEFAULT_PITCH, L"Segoe UI");
  old_font = ::SelectObject(dc, hint_font);
  RECT hint_rect{DpiScale(14, dpi_), DpiScale(33, dpi_),
                 width_ - DpiScale(14, dpi_), height_ - DpiScale(7, dpi_)};
  ::DrawTextW(dc, L"Release to open a new window", -1, &hint_rect,
              DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS);
  ::SelectObject(dc, old_font);
  ::DeleteObject(hint_font);
  ::EndPaint(hwnd, &ps);
}

LRESULT CALLBACK Win32TabDragPreview::WindowProcedure(
    HWND hwnd, UINT message, WPARAM wparam, LPARAM lparam) {
  auto* preview = reinterpret_cast<Win32TabDragPreview*>(
      ::GetWindowLongPtrW(hwnd, GWLP_USERDATA));
  if (message == WM_NCCREATE) {
    const auto* create = reinterpret_cast<CREATESTRUCTW*>(lparam);
    preview = static_cast<Win32TabDragPreview*>(create->lpCreateParams);
    ::SetWindowLongPtrW(hwnd, GWLP_USERDATA,
                        reinterpret_cast<LONG_PTR>(preview));
  }
  switch (message) {
    case WM_NCHITTEST:
      return HTTRANSPARENT;
    case WM_MOUSEACTIVATE:
      return MA_NOACTIVATE;
    case WM_TIMER:
      if (preview && wparam == kFollowCursorTimer) {
        preview->Update();
        return 0;
      }
      break;
    case WM_PAINT:
      if (preview) {
        preview->Paint(hwnd);
        return 0;
      }
      break;
  }
  return ::DefWindowProcW(hwnd, message, wparam, lparam);
}
