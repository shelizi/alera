#include "win32_rich_clipboard.h"

#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <thread>
#include <vector>
#include <wincodec.h>
#include <wrl/client.h>

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

// A real CF_DIB is needed for single-image paste into Word/Outlook and for
// Chromium email composers. Windows Imaging Component decodes the PNG without
// an external image codec or temporary file.
bool BuildDibFromPng(const std::vector<uint8_t>& png,
                     std::vector<uint8_t>& dib) {
  if (png.size() < 24 || png.size() > 4 * 1024 * 1024 ||
      std::memcmp(png.data(), "\x89PNG\r\n\x1a\n", 8) != 0) {
    return false;
  }
  const HRESULT initialized = CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);
  if (FAILED(initialized) && initialized != RPC_E_CHANGED_MODE) return false;
  const auto cleanup = [&]() {
    if (SUCCEEDED(initialized)) CoUninitialize();
  };
  bool ok = false;
  do {
    Microsoft::WRL::ComPtr<IWICImagingFactory> factory;
    if (FAILED(CoCreateInstance(CLSID_WICImagingFactory, nullptr,
                                CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&factory)))) {
      break;
    }
    Microsoft::WRL::ComPtr<IWICStream> stream;
    if (FAILED(factory->CreateStream(&stream)) ||
        FAILED(stream->InitializeFromMemory(const_cast<BYTE*>(png.data()),
                                            static_cast<DWORD>(png.size())))) {
      break;
    }
    Microsoft::WRL::ComPtr<IWICBitmapDecoder> decoder;
    if (FAILED(factory->CreateDecoderFromStream(
            stream.Get(), nullptr, WICDecodeMetadataCacheOnLoad, &decoder))) {
      break;
    }
    Microsoft::WRL::ComPtr<IWICBitmapFrameDecode> frame;
    if (FAILED(decoder->GetFrame(0, &frame))) break;
    Microsoft::WRL::ComPtr<IWICFormatConverter> converter;
    if (FAILED(factory->CreateFormatConverter(&converter)) ||
        FAILED(converter->Initialize(frame.Get(), GUID_WICPixelFormat32bppBGRA,
                                     WICBitmapDitherTypeNone, nullptr, 0.0,
                                     WICBitmapPaletteTypeCustom))) {
      break;
    }
    UINT width = 0;
    UINT height = 0;
    if (FAILED(converter->GetSize(&width, &height)) || width == 0 ||
        height == 0 || width > 4096 || height > 4096 ||
        static_cast<uint64_t>(width) * height > 16 * 1024 * 1024) {
      break;
    }
    const auto stride = static_cast<size_t>(width) * 4;
    const auto pixel_bytes = stride * height;
    std::vector<uint8_t> pixels(pixel_bytes);
    if (FAILED(converter->CopyPixels(nullptr, static_cast<UINT>(stride),
                                     static_cast<UINT>(pixel_bytes), pixels.data()))) {
      break;
    }
    BITMAPINFOHEADER header = {};
    header.biSize = sizeof(BITMAPINFOHEADER);
    header.biWidth = static_cast<LONG>(width);
    header.biHeight = static_cast<LONG>(height);  // bottom-up DIB scanlines
    header.biPlanes = 1;
    header.biBitCount = 32;
    header.biCompression = BI_RGB;
    header.biSizeImage = static_cast<DWORD>(pixel_bytes);
    dib.resize(sizeof(header) + pixel_bytes);
    std::memcpy(dib.data(), &header, sizeof(header));
    for (UINT y = 0; y < height; ++y) {
      const auto* src = pixels.data() + y * stride;
      auto* dst = dib.data() + sizeof(header) + (height - 1 - y) * stride;
      for (UINT x = 0; x < width; ++x) {
        const auto alpha = static_cast<unsigned>(src[x * 4 + 3]);
        for (int component = 0; component < 3; ++component) {
          dst[x * 4 + component] = static_cast<uint8_t>(
              (static_cast<unsigned>(src[x * 4 + component]) * alpha +
               255 * (255 - alpha)) / 255);
        }
        dst[x * 4 + 3] = 255;
      }
    }
    ok = true;
  } while (false);
  cleanup();
  return ok;
}

bool SetPngClipboard(HWND window, const std::vector<uint8_t>& png) {
  std::vector<uint8_t> dib;
  if (!BuildDibFromPng(png, dib)) return false;
  const UINT png_format = RegisterClipboardFormatW(L"PNG");
  if (!png_format) return false;
  HGLOBAL dib_memory = AllocateGlobal(dib.data(), dib.size());
  HGLOBAL png_memory = AllocateGlobal(png.data(), png.size());
  if (!dib_memory || !png_memory) {
    if (dib_memory) GlobalFree(dib_memory);
    if (png_memory) GlobalFree(png_memory);
    return false;
  }
  bool opened = false;
  for (int attempt = 0; attempt < 10; ++attempt) {
    if (OpenClipboard(window)) { opened = true; break; }
    std::this_thread::sleep_for(std::chrono::milliseconds(5));
  }
  if (!opened) {
    GlobalFree(dib_memory);
    GlobalFree(png_memory);
    return false;
  }
  bool ok = false;
  if (EmptyClipboard()) {
    if (SetClipboardData(CF_DIB, dib_memory)) {
      dib_memory = nullptr;
      if (SetClipboardData(png_format, png_memory)) {
        png_memory = nullptr;
        ok = true;
      }
    }
    if (!ok) EmptyClipboard();
  }
  CloseClipboard();
  if (dib_memory) GlobalFree(dib_memory);
  if (png_memory) GlobalFree(png_memory);
  return ok;
}

}  // namespace

void HandleAleraRichClipboardMethodCall(
    HWND window, const flutter::MethodCall<flutter::EncodableValue>& call,
    std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result) {
  if (call.method_name() == "setImagePng") {
    const auto* png = std::get_if<std::vector<uint8_t>>(call.arguments());
    if (!png || png->empty() || png->size() > 4 * 1024 * 1024) {
      result->Error("invalid-arguments", "Expected PNG clipboard bytes.");
    } else if (!SetPngClipboard(window, *png)) {
      result->Error("clipboard-error", "Unable to copy PNG image.");
    } else {
      result->Success();
    }
    return;
  }
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
