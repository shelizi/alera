#include "win32_single_instance.h"

#include <cstdlib>

Win32SingleInstance::Win32SingleInstance(const std::wstring& app_id)
    : object_name_(L"Local\\" + app_id + L".desktop") {}

Win32SingleInstance::~Win32SingleInstance() {
  if (owns_mutex_) {
    if (window_identity_) {
      ::InterlockedExchange(&window_identity_->process_id, 0);
      ::InterlockedExchange64(&window_identity_->hwnd, 0);
    }
    ::ReleaseMutex(mutex_);
  }
  if (window_identity_) {
    ::UnmapViewOfFile(window_identity_);
  }
  if (window_mapping_) {
    ::CloseHandle(window_mapping_);
  }
  if (mutex_) {
    ::CloseHandle(mutex_);
  }
  if (activation_event_) {
    ::CloseHandle(activation_event_);
  }
}

Win32SingleInstance::StartResult Win32SingleInstance::Start() {
  // Every contender opens the event before competing for the mutex. A second
  // launch can then queue activation even before the first creates its window.
  activation_event_ = ::CreateEventW(nullptr, FALSE, FALSE,
                                     (object_name_ + L".activate").c_str());
  mutex_ = ::CreateMutexW(nullptr, FALSE, (object_name_ + L".mutex").c_str());
  if (!activation_event_ || !mutex_) {
    return StartResult::failed;
  }
  // A fixed-size kernel mapping records the primary HWND/PID for transfer of
  // foreground rights. This never calls GlobalAddAtom/SetProp and survives a
  // session where all global atom slots are already exhausted.
  window_mapping_ = ::CreateFileMappingW(
      INVALID_HANDLE_VALUE, nullptr, PAGE_READWRITE, 0,
      sizeof(SharedWindowIdentity), (object_name_ + L".window").c_str());
  if (window_mapping_) {
    window_identity_ = static_cast<SharedWindowIdentity*>(
        ::MapViewOfFile(window_mapping_, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0,
                        sizeof(SharedWindowIdentity)));
  }

  const DWORD wait = ::WaitForSingleObject(mutex_, 0);
  if (wait == WAIT_OBJECT_0 || wait == WAIT_ABANDONED) {
    owns_mutex_ = true;
    if (window_identity_) {
      ::InterlockedExchange(&window_identity_->process_id, 0);
      ::InterlockedExchange64(&window_identity_->hwnd, 0);
    }
    return StartResult::primary;
  }
  if (wait != WAIT_TIMEOUT) {
    return StartResult::failed;
  }

  // Windows gives the freshly launched process foreground rights. Pass those
  // rights to the existing window without relying on its mutable title.
  AllowForegroundForPrimaryWindow();
  return ::SetEvent(activation_event_) ? StartResult::forwarded
                                       : StartResult::failed;
}

void Win32SingleInstance::AllowForegroundForPrimaryWindow() const {
  if (!window_identity_) {
    return;
  }
  const LONG pid =
      ::InterlockedCompareExchange(&window_identity_->process_id, 0, 0);
  const LONG64 hwnd_bits =
      ::InterlockedCompareExchange64(&window_identity_->hwnd, 0, 0);
  if (pid <= 0 || !hwnd_bits) {
    return;
  }
  const HWND window = reinterpret_cast<HWND>(static_cast<INT_PTR>(hwnd_bits));
  DWORD actual_pid = 0;
  if (::IsWindow(window)) {
    ::GetWindowThreadProcessId(window, &actual_pid);
  }
  if (actual_pid == static_cast<DWORD>(pid)) {
    ::AllowSetForegroundWindow(actual_pid);
  }
}

int Win32SingleInstance::RunMessageLoop(HWND window) {
  if (!owns_mutex_ || !window || !::IsWindow(window)) {
    return EXIT_FAILURE;
  }
  if (window_identity_) {
    ::InterlockedExchange64(&window_identity_->hwnd,
                            reinterpret_cast<LONG64>(window));
    ::InterlockedExchange(&window_identity_->process_id,
                          static_cast<LONG>(::GetCurrentProcessId()));
  }

  int exit_code = EXIT_SUCCESS;
  bool running = true;
  while (running) {
    // INPUTAVAILABLE also wakes for messages Flutter has already inspected.
    // The event keeps idle instances asleep and retains early launch requests.
    const DWORD wait = ::MsgWaitForMultipleObjectsEx(
        1, &activation_event_, INFINITE, QS_ALLINPUT, MWMO_INPUTAVAILABLE);
    if (wait == WAIT_FAILED) {
      exit_code = EXIT_FAILURE;
      break;
    }
    if (wait == WAIT_OBJECT_0) {
      ::ShowWindow(window, ::IsIconic(window) ? SW_RESTORE : SW_SHOW);
      ::SetForegroundWindow(window);
    }

    MSG message{};
    while (::PeekMessageW(&message, nullptr, 0, 0, PM_REMOVE)) {
      if (message.message == WM_QUIT) {
        exit_code = static_cast<int>(message.wParam);
        running = false;
        break;
      }
      ::TranslateMessage(&message);
      ::DispatchMessageW(&message);
    }
  }
  if (window_identity_) {
    ::InterlockedExchange(&window_identity_->process_id, 0);
    ::InterlockedExchange64(&window_identity_->hwnd, 0);
  }
  return exit_code;
}
