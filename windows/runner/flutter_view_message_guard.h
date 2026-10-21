#ifndef RUNNER_FLUTTER_VIEW_MESSAGE_GUARD_H_
#define RUNNER_FLUTTER_VIEW_MESSAGE_GUARD_H_

#include <windows.h>

// Flutter's Windows embedder can synchronously send WM_PARENTNOTIFY with
// WM_DESTROY while its child HWND is being torn down. Forwarding that event
// back through FlutterDesktopViewControllerHandleTopLevelWindowProc can try to
// call FlutterWindowsView::GetEngine() after the view has been removed.
// Keep regular parent notifications and all live-view messages intact.
inline bool ShouldForwardToFlutterView(bool view_ready, UINT message,
                                       WPARAM wparam) noexcept {
  if (!view_ready || message == WM_DESTROY || message == WM_NCDESTROY) {
    return false;
  }
  return message != WM_PARENTNOTIFY || LOWORD(wparam) != WM_DESTROY;
}

#endif  // RUNNER_FLUTTER_VIEW_MESSAGE_GUARD_H_
