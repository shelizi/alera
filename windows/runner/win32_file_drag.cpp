#include "win32_file_drag.h"

#include <ole2.h>
#include <shellapi.h>
#include <shlobj.h>

#include <atomic>
#include <cstring>
#include <filesystem>
#include <limits>
#include <new>
#include <string>
#include <utility>
#include <vector>

namespace {

std::wstring Utf8ToWide(const std::string& input) {
  if (input.empty()) return {};
  const int length = ::MultiByteToWideChar(
      CP_UTF8, MB_ERR_INVALID_CHARS, input.c_str(),
      static_cast<int>(input.size()), nullptr, 0);
  if (length <= 0) return {};
  std::wstring result(length, L'\0');
  if (::MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS,
                            input.c_str(), static_cast<int>(input.size()),
                            result.data(), length) != length) {
    return {};
  }
  return result;
}

// CF_HDROP lets Explorer decide the default move/copy action based on source
// and target drives. Ctrl forces copy; Shift forces move, as with shell files.
class ExplorerFileDataObject final : public IDataObject {
 public:
  explicit ExplorerFileDataObject(std::vector<std::wstring> paths)
      : paths_(std::move(paths)) {}

  HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void** object) override {
    if (!object) return E_POINTER;
    *object = nullptr;
    if (iid != IID_IUnknown && iid != IID_IDataObject) {
      return E_NOINTERFACE;
    }
    *object = static_cast<IDataObject*>(this);
    AddRef();
    return S_OK;
  }

  ULONG STDMETHODCALLTYPE AddRef() override { return ++refs_; }
  ULONG STDMETHODCALLTYPE Release() override {
    const ULONG remaining = --refs_;
    if (!remaining) delete this;
    return remaining;
  }

  HRESULT STDMETHODCALLTYPE QueryGetData(FORMATETC* format) override {
    if (!format) return E_POINTER;
    if (format->cfFormat != CF_HDROP ||
        !(format->tymed & TYMED_HGLOBAL) ||
        format->dwAspect != DVASPECT_CONTENT ||
        format->lindex != -1) {
      return DV_E_FORMATETC;
    }
    return S_OK;
  }

  HRESULT STDMETHODCALLTYPE GetData(FORMATETC* format,
                                    STGMEDIUM* medium) override {
    if (!medium) return E_POINTER;
    *medium = {};
    const HRESULT valid = QueryGetData(format);
    if (FAILED(valid)) return valid;

    size_t chars = 1;  // Double-null terminated path list.
    for (const auto& path : paths_) {
      if (path.size() > (std::numeric_limits<size_t>::max() - chars - 1)) {
        return E_OUTOFMEMORY;
      }
      chars += path.size() + 1;
    }
    const size_t maximum = std::numeric_limits<size_t>::max();
    if (chars > (maximum - sizeof(DROPFILES)) / sizeof(wchar_t)) {
      return E_OUTOFMEMORY;
    }
    const size_t bytes = sizeof(DROPFILES) + chars * sizeof(wchar_t);
    HGLOBAL global = ::GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes);
    if (!global) return E_OUTOFMEMORY;
    void* buffer = ::GlobalLock(global);
    if (!buffer) {
      ::GlobalFree(global);
      return E_OUTOFMEMORY;
    }
    auto* header = static_cast<DROPFILES*>(buffer);
    header->pFiles = sizeof(DROPFILES);
    header->fWide = TRUE;
    auto* names = reinterpret_cast<wchar_t*>(
        static_cast<unsigned char*>(buffer) + sizeof(DROPFILES));
    size_t cursor = 0;
    for (const auto& path : paths_) {
      const size_t size = (path.size() + 1) * sizeof(wchar_t);
      std::memcpy(names + cursor, path.c_str(), size);
      cursor += path.size() + 1;
    }
    ::GlobalUnlock(global);
    medium->tymed = TYMED_HGLOBAL;
    medium->hGlobal = global;
    medium->pUnkForRelease = nullptr;
    return S_OK;
  }

  HRESULT STDMETHODCALLTYPE GetDataHere(FORMATETC*, STGMEDIUM*) override {
    return DATA_E_FORMATETC;
  }
  HRESULT STDMETHODCALLTYPE GetCanonicalFormatEtc(FORMATETC*,
                                                  FORMATETC* output) override {
    if (output) output->ptd = nullptr;
    return DATA_S_SAMEFORMATETC;
  }
  HRESULT STDMETHODCALLTYPE SetData(FORMATETC* format, STGMEDIUM* medium,
                                    BOOL release) override {
    if (!format || !medium) return E_POINTER;
    // Explorer reports its completed action using this clipboard format.
    // Explorer itself transfers the files: never delete a source a second time.
    const auto performed = ::RegisterClipboardFormatW(CFSTR_PERFORMEDDROPEFFECT);
    const auto logical = ::RegisterClipboardFormatW(CFSTR_LOGICALPERFORMEDDROPEFFECT);
    if (format->cfFormat != performed && format->cfFormat != logical) {
      return E_NOTIMPL;
    }
    if (release) ::ReleaseStgMedium(medium);
    return S_OK;
  }
  HRESULT STDMETHODCALLTYPE EnumFormatEtc(DWORD direction,
                                          IEnumFORMATETC** enumerator) override {
    if (!enumerator) return E_POINTER;
    *enumerator = nullptr;
    if (direction != DATADIR_GET) return E_NOTIMPL;
    FORMATETC format{CF_HDROP, nullptr, DVASPECT_CONTENT, -1, TYMED_HGLOBAL};
    return ::SHCreateStdEnumFmtEtc(1, &format, enumerator);
  }
  HRESULT STDMETHODCALLTYPE DAdvise(FORMATETC*, DWORD, IAdviseSink*,
                                     DWORD*) override {
    return OLE_E_ADVISENOTSUPPORTED;
  }
  HRESULT STDMETHODCALLTYPE DUnadvise(DWORD) override {
    return OLE_E_ADVISENOTSUPPORTED;
  }
  HRESULT STDMETHODCALLTYPE EnumDAdvise(IEnumSTATDATA**) override {
    return OLE_E_ADVISENOTSUPPORTED;
  }

 private:
  ~ExplorerFileDataObject() = default;
  std::atomic<ULONG> refs_{1};
  std::vector<std::wstring> paths_;
};

bool IsCursorOverOtherWindow(HWND owner) {
  POINT point{};
  if (!::GetCursorPos(&point)) return false;
  HWND hit = ::WindowFromPoint(point);
  if (!hit) return true;
  return ::GetAncestor(hit, GA_ROOT) != ::GetAncestor(owner, GA_ROOT);
}

}  // namespace

void HandleAleraFileDragMethodCall(
    HWND owner,
    const flutter::MethodCall<flutter::EncodableValue>& call,
    std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result) {
  if (call.method_name() == "cursorOutsideWindow") {
    result->Success(flutter::EncodableValue(IsCursorOverOtherWindow(owner)));
    return;
  }
  if (call.method_name() != "startDrag") {
    result->NotImplemented();
    return;
  }
  if ((::GetAsyncKeyState(VK_LBUTTON) & 0x8000) == 0) {
    result->Success(flutter::EncodableValue("cancelled"));
    return;
  }

  const auto* args = std::get_if<flutter::EncodableMap>(call.arguments());
  if (!args) {
    result->Error("invalid_paths", "A list of file paths is required.");
    return;
  }
  const auto it = args->find(flutter::EncodableValue("paths"));
  if (it == args->end()) {
    result->Error("invalid_paths", "Missing paths.");
    return;
  }
  const auto* values = std::get_if<flutter::EncodableList>(&it->second);
  if (!values || values->empty() || values->size() > 512) {
    result->Error("invalid_paths", "Select between 1 and 512 files.");
    return;
  }
  std::vector<std::wstring> paths;
  paths.reserve(values->size());
  for (const auto& value : *values) {
    const auto* utf8 = std::get_if<std::string>(&value);
    if (!utf8) {
      result->Error("invalid_paths", "All paths must be strings.");
      return;
    }
    std::wstring path = Utf8ToWide(*utf8);
    if (path.empty() || !std::filesystem::path(path).is_absolute() ||
        ::GetFileAttributesW(path.c_str()) == INVALID_FILE_ATTRIBUTES) {
      result->Error("invalid_paths", "One of the selected paths does not exist.");
      return;
    }
    paths.push_back(std::move(path));
  }

  const HRESULT initialized = ::OleInitialize(nullptr);
  if (FAILED(initialized)) {
    result->Error("ole_unavailable", "Windows OLE drag-and-drop could not start.");
    return;
  }
  auto* data = new (std::nothrow) ExplorerFileDataObject(std::move(paths));
  if (!data) {
    ::OleUninitialize();
    result->Error("out_of_memory", "File drag allocation failed.");
    return;
  }

  // Flutter holds mouse capture while dragging within its own view. Release
  // that capture when transferring the drag gesture to the Windows Shell.
  ::ReleaseCapture();
  DWORD effect = DROPEFFECT_NONE;
  const HRESULT status = ::SHDoDragDrop(
      nullptr, data, nullptr, DROPEFFECT_COPY | DROPEFFECT_MOVE, &effect);
  data->Release();
  ::OleUninitialize();

  if (status == DRAGDROP_S_CANCEL) {
    result->Success(flutter::EncodableValue("cancelled"));
  } else if (status == DRAGDROP_S_DROP) {
    result->Success(flutter::EncodableValue(
        (effect & DROPEFFECT_MOVE) != 0 ? "move" :
        (effect & DROPEFFECT_COPY) != 0 ? "copy" : "none"));
  } else {
    result->Error("drag_failed", "Windows could not complete the file drag.");
  }
}
