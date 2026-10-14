#ifndef RUNNER_WIN32_FILE_DRAG_H_
#define RUNNER_WIN32_FILE_DRAG_H_

#include <windows.h>

#include <flutter/encodable_value.h>
#include <flutter/method_channel.h>

#include <memory>

// Windows Shell CF_HDROP drag source, exposed to the Explorer file tree.
// No Dart package dependencies and no filesystem mutations by the source.
void HandleAleraFileDragMethodCall(
    HWND owner,
    const flutter::MethodCall<flutter::EncodableValue>& call,
    std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result);

#endif  // RUNNER_WIN32_FILE_DRAG_H_
