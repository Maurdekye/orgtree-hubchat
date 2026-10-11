"""Offline Windows icon decoding/conversion evidence. Requires Python + Pillow.

Loads an optional executable only as a resource/data file; never starts the app.
Usage: python tools/windows-icon-proof.py --output <scratch-dir> [--exe <exe>]
"""
import argparse
import ctypes as C
from ctypes import wintypes as W
import hashlib
import json
from pathlib import Path

from PIL import Image, ImageDraw


class IconInfo(C.Structure):
    _fields_ = [("is_icon", W.BOOL), ("x", W.DWORD), ("y", W.DWORD),
                ("mask", W.HBITMAP), ("color", W.HBITMAP)]


class Bitmap(C.Structure):
    _fields_ = [("type", W.LONG), ("width", W.LONG), ("height", W.LONG),
                ("stride", W.LONG), ("planes", W.WORD), ("bits", W.WORD),
                ("data", C.c_void_p)]


class BitmapInfo(C.Structure):
    _fields_ = [("size", W.DWORD), ("width", W.LONG), ("height", W.LONG),
                ("planes", W.WORD), ("bits", W.WORD), ("compression", W.DWORD),
                ("image_size", W.DWORD), ("xppm", W.LONG), ("yppm", W.LONG),
                ("colors", W.DWORD), ("important", W.DWORD),
                ("palette", W.DWORD * 3)]


user = C.WinDLL("user32", use_last_error=True)
gdi = C.WinDLL("gdi32", use_last_error=True)
kernel = C.WinDLL("kernel32", use_last_error=True)
controls = C.WinDLL("comctl32", use_last_error=True)


def api(dll, name, result, *args):
    fn = getattr(dll, name)
    fn.restype, fn.argtypes = result, args
    return fn


load_image = api(user, "LoadImageW", W.HANDLE, W.HINSTANCE, C.c_void_p,
                 W.UINT, C.c_int, C.c_int, W.UINT)
get_icon_info = api(user, "GetIconInfo", W.BOOL, W.HICON, C.POINTER(IconInfo))
destroy_icon = api(user, "DestroyIcon", W.BOOL, W.HICON)
create_icon = api(user, "CreateIcon", W.HICON, W.HINSTANCE, C.c_int, C.c_int,
                  C.c_ubyte, C.c_ubyte, C.c_void_p, C.c_void_p)
draw_icon = api(user, "DrawIconEx", W.BOOL, W.HDC, C.c_int, C.c_int, W.HICON,
                C.c_int, C.c_int, W.UINT, W.HBRUSH, W.UINT)
create_dc = api(gdi, "CreateCompatibleDC", W.HDC, W.HDC)
delete_dc = api(gdi, "DeleteDC", W.BOOL, W.HDC)
delete_object = api(gdi, "DeleteObject", W.BOOL, W.HANDLE)
get_object = api(gdi, "GetObjectW", C.c_int, W.HANDLE, C.c_int, C.c_void_p)
get_bits = api(gdi, "GetDIBits", C.c_int, W.HDC, W.HBITMAP, W.UINT,
               W.UINT, C.c_void_p, C.POINTER(BitmapInfo), W.UINT)
create_dib = api(gdi, "CreateDIBSection", W.HBITMAP, W.HDC,
                 C.POINTER(BitmapInfo), W.UINT, C.POINTER(C.c_void_p), W.HANDLE, W.DWORD)
select_object = api(gdi, "SelectObject", W.HANDLE, W.HDC, W.HANDLE)
flush_gdi = api(gdi, "GdiFlush", W.BOOL)
load_library = api(kernel, "LoadLibraryExW", W.HMODULE, W.LPCWSTR, W.HANDLE, W.DWORD)
free_library = api(kernel, "FreeLibrary", W.BOOL, W.HMODULE)
create_list = api(controls, "ImageList_Create", W.HANDLE, C.c_int, C.c_int, W.UINT, C.c_int, C.c_int)
replace_icon = api(controls, "ImageList_ReplaceIcon", C.c_int, W.HANDLE, C.c_int, W.HICON)
draw_list = api(controls, "ImageList_Draw", W.BOOL, W.HANDLE, C.c_int, W.HDC, C.c_int, C.c_int, W.UINT)
destroy_list = api(controls, "ImageList_Destroy", W.BOOL, W.HANDLE)
create_window = api(user, "CreateWindowExW", W.HWND, W.DWORD, W.LPCWSTR, W.LPCWSTR,
                    W.DWORD, C.c_int, C.c_int, C.c_int, C.c_int, W.HWND, W.HMENU, W.HINSTANCE, C.c_void_p)
destroy_window = api(user, "DestroyWindow", W.BOOL, W.HWND)
send_message = api(user, "SendMessageW", C.c_ssize_t, W.HWND, W.UINT, W.WPARAM, W.LPARAM)


def check(value):
    if not value:
        raise C.WinError(C.get_last_error())
    return value


def info(width, height):
    result = BitmapInfo()
    result.size, result.width, result.height = 40, width, -height
    result.planes, result.bits = 1, 32
    return result


def read_bitmap(bitmap, width, height):
    data = C.create_string_buffer(width * height * 4)
    dc = check(create_dc(None))
    try:
        spec = info(width, height)
        assert get_bits(dc, bitmap, 0, height, data, C.byref(spec), 0) == height
        return bytearray(data.raw)
    finally:
        delete_dc(dc)


def tauri_rgba(icon, size):
    """Same native operations as Tauri 2.12.1 Image::from_icon_resource."""
    details = IconInfo()
    check(get_icon_info(icon, C.byref(details)))
    try:
        bitmap = Bitmap()
        check(get_object(details.color, C.sizeof(bitmap), C.byref(bitmap)))
        assert (bitmap.width, bitmap.height) == (size, size)
        data = read_bitmap(details.color, size, size)
        if all(a == 0 for a in data[3::4]):
            mask = read_bitmap(details.mask, size, size)
            for offset in range(0, len(data), 4):
                data[offset + 3] = 255 if mask[offset] == 0 else 0
        for offset in range(0, len(data), 4):
            data[offset], data[offset + 2] = data[offset + 2], data[offset]
        return bytes(data)
    finally:
        delete_object(details.color)
        delete_object(details.mask)


def tao_icon(rgba, size):
    """Same byte conversion/CreateIcon call as Tao 0.37.1 from_rgba."""
    data = bytearray(rgba)
    mask = bytes((a - 255) % 256 for a in data[3::4])
    for offset in range(0, len(data), 4):
        data[offset], data[offset + 2] = data[offset + 2], data[offset]
    return check(create_icon(None, size, size, 1, 32,
                             C.create_string_buffer(mask), C.create_string_buffer(bytes(data))))


def render(icon, size, flags=3, list_depth=None):
    dc = check(create_dc(None))
    pixels = C.c_void_p()
    spec = info(size, size)
    bitmap = check(create_dib(dc, C.byref(spec), 0, C.byref(pixels), None, 0))
    old = select_object(dc, bitmap)
    try:
        C.memmove(pixels, bytes([220, 220, 220, 255]) * size * size, size * size * 4)
        if list_depth is None:
            check(draw_icon(dc, 0, 0, icon, size, size, 0, None, flags))
        else:
            image_list = check(create_list(size, size, list_depth | 1, 1, 1))
            try:
                assert replace_icon(image_list, -1, icon) == 0
                check(draw_list(image_list, 0, dc, 0, 0, 1))
            finally:
                destroy_list(image_list)
        check(flush_gdi())
        raw = C.string_at(pixels, size * size * 4)
        return Image.frombytes("RGBA", (size, size), raw, "raw", "BGRA").convert("RGB")
    finally:
        select_object(dc, old)
        delete_object(bitmap)
        delete_dc(dc)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--exe", type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    ico = Path(__file__).resolve().parents[1] / "src-tauri/icons/icon.ico"
    sources = [("ico", None, C.cast(C.c_wchar_p(str(ico)), C.c_void_p), 0x10)]
    module = None
    if args.exe:
        # LOAD_LIBRARY_AS_DATAFILE | LOAD_LIBRARY_AS_IMAGE_RESOURCE: no execution.
        module = check(load_library(str(args.exe), None, 0x22))
        sources.append(("exe", module, C.c_void_p(32512), 0))
    rows, results = [], []
    try:
        for label, instance, resource, flags in sources:
            for size in [16, 20, 24, 32, 40, 48, 64]:
                icon = check(load_image(instance, resource, 1, size, size, flags))
                converted = None
                try:
                    rgba = tauri_rgba(icon, size)
                    converted = tao_icon(rgba, size)
                    native, roundtrip = render(icon, size), render(converted, size)
                    native.save(args.output / f"{label}-{size}-native.png")
                    roundtrip.save(args.output / f"{label}-{size}-roundtrip.png")
                    equal = native.tobytes() == roundtrip.tobytes()
                    results.append({"source": label, "size": size, "native_equals_roundtrip": equal,
                                    "native_sha256": hashlib.sha256(native.tobytes()).hexdigest(),
                                    "roundtrip_sha256": hashlib.sha256(roundtrip.tobytes()).hexdigest()})
                    rows.append((f"{label} {size}px", native, roundtrip))
                    if size == 32:
                        for draw_size in [16, 20, 24]:
                            before, after = render(icon, draw_size), render(converted, draw_size)
                            results.append({"source": label, "size": size, "draw_size": draw_size,
                                            "scaled_native_equals_roundtrip": before.tobytes() == after.tobytes()})
                            rows.append((f"{label} 32->{draw_size}", before, after))
                        before, after = render(icon, size, 1), render(converted, size, 1)
                        rows.append((f"{label} AND mask", before, after))
                        results.append({"source": label, "size": size,
                                        "mask_native_equals_roundtrip": before.tobytes() == after.tobytes()})
                        for depth in [24, 32]:
                            before, after = render(icon, 16, list_depth=depth), render(converted, 16, list_depth=depth)
                            before.save(args.output / f"{label}-list{depth}-native.png")
                            after.save(args.output / f"{label}-list{depth}-roundtrip.png")
                            rows.append((f"{label} list{depth}", before, after))
                            results.append({"source": label, "size": size, "list_depth": depth,
                                            "list_native_equals_roundtrip": before.tobytes() == after.tobytes()})
                        # A hidden built-in window exercises the actual WM_SETICON /
                        # WM_GETICON transport without starting Hubchat or showing UI.
                        window = check(create_window(0, "STATIC", "offline icon proof", 0,
                                                      0, 0, 32, 32, None, None, None, None))
                        small = check(load_image(instance, resource, 1, 16, 16, flags))
                        try:
                            send_message(window, 0x80, 0, converted)
                            before_handle = send_message(window, 0x7f, 0, 0)
                            assert before_handle == converted
                            before = render(before_handle, 16, list_depth=24)
                            send_message(window, 0x80, 0, small)
                            send_message(window, 0x80, 1, icon)
                            after_handle = send_message(window, 0x7f, 0, 0)
                            assert after_handle == small
                            assert send_message(window, 0x7f, 1, 0) == icon
                            after = render(after_handle, 16, list_depth=24)
                            expected = render(small, 16, list_depth=24)
                            assert after.tobytes() == expected.tobytes()
                            assert before.tobytes() != after.tobytes()
                            before.save(args.output / f"{label}-window-before.png")
                            after.save(args.output / f"{label}-window-after.png")
                            rows.append((f"{label} window fix", before, after))
                            results.append({"source": label, "window_assignment_matches_native": True,
                                            "converted_window_differs": True})
                        finally:
                            destroy_window(window)
                            destroy_icon(small)
                finally:
                    destroy_icon(icon)
                    if converted:
                        destroy_icon(converted)
    finally:
        if module:
            free_library(module)
    sheet = Image.new("RGB", (420, 24 + len(rows) * 82), "white")
    draw = ImageDraw.Draw(sheet)
    draw.text((130, 4), "native Windows", fill="black")
    draw.text((265, 4), "Tauri/Tao roundtrip", fill="black")
    for row, (label, native, roundtrip) in enumerate(rows):
        y = 24 + row * 82
        draw.text((4, y + 28), label, fill="black")
        for x, img in [(140, native), (285, roundtrip)]:
            sheet.paste(img.resize((72, 72), Image.Resampling.NEAREST), (x, y))
    sheet.save(args.output / "comparison.png")
    (args.output / "result.json").write_text(json.dumps(results, indent=2) + "\n")
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
