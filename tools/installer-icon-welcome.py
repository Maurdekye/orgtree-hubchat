"""Contained NSIS Welcome icon/capture proof; never advances installation.

python tools/installer-icon-welcome.py --setup <own-C/Hubchat Icon Test_...exe> --output <new-own-C-output>
Run in the rig's shared lock/kill-on-close Job. Requires Windows and Pillow.
Only starts a PE whose ProductName is exactly Hubchat Icon Test. Refuses an
existing disposable install/key. Sends only Cancel/Yes-to-quit, never Next.
"""
import argparse
import ctypes as C
from ctypes import wintypes as W
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import time
import winreg

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--setup', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
setup, out = args.setup.resolve(), args.output.resolve()
if len(setup.parts) < 4 or setup.parts[0].lower() != 'c:\\' or setup.parts[1].lower() != 'orgtree-build':
    raise RuntimeError('Setup must be under an owned C:/orgtree-build/<agent> scope')
scope = Path(*setup.parts[:3])
if scope not in out.parents or out.exists() or not setup.name.startswith('Hubchat Icon Test_') or setup.suffix.lower() != '.exe':
    raise RuntimeError('Require disposable setup and a new output in the same granted C agent scope')
spec = importlib.util.spec_from_file_location('icon_native', Path(__file__).with_name('windows-icon-proof.py'))
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)
version = C.WinDLL('version', use_last_error=True)
version_size = native.api(version, 'GetFileVersionInfoSizeW', W.DWORD, W.LPCWSTR, C.POINTER(W.DWORD))
version_read = native.api(version, 'GetFileVersionInfoW', W.BOOL, W.LPCWSTR, W.DWORD, W.DWORD, C.c_void_p)
version_query = native.api(version, 'VerQueryValueW', W.BOOL, C.c_void_p, W.LPCWSTR, C.POINTER(C.c_void_p), C.POINTER(W.UINT))
unused = W.DWORD()
size = native.check(version_size(str(setup), C.byref(unused)))
data = C.create_string_buffer(size)
native.check(version_read(str(setup), 0, size, data))
pointer, length = C.c_void_p(), W.UINT()
native.check(version_query(data, r'\VarFileInfo\Translation', C.byref(pointer), C.byref(length)))
language, codepage = C.cast(pointer, C.POINTER(W.WORD))[:2]
native.check(version_query(data, f'\\StringFileInfo\\{language:04x}{codepage:04x}\\ProductName', C.byref(pointer), C.byref(length)))
if C.wstring_at(pointer) != 'Hubchat Icon Test':
    raise RuntimeError('Refusing a PE that is not the disposable test product')

def install_state():
    keys = []
    for root in [winreg.HKEY_CURRENT_USER, winreg.HKEY_LOCAL_MACHINE]:
        for view in [winreg.KEY_WOW64_32KEY, winreg.KEY_WOW64_64KEY]:
            try:
                with winreg.OpenKey(root, r'Software\Microsoft\Windows\CurrentVersion\Uninstall\Hubchat Icon Test',
                                    0, winreg.KEY_READ | view):
                    keys.append((int(root), view))
            except FileNotFoundError:
                pass
    return {'keys': keys, 'install_exists': (Path(os.environ['LOCALAPPDATA']) / 'Hubchat Icon Test').exists()}

before = install_state()
if before['keys'] or before['install_exists']:
    raise RuntimeError('Disposable installation/key already exists; not mine to reuse')
out.mkdir(parents=True)
(out / 'tmp').mkdir()
result = {'setup': str(setup), 'setup_sha256': hashlib.sha256(setup.read_bytes()).hexdigest(),
          'scope': 'Disposable Welcome page only; no Next, install, ARP or live-product writes',
          'before': before, 'forced_cleanup': [], 'icons': []}

class ProcessEntry(C.Structure):
    _fields_ = [('size', W.DWORD), ('usage', W.DWORD), ('pid', W.DWORD),
                ('heap', C.c_size_t), ('module', W.DWORD), ('threads', W.DWORD),
                ('parent', W.DWORD), ('priority', W.LONG), ('flags', W.DWORD), ('exe', W.WCHAR * 260)]

snapshot = native.api(native.kernel, 'CreateToolhelp32Snapshot', W.HANDLE, W.DWORD, W.DWORD)
first = native.api(native.kernel, 'Process32FirstW', W.BOOL, W.HANDLE, C.POINTER(ProcessEntry))
next_process = native.api(native.kernel, 'Process32NextW', W.BOOL, W.HANDLE, C.POINTER(ProcessEntry))
close_handle = native.api(native.kernel, 'CloseHandle', W.BOOL, W.HANDLE)
open_process = native.api(native.kernel, 'OpenProcess', W.HANDLE, W.DWORD, W.BOOL, W.DWORD)
image_path = native.api(native.kernel, 'QueryFullProcessImageNameW', W.BOOL, W.HANDLE, W.DWORD, W.LPWSTR, C.POINTER(W.DWORD))
wait = native.api(native.kernel, 'WaitForSingleObject', W.DWORD, W.HANDLE, W.DWORD)
terminate = native.api(native.kernel, 'TerminateProcess', W.BOOL, W.HANDLE, W.UINT)
process_times = native.api(native.kernel, 'GetProcessTimes', W.BOOL, W.HANDLE,
                           C.POINTER(W.FILETIME), C.POINTER(W.FILETIME), C.POINTER(W.FILETIME), C.POINTER(W.FILETIME))
window_callback = C.WINFUNCTYPE(W.BOOL, W.HWND, C.c_ssize_t)
enum_windows = native.api(native.user, 'EnumWindows', W.BOOL, window_callback, C.c_ssize_t)
enum_children = native.api(native.user, 'EnumChildWindows', W.BOOL, W.HWND, window_callback, C.c_ssize_t)
window_pid = native.api(native.user, 'GetWindowThreadProcessId', W.DWORD, W.HWND, C.POINTER(W.DWORD))
window_text = native.api(native.user, 'GetWindowTextW', C.c_int, W.HWND, W.LPWSTR, C.c_int)
visible = native.api(native.user, 'IsWindowVisible', W.BOOL, W.HWND)
send = native.api(native.user, 'SendMessageTimeoutW', C.c_ssize_t, W.HWND, W.UINT,
                  C.c_size_t, C.c_ssize_t, W.UINT, W.UINT, C.POINTER(C.c_size_t))
post = native.api(native.user, 'PostMessageW', W.BOOL, W.HWND, W.UINT, C.c_size_t, C.c_ssize_t)
dialog_item = native.api(native.user, 'GetDlgItem', W.HWND, W.HWND, C.c_int)
class_long = native.api(native.user, 'GetClassLongPtrW' if C.sizeof(C.c_void_p) == 8 else 'GetClassLongW',
                        C.c_size_t, W.HWND, C.c_int)
copy_icon = native.api(native.user, 'CopyIcon', W.HICON, W.HICON)
rect = native.api(native.user, 'GetWindowRect', W.BOOL, W.HWND, C.POINTER(W.RECT))
print_window = native.api(native.user, 'PrintWindow', W.BOOL, W.HWND, W.HDC, W.UINT)

def text(hwnd):
    buffer = C.create_unicode_buffer(1024)
    window_text(hwnd, buffer, len(buffer))
    return buffer.value

def children(hwnd):
    rows = []
    @window_callback
    def collect(child, context):
        rows.append(text(child))
        return True
    enum_children(hwnd, collect, 0)
    return rows

def capture(hwnd):
    bounds = W.RECT()
    native.check(rect(hwnd, C.byref(bounds)))
    width, height = bounds.right - bounds.left, bounds.bottom - bounds.top
    assert 100 < width < 2000 and 100 < height < 2000, 'unexpected wizard window bounds'
    dc = native.check(native.create_dc(None))
    bits = C.c_void_p()
    info = native.info(width, height)
    bitmap = native.check(native.create_dib(dc, C.byref(info), 0, C.byref(bits), None, 0))
    previous = native.select_object(dc, bitmap)
    try:
        native.check(print_window(hwnd, dc, 2) or print_window(hwnd, dc, 0))
        native.flush_gdi()
        image = native.Image.frombytes('RGBA', (width, height), C.string_at(bits, width * height * 4), 'raw', 'BGRA').convert('RGB')
        assert len(image.getcolors(width * height) or []) > 10, 'blank window capture'
        image.save(out / 'welcome-window.png')
    finally:
        native.select_object(dc, previous)
        native.delete_object(bitmap)
        native.delete_dc(dc)

processes = {}
root = None
error = None

def lifetime(handle):
    created, exited, kernel_time, user_time = W.FILETIME(), W.FILETIME(), W.FILETIME(), W.FILETIME()
    native.check(process_times(handle, C.byref(created), C.byref(exited), C.byref(kernel_time), C.byref(user_time)))
    def number(value):
        return (value.dwHighDateTime << 32) | value.dwLowDateTime
    return number(created), number(exited)

def refresh_tree():
    if root is None:
        return
    handle = snapshot(2, 0)
    if handle == C.c_void_p(-1).value:
        raise C.WinError(C.get_last_error())
    rows = []
    entry = ProcessEntry()
    entry.size = C.sizeof(entry)
    try:
        more = first(handle, C.byref(entry))
        while more:
            rows.append((entry.pid, entry.parent))
            more = next_process(handle, C.byref(entry))
    finally:
        close_handle(handle)
    # Hold stable process handles; compare creation/exit times before admitting
    # a descendant, including a child whose bootstrap parent already exited.
    changed = True
    while changed:
        changed = False
        for pid, parent in rows:
            if pid in processes or parent not in processes:
                continue
            handle = open_process(0x1000 | 0x100000 | 1, False, pid)
            if not handle:
                continue
            parent_start, parent_end = lifetime(processes[parent][0])
            child_start, _ = lifetime(handle)
            if child_start < parent_start or (parent_end and child_start > parent_end):
                close_handle(handle)
                continue
            buffer, size = C.create_unicode_buffer(32768), W.DWORD(32768)
            if not image_path(handle, 0, buffer, C.byref(size)) or scope not in Path(buffer.value).resolve().parents:
                close_handle(handle)
                raise RuntimeError('Refusing process outside contained C scope')
            processes[pid] = (handle, buffer.value)
            changed = True

def windows():
    found = []
    @window_callback
    def collect(hwnd, context):
        pid = W.DWORD()
        window_pid(hwnd, C.byref(pid))
        if pid.value in processes and wait(processes[pid.value][0], 0) == 258:
            found.append(hwnd)
        return True
    enum_windows(collect, 0)
    return found

try:
    environment = {**os.environ, 'TEMP': str(out / 'tmp'), 'TMP': str(out / 'tmp')}
    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 4  # SW_SHOWNOACTIVATE: contained fixture, no focus grab.
    root = subprocess.Popen([str(setup)], cwd=setup.parent, env=environment, startupinfo=startup)
    processes[root.pid] = (native.check(open_process(0x1000 | 0x100000 | 1, False, root.pid)), str(setup))
    deadline, welcome = time.monotonic() + 20, None
    while time.monotonic() < deadline:
        refresh_tree()
        for hwnd in windows():
            labels = children(hwnd)
            if visible(hwnd) and 'Hubchat Icon Test' in text(hwnd) and any('welcome' in label.lower() for label in labels):
                welcome = hwnd
                result.update({'window_title': text(hwnd), 'controls': labels})
                break
        if welcome:
            break
        time.sleep(0.1)
    assert welcome, 'disposable Welcome page did not appear'
    time.sleep(0.2)
    capture(welcome)
    icon_result = C.c_size_t()
    native.check(send(welcome, 0x7F, 2, 0, 2, 1000, C.byref(icon_result)))  # WM_GETICON, ICON_SMALL2
    icon_handle = icon_result.value or class_long(welcome, -34)  # GCLP_HICONSM
    source = 'WM_GETICON/ICON_SMALL2' if icon_result.value else 'GCLP_HICONSM'
    assert icon_handle, 'Welcome window has no small icon handle'
    owned_icon = native.check(copy_icon(icon_handle))
    reference = None
    try:
        ico = Path(__file__).resolve().parents[1] / 'src-tauri/icons/icon.ico'
        reference = native.check(native.load_image(None, C.cast(C.c_wchar_p(str(ico)), C.c_void_p), 1, 16, 16, 0x10))
        actual, expected = native.render(owned_icon, 16), native.render(reference, 16)
        actual.save(out / 'welcome-small-icon.png')
        expected.save(out / 'reference-small-icon.png')
        result['icons'].append({'source': source, 'size': 16, 'matches_app_icon': actual.tobytes() == expected.tobytes(),
                                'actual_sha256': hashlib.sha256(actual.tobytes()).hexdigest(),
                                'reference_sha256': hashlib.sha256(expected.tobytes()).hexdigest()})
        assert result['icons'][0]['matches_app_icon'], 'Welcome title-bar icon differs from the existing app ICO'
    finally:
        native.destroy_icon(owned_icon)
        if reference:
            native.destroy_icon(reference)
except BaseException as failure:
    error = failure
    result['error'] = repr(failure)
finally:
    try:
        refresh_tree()
        deadline = time.monotonic() + 5
        while any(wait(h, 0) == 258 for h, _ in processes.values()) and time.monotonic() < deadline:
            for hwnd in windows():
                yes = dialog_item(hwnd, 6)
                if yes and text(yes).replace('&', '').lower() == 'yes':
                    post(hwnd, 0x111, 6, 0)  # Quit confirmation only; never Next/Install.
                else:
                    post(hwnd, 0x10, 0, 0)  # WM_CLOSE
            time.sleep(0.1)
            refresh_tree()
        for pid, (handle, executable) in reversed(list(processes.items())):
            if wait(handle, 0) == 258:
                result['forced_cleanup'].append({'pid': pid, 'exe': executable, 'reason': 'contained Welcome did not exit after WM_CLOSE/Yes within5s'})
                # Persist the forced-close record before touching the process.
                (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
                native.check(terminate(handle, 1))
                assert wait(handle, 5000) == 0, 'contained process survived cleanup'
        if root:
            root.wait(timeout=5)
        result['remaining_pids'] = [pid for pid, (handle, _) in processes.items() if wait(handle, 0) == 258]
        result['processes'] = [{'pid': pid, 'exe': executable} for pid, (_, executable) in processes.items()]
        result['after'] = install_state()
        assert result['after'] == before, 'disposable installation/key state changed'
        assert not result['remaining_pids'], 'contained process remains'
    finally:
        for handle, _ in processes.values():
            close_handle(handle)
        (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(result, indent=2))
if error:
    raise error
