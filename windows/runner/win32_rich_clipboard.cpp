#include "win32_rich_clipboard.h"

#include <chrono>
#include <cstdio>
#include <cstring>
#include <string>
#include <thread>

namespace {

// CF_HTML offsets are byte offsets from the beginning of the UTF-8 payload,
// not UTF-16 character offsets. Word/Outlook need them to find the fragment.
std::string BuildCfHtml(const std::string& fragment) {
  const std::string before = "<html><body><!--StartFragment-->";
  const std::string after = "<!--EndFragment--></body></html>";
  char header[192] = {};
  constexpr const char* pattern =
      "Version:0.9\r\nStartHTML:%010llu\r\nEndHTML:%010llu\r\n"
      "StartFragment:%010llu\r\nEndFragment:%010llu\r\n";
  const auto header_bytes = std::snprintf(header, sizeof(header), pattern,
                                         0ULL, 0ULL, 0ULL, 0ULL);
  const auto start_html = static_cast<unsigned long long>(header_bytes);
  const auto start_fragment = start_html + before.size();
  const auto end_fragment = start_fragment + fragment.size();
  const auto end_html = end_fragment + after.size();
  std::snprintf(header, sizeof(header), pattern, start_html, end_html,
                start_fragment, end_fragment);
  return std::string(header) + before + fragment + after;
}

HGLOBAL AllocateGlobal(const void* data, size_t bytes) {
  HGLOBAL memory = GlobalAlloc(GMEM_MOVEABLE, bytes);
  if (!memory) return nullptr;
  void* destination = GlobalLock(memory);
  if (!destination) {
    GlobalFree(memory);
    return nullptr;
  }
  std::memcpy(destination, data, bytes);
  GlobalUnlock(memory);
  return memory;
}

std::wstring Utf8ToUtf16(const std::string& source) {
  if (source.empty()) return {};
  const int length = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS,
                                         source.data(),
                                         static_cast<int>(source.size()),
                                         nullptr, 0);
  if (length <= 0) return {};
  std::wstring wide(length, L'\0');
  if (MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, source.data(),
                          static_cast<int>(source.size()), wide.data(),
                          length) <= 0) {
    return {};
  }
  return wide;
}

bool SetRichClipboard(HWND window, const std::string& html,
                      const std::string& plain_text) {
  const auto format = RegisterClipboardFormatW(L"HTML Format");
  if (!format) return false;
  const auto cf_html = BuildCfHtml(html);
  const auto wide_text = Utf8ToUtf16(plain_text);
  if (!plain_text.empty() && wide_text.empty()) return false;
  HGLOBAL html_memory = AllocateGlobal(cf_html.c_str(), cf_html.size() + 1);
  HGLOBAL text_memory = AllocateGlobal(
      wide_text.c_str(), (wide_text.size() + 1) * sizeof(wchar_t));
  if (!html_memory || !text_memory) {
    if (html_memory) GlobalFree(html_memory);
    if (text_memory) GlobalFree(text_memory);
    return false;
  }

  bool opened = false;
  for (int attempt = 0; attempt < 10; ++attempt) {
    if (OpenClipboard(window)) {
      opened = true;
      break;
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(5));
  }
  if (!opened) {
    GlobalFree(html_memory);
    GlobalFree(text_memory);
    return false;
  }

  bool ok = false;
  if (EmptyClipboard()) {
    if (SetClipboardData(format, html_memory)) {
      html_memory = nullptr;
      if (SetClipboardData(CF_UNICODETEXT, text_memory)) {
        text_memory = nullptr;
        ok = true;
      }
    }
    if (!ok) EmptyClipboard();
  }
  CloseClipboard();
  if (html_memory) GlobalFree(html_memory);
  if (text_memory) GlobalFree(text_memory);
  return ok;
}

}  // namespace

void HandleAleraRichClipboardMethodCall(
    HWND window, const flutter::MethodCall<flutter::EncodableValue>& call,
    std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result) {
  if (call.method_name() != "setHtml") {
    result->NotImplemented();
    return;
  }
  const auto* values = std::get_if<flutter::EncodableMap>(call.arguments());
  if (!values) {
    result->Error("invalid-arguments", "Expected an HTML and text map.");
    return;
  }
  const auto html_it = values->find(flutter::EncodableValue("html"));
  const auto text_it = values->find(flutter::EncodableValue("text"));
  if (html_it == values->end() || text_it == values->end()) {
    result->Error("invalid-arguments", "Missing HTML or plain text.");
    return;
  }
  const auto* html = std::get_if<std::string>(&html_it->second);
  const auto* text = std::get_if<std::string>(&text_it->second);
  if (!html || !text || html->empty()) {
    result->Error("invalid-arguments", "Invalid HTML clipboard content.");
    return;
  }
  if (!SetRichClipboard(window, *html, *text)) {
    result->Error("clipboard-busy", "Unable to set the rich clipboard.");
    return;
  }
  result->Success();
}
