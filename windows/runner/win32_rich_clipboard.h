#ifndef RUNNER_WIN32_RICH_CLIPBOARD_H_
#define RUNNER_WIN32_RICH_CLIPBOARD_H_

#include <flutter/encodable_value.h>
#include <flutter/method_channel.h>
#include <windows.h>

#include <memory>

void HandleAleraRichClipboardMethodCall(
    HWND window, const flutter::MethodCall<flutter::EncodableValue>& call,
    std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result);

#endif  // RUNNER_WIN32_RICH_CLIPBOARD_H_
