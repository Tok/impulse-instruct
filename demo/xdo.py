#!/usr/bin/env python3
"""xdo.py — xdotool replacement using libxdo3 (already installed).

Usage as CLI:
    python3 xdo.py type "hello world"
    python3 xdo.py key Return
    python3 xdo.py mousemove --window 0x1234 100 200
    python3 xdo.py click 1
    python3 xdo.py search --name "Impulse"
    python3 xdo.py getactivewindow
    python3 xdo.py windowfocus 0x1234
    python3 xdo.py windowsize 0x1234 1920 1080
    python3 xdo.py windowmove 0x1234 0 0
    python3 xdo.py scroll_down 3
    python3 xdo.py scroll_up 3

Usage as library:
    from xdo import Xdo
    x = Xdo()
    wid = x.search_window("Impulse")
    x.type_text("hello", window=wid, delay_us=50000)
    x.key("Return", window=wid)
    x.mouse_move(100, 200, window=wid)
    x.click(1, window=wid)
"""

import ctypes
import ctypes.util
import sys
import time

# ─── libxdo bindings ──────────────────────────────────────────────────────────

_lib = ctypes.CDLL("libxdo.so.3")

# Opaque handle
class _XdoHandle(ctypes.Structure):
    pass

_lib.xdo_new.restype = ctypes.POINTER(_XdoHandle)
_lib.xdo_new.argtypes = [ctypes.c_char_p]

_lib.xdo_free.argtypes = [ctypes.POINTER(_XdoHandle)]

_lib.xdo_enter_text_window.argtypes = [
    ctypes.POINTER(_XdoHandle), ctypes.c_ulong, ctypes.c_char_p, ctypes.c_ulong
]
_lib.xdo_enter_text_window.restype = ctypes.c_int

_lib.xdo_send_keysequence_window.argtypes = [
    ctypes.POINTER(_XdoHandle), ctypes.c_ulong, ctypes.c_char_p, ctypes.c_ulong
]
_lib.xdo_send_keysequence_window.restype = ctypes.c_int

_lib.xdo_move_mouse.argtypes = [
    ctypes.POINTER(_XdoHandle), ctypes.c_int, ctypes.c_int, ctypes.c_int
]
_lib.xdo_move_mouse.restype = ctypes.c_int

_lib.xdo_move_mouse_relative_to_window.argtypes = [
    ctypes.POINTER(_XdoHandle), ctypes.c_ulong, ctypes.c_int, ctypes.c_int
]
_lib.xdo_move_mouse_relative_to_window.restype = ctypes.c_int

_lib.xdo_click_window.argtypes = [
    ctypes.POINTER(_XdoHandle), ctypes.c_ulong, ctypes.c_int
]
_lib.xdo_click_window.restype = ctypes.c_int

_lib.xdo_mouse_down.argtypes = [
    ctypes.POINTER(_XdoHandle), ctypes.c_ulong, ctypes.c_int
]
_lib.xdo_mouse_down.restype = ctypes.c_int

_lib.xdo_mouse_up.argtypes = [
    ctypes.POINTER(_XdoHandle), ctypes.c_ulong, ctypes.c_int
]
_lib.xdo_mouse_up.restype = ctypes.c_int

_lib.xdo_focus_window.argtypes = [ctypes.POINTER(_XdoHandle), ctypes.c_ulong]
_lib.xdo_focus_window.restype = ctypes.c_int

_lib.xdo_get_active_window.argtypes = [
    ctypes.POINTER(_XdoHandle), ctypes.POINTER(ctypes.c_ulong)
]
_lib.xdo_get_active_window.restype = ctypes.c_int

_lib.xdo_set_window_size.argtypes = [
    ctypes.POINTER(_XdoHandle), ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.c_int
]
_lib.xdo_set_window_size.restype = ctypes.c_int

_lib.xdo_move_window.argtypes = [
    ctypes.POINTER(_XdoHandle), ctypes.c_ulong, ctypes.c_int, ctypes.c_int
]
_lib.xdo_move_window.restype = ctypes.c_int

# Search
class _XdoSearch(ctypes.Structure):
    _fields_ = [
        ("title", ctypes.c_char_p),
        ("winclass", ctypes.c_char_p),
        ("winclassname", ctypes.c_char_p),
        ("winname", ctypes.c_char_p),
        ("pid", ctypes.c_int),
        ("max_depth", ctypes.c_long),
        ("only_visible", ctypes.c_int),
        ("screen", ctypes.c_int),
        ("require", ctypes.c_int),  # enum: any=0, all=1
        ("searchmask", ctypes.c_uint),
        ("desktop", ctypes.c_long),
        ("limit", ctypes.c_uint),
    ]

SEARCH_NAME = (1 << 0)
SEARCH_CLASS = (1 << 1)
SEARCH_CLASSNAME = (1 << 2)
SEARCH_TITLE = (1 << 3)  # not always available
SEARCH_ONLYVISIBLE = (1 << 4)

_lib.xdo_search_windows.argtypes = [
    ctypes.POINTER(_XdoHandle),
    ctypes.POINTER(_XdoSearch),
    ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)),
    ctypes.POINTER(ctypes.c_uint),
]
_lib.xdo_search_windows.restype = ctypes.c_int

# ─── Python wrapper ──────────────────────────────────────────────────────────

class Xdo:
    def __init__(self, display=None):
        disp = display.encode() if display else None
        self._handle = _lib.xdo_new(disp)
        if not self._handle:
            raise RuntimeError("Failed to create xdo instance")

    def __del__(self):
        if hasattr(self, "_handle") and self._handle:
            _lib.xdo_free(self._handle)

    def type_text(self, text, window=0, delay_us=12000):
        """Type text character by character."""
        _lib.xdo_enter_text_window(
            self._handle, window, text.encode("utf-8"), delay_us
        )

    def key(self, keysequence, window=0, delay_us=12000):
        """Send key sequence (e.g. 'Return', 'ctrl+a', 'Tab')."""
        _lib.xdo_send_keysequence_window(
            self._handle, window, keysequence.encode("utf-8"), delay_us
        )

    def mouse_move(self, x, y, window=None, screen=0):
        """Move mouse to absolute or window-relative position."""
        if window:
            _lib.xdo_move_mouse_relative_to_window(self._handle, window, x, y)
        else:
            _lib.xdo_move_mouse(self._handle, x, y, screen)

    def click(self, button=1, window=0):
        """Click mouse button (1=left, 2=middle, 3=right)."""
        _lib.xdo_click_window(self._handle, window, button)

    def mouse_down(self, button=1, window=0):
        _lib.xdo_mouse_down(self._handle, window, button)

    def mouse_up(self, button=1, window=0):
        _lib.xdo_mouse_up(self._handle, window, button)

    def scroll(self, direction="down", clicks=3, window=0):
        """Scroll up (button 4) or down (button 5)."""
        btn = 5 if direction == "down" else 4
        for _ in range(clicks):
            _lib.xdo_click_window(self._handle, window, btn)
            time.sleep(0.05)

    def focus_window(self, window):
        _lib.xdo_focus_window(self._handle, window)

    def get_active_window(self):
        wid = ctypes.c_ulong()
        _lib.xdo_get_active_window(self._handle, ctypes.byref(wid))
        return wid.value

    def set_window_size(self, window, width, height):
        _lib.xdo_set_window_size(self._handle, window, width, height, 0)

    def move_window(self, window, x, y):
        _lib.xdo_move_window(self._handle, window, x, y)

    def search_window(self, name):
        """Find window by name substring. Returns first match or 0."""
        search = _XdoSearch()
        ctypes.memset(ctypes.byref(search), 0, ctypes.sizeof(search))
        search.winname = name.encode("utf-8")
        search.searchmask = SEARCH_NAME | SEARCH_ONLYVISIBLE
        search.only_visible = 1
        search.max_depth = -1
        search.limit = 10

        windows = ctypes.POINTER(ctypes.c_ulong)()
        nwindows = ctypes.c_uint()
        _lib.xdo_search_windows(
            self._handle, ctypes.byref(search),
            ctypes.byref(windows), ctypes.byref(nwindows)
        )
        if nwindows.value > 0:
            result = windows[0]
            # Free the allocated array
            ctypes.CDLL("libc.so.6").free(windows)
            return result
        return 0

    def search_all_windows(self, name):
        """Find all windows matching name. Returns list of window IDs."""
        search = _XdoSearch()
        ctypes.memset(ctypes.byref(search), 0, ctypes.sizeof(search))
        search.winname = name.encode("utf-8")
        search.searchmask = SEARCH_NAME | SEARCH_ONLYVISIBLE
        search.only_visible = 1
        search.max_depth = -1
        search.limit = 50

        windows = ctypes.POINTER(ctypes.c_ulong)()
        nwindows = ctypes.c_uint()
        _lib.xdo_search_windows(
            self._handle, ctypes.byref(search),
            ctypes.byref(windows), ctypes.byref(nwindows)
        )
        results = [windows[i] for i in range(nwindows.value)]
        if nwindows.value > 0:
            ctypes.CDLL("libc.so.6").free(windows)
        return results


# ─── CLI interface ────────────────────────────────────────────────────────────

def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)

    x = Xdo()
    cmd = sys.argv[1]

    if cmd == "type":
        text = sys.argv[2]
        delay = int(sys.argv[3]) if len(sys.argv) > 3 else 12000
        x.type_text(text, delay_us=delay)

    elif cmd == "key":
        x.key(sys.argv[2])

    elif cmd == "mousemove":
        args = sys.argv[2:]
        window = 0
        if "--window" in args:
            idx = args.index("--window")
            window = int(args[idx + 1], 0)
            args = args[:idx] + args[idx+2:]
        mx, my = int(args[0]), int(args[1])
        if window:
            x.mouse_move(mx, my, window=window)
        else:
            x.mouse_move(mx, my)

    elif cmd == "click":
        btn = int(sys.argv[2]) if len(sys.argv) > 2 else 1
        x.click(btn)

    elif cmd == "search":
        name = sys.argv[3] if "--name" in sys.argv else sys.argv[2]
        wid = x.search_window(name)
        if wid:
            print(hex(wid))
        else:
            sys.exit(1)

    elif cmd == "getactivewindow":
        print(hex(x.get_active_window()))

    elif cmd == "windowfocus":
        x.focus_window(int(sys.argv[2], 0))

    elif cmd == "windowsize":
        x.set_window_size(int(sys.argv[2], 0), int(sys.argv[3]), int(sys.argv[4]))

    elif cmd == "windowmove":
        x.move_window(int(sys.argv[2], 0), int(sys.argv[3]), int(sys.argv[4]))

    elif cmd == "scroll_down":
        n = int(sys.argv[2]) if len(sys.argv) > 2 else 3
        x.scroll("down", n)

    elif cmd == "scroll_up":
        n = int(sys.argv[2]) if len(sys.argv) > 2 else 3
        x.scroll("up", n)

    else:
        print(f"Unknown command: {cmd}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
