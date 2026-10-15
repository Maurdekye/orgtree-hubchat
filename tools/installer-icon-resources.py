"""Extract shell icon resources without executing any PE; compare native pixels.

python tools/installer-icon-resources.py --output <new-own-C-directory> \
  --exe <contained-setup.exe> --exe <captured-uninstaller.exe> --exe <standin-hubchat.exe>
Requires Windows and Pillow, as tools/windows-icon-proof.py does.
"""
import argparse
import ctypes as C
from ctypes import wintypes as W
import hashlib
import importlib.util
import json
from pathlib import Path
import struct

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--exe', type=Path, action='append', required=True)
args = parser.parse_args()
if args.output.exists():
    raise RuntimeError('Use a new owned output directory')
args.output.mkdir(parents=True)
spec = importlib.util.spec_from_file_location('icon_native', Path(__file__).with_name('windows-icon-proof.py'))
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)
ico = Path(__file__).resolve().parents[1] / 'src-tauri/icons/icon.ico'
shell = C.WinDLL('shell32', use_last_error=True)
extract = native.api(shell, 'ExtractIconExW', W.UINT, W.LPCWSTR, C.c_int,
                     C.POINTER(W.HICON), C.POINTER(W.HICON), W.UINT)
find = native.api(native.kernel, 'FindResourceW', W.HANDLE, W.HMODULE, C.c_void_p, C.c_void_p)
load = native.api(native.kernel, 'LoadResource', W.HANDLE, W.HMODULE, W.HANDLE)
lock = native.api(native.kernel, 'LockResource', C.c_void_p, W.HANDLE)
size = native.api(native.kernel, 'SizeofResource', W.DWORD, W.HMODULE, W.HANDLE)
callback_type = C.WINFUNCTYPE(W.BOOL, W.HMODULE, C.c_void_p, C.c_void_p, C.c_ssize_t)
enum = native.api(native.kernel, 'EnumResourceNamesW', W.BOOL, W.HMODULE, C.c_void_p,
                  callback_type, C.c_ssize_t)

def resource(module, name, kind):
    handle = native.check(find(module, name, C.c_void_p(kind)))
    return C.string_at(native.check(lock(native.check(load(module, handle)))), size(module, handle))

results = []
for index, exe in enumerate(args.exe):
    module = native.check(native.load_library(str(exe.resolve()), None, 0x22))
    try:
        groups = []
        @callback_type
        def collect(instance, kind, name, context):
            # Resource-name pointers are valid only inside the callback.
            groups.append(resource(instance, name, 14))
            return True
        native.check(enum(module, C.c_void_p(14), collect, 0))
        if not groups:
            raise RuntimeError('No RT_GROUP_ICON in ' + str(exe))
        for group_index, group in enumerate(groups):
            reserved, kind, count = struct.unpack_from('<HHH', group)
            directory, frames = bytearray(), bytearray()
            offset = 6 + 16 * count
            for n in range(count):
                w, h, colors, zero, planes, bits, length, resource_id = struct.unpack_from('<BBBBHHIH', group, 6 + 14 * n)
                frame = resource(module, C.c_void_p(resource_id), 3)
                assert len(frame) == length
                directory.extend(struct.pack('<BBBBHHII', w, h, colors, zero, planes, bits, length, offset))
                frames.extend(frame)
                offset += length
            extracted = struct.pack('<HHH', reserved, kind, count) + directory + frames
            (args.output / f'{index}-group-{group_index}.ico').write_bytes(extracted)
        large, small = W.HICON(), W.HICON()
        try:
            extracted_count = extract(str(exe.resolve()), 0, C.byref(large), C.byref(small), 1)
            assert extracted_count > 0 and large and small, 'shell extraction must return both requested icon handles'
            for pixels, icon in [(16, small), (32, large)]:
                reference = native.check(native.load_image(None, C.cast(C.c_wchar_p(str(ico)), C.c_void_p), 1, pixels, pixels, 0x10))
                try:
                    actual, expected = native.render(icon, pixels), native.render(reference, pixels)
                    actual.save(args.output / f'{index}-shell-{pixels}.png')
                    expected.save(args.output / f'reference-{pixels}.png')
                    same = actual.tobytes() == expected.tobytes()
                    results.append({'exe': str(exe), 'pixels': pixels, 'matches_app_icon': same,
                                    'extracted_count': extracted_count,
                                    'actual_sha256': hashlib.sha256(actual.tobytes()).hexdigest(),
                                    'reference_sha256': hashlib.sha256(expected.tobytes()).hexdigest()})
                finally:
                    native.destroy_icon(reference)
        finally:
            if large:
                native.destroy_icon(large)
            if small:
                native.destroy_icon(small)
    finally:
        native.free_library(module)
(args.output / 'result.json').write_text(json.dumps(results, indent=2) + '\n')
print(json.dumps(results, indent=2))
assert all(row['matches_app_icon'] for row in results), 'embedded shell icon differs from existing Hubchat ICO'
