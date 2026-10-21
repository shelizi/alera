#ifndef RUNNER_WIN32_SINGLE_INSTANCE_H_
#define RUNNER_WIN32_SINGLE_INSTANCE_H_

#include <windows.h>

#include <string>

// Owned by the startup thread until the Flutter window has been destroyed.
class Win32SingleInstance {
 public:
  enum class StartResult { primary, forwarded, failed };

  explicit Win32SingleInstance(const std::wstring& app_id);
  ~Win32SingleInstance();
  Win32SingleInstance(const Win32SingleInstance&) = delete;
  Win32SingleInstance& operator=(const Win32SingleInstance&) = delete;

  StartResult Start();
  int RunMessageLoop(HWND window);

 private:
  // Shared through a named page-file mapping, not a global Win32 atom.
  // SetPropW can fail with ERROR_NOT_ENOUGH_MEMORY when the session-wide
  // global atom table is exhausted. That must not prevent Alera from opening.
  struct alignas(8) SharedWindowIdentity {
    volatile LONG64 hwnd = 0;
    volatile LONG process_id = 0;
    LONG reserved = 0;
  };

  void AllowForegroundForPrimaryWindow() const;

  const std::wstring object_name_;
  HANDLE window_mapping_ = nullptr;
  SharedWindowIdentity* window_identity_ = nullptr;
  HANDLE mutex_ = nullptr;
  HANDLE activation_event_ = nullptr;
  bool owns_mutex_ = false;
};

#endif  // RUNNER_WIN32_SINGLE_INSTANCE_H_
