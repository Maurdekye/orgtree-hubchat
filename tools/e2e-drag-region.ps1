# Windows' side of the title bar checks, for tools/e2e-drag-region.mjs, at
# points of a Hubchat test build's page. ASCII only.
#   e2e-drag-region.ps1 -TargetPid <pid> -Points <points.json> -Out <answers.json> [-Mode hittest|clicks]
# points.json: { "dpr": 1, "points": [ { "name": "...", "x": <css px>, "y": <css px>, "act": "right double" } ] }
# (page coordinates, from the top-left corner of the page's viewport)
#
# hittest (read-only): Windows' hit test (WM_NCHITTEST) at each point. Only
# windows that contain the point are asked (their rectangle, their window
# region if any, WS_VISIBLE), as Windows does: WebView2 covers a page's drag
# region with child windows of its own that answer HTCAPTION wherever they are
# asked (rig round 447). "hit" is the window Windows would ask: from the main
# window down, each time the topmost child that contains the point.
#
# clicks: what real mouse input would post, without a mouse (agents run in a
# non-interactive window station: no cursor, no SendInput, rig round 446):
#   - control: WM_SYSCOMMAND SC_KEYMENU ' ' to the main window (Alt+Space),
#     which opens the system menu: it shows that this rig can see one;
#   - at each point with act "right": a right-click on the window under the
#     point, as Windows would deliver it (WM_NCRBUTTONDOWN/UP with the hit
#     code, or the release to the window that took capture), then 1.5 s of
#     watching for a menu (GetGUIThreadInfo menu flags, visible #32768 windows
#     of the UI thread);
#   - a menu that opens is identified: GUI_SYSTEMMENUMODE, or its items
#     (MN_GETHMENU, GetMenuItemID) hold SC_MOVE and SC_CLOSE, as the window's
#     system menu does. Then it is closed again (WM_CANCELMODE to its owner,
#     then Escape);
#   - at the point with act "double": WM_NCLBUTTONDBLCLK twice, watching the
#     main window maximise and then restore.
param([Parameter(Mandatory)][int]$TargetPid, [Parameter(Mandatory)][string]$Points, [Parameter(Mandatory)][string]$Out,
  [ValidateSet('hittest', 'clicks')][string]$Mode = 'hittest')
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public static class DragRegion {
    delegate bool EnumProc(IntPtr h, IntPtr l);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int left, top, right, bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int x, y; }
    [StructLayout(LayoutKind.Sequential)] public struct GUITHREADINFO {
        public int cbSize; public uint flags;
        public IntPtr hwndActive, hwndFocus, hwndCapture, hwndMenuOwner, hwndMoveSize, hwndCaret;
        public RECT rcCaret;
    }
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumProc cb, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumThreadWindows(uint tid, EnumProc cb, IntPtr l);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] static extern IntPtr GetParent(IntPtr h);
    [DllImport("user32.dll")] static extern IntPtr SendMessageTimeoutW(IntPtr h, uint msg, IntPtr w, IntPtr l, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll")] static extern bool PostMessageW(IntPtr h, uint msg, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] static extern IntPtr GetWindow(IntPtr h, uint cmd);
    [DllImport("user32.dll")] static extern int GetWindowLongW(IntPtr h, int index);
    [DllImport("user32.dll")] static extern int GetWindowRgn(IntPtr h, IntPtr rgn);
    [DllImport("user32.dll")] static extern bool GetGUIThreadInfo(uint tid, ref GUITHREADINFO info);
    [DllImport("user32.dll")] static extern bool ScreenToClient(IntPtr h, ref POINT p);
    [DllImport("user32.dll")] static extern bool IsZoomed(IntPtr h);
    [DllImport("user32.dll")] static extern int GetMenuItemCount(IntPtr menu);
    [DllImport("user32.dll")] static extern uint GetMenuItemID(IntPtr menu, int pos);
    [DllImport("gdi32.dll")] static extern IntPtr CreateRectRgn(int left, int top, int right, int bottom);
    [DllImport("gdi32.dll")] static extern bool PtInRegion(IntPtr rgn, int x, int y);
    [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr o);

    const uint WM_NCHITTEST = 0x0084, WM_CANCELMODE = 0x001F, WM_KEYDOWN = 0x0100, WM_KEYUP = 0x0101;
    const uint WM_SYSCOMMAND = 0x0112, SC_KEYMENU = 0xF100, SC_MOVE = 0xF010, SC_CLOSE = 0xF060, MN_GETHMENU = 0x01E1;
    const uint WM_NCLBUTTONDBLCLK = 0x00A3, WM_NCRBUTTONDOWN = 0x00A4, WM_NCRBUTTONUP = 0x00A5;
    const uint WM_LBUTTONDBLCLK = 0x0203, WM_RBUTTONDOWN = 0x0204, WM_RBUTTONUP = 0x0205;
    const int HTCLIENT = 1, MK_LBUTTON = 1, MK_RBUTTON = 2, VK_ESCAPE = 0x1B;
    // GUI_INMENUMODE | GUI_SYSTEMMENUMODE | GUI_POPUPMENUMODE
    const uint MENU_FLAGS = 0x4 | 0x8 | 0x10;

    // A window takes a point if it has WS_VISIBLE (IsWindowVisible is false for
    // every window in a non-interactive window station), its rectangle holds
    // the point, and so does its window region, if it has one.
    static bool Takes(IntPtr h, int sx, int sy) {
        if ((GetWindowLongW(h, -16) & 0x10000000) == 0) return false;
        RECT r; GetWindowRect(h, out r);
        if (sx < r.left || sx >= r.right || sy < r.top || sy >= r.bottom) return false;
        IntPtr rgn = CreateRectRgn(0, 0, 0, 0);
        int kind = GetWindowRgn(h, rgn);
        bool inside = kind == 0 || PtInRegion(rgn, sx - r.left, sy - r.top);
        DeleteObject(rgn);
        return inside;
    }
    // The window Windows asks: from the top, each time the topmost child that takes the point.
    static IntPtr Hit(IntPtr top, int sx, int sy) {
        IntPtr cur = top;
        while (true) {
            IntPtr found = IntPtr.Zero;
            for (IntPtr c = GetWindow(cur, 5); c != IntPtr.Zero; c = GetWindow(c, 2)) { if (Takes(c, sx, sy)) { found = c; break; } }
            if (found == IntPtr.Zero) return cur;
            cur = found;
        }
    }
    static string Ask(IntPtr h, IntPtr lp) {
        IntPtr res; IntPtr ok = SendMessageTimeoutW(h, WM_NCHITTEST, IntPtr.Zero, lp, 0x0002, 1000, out res);
        return ok == IntPtr.Zero ? "null" : ((int)(long)res).ToString();
    }
    static IntPtr LParam(int x, int y) { return (IntPtr)(int)(((y & 0xFFFF) << 16) | (x & 0xFFFF)); }

    static string Cls(IntPtr h) { var s = new StringBuilder(128); GetClassNameW(h, s, 128); return s.ToString(); }
    static string Title(IntPtr h) { var s = new StringBuilder(128); GetWindowTextW(h, s, 128); return s.ToString(); }
    static string J(string s) { return "\"" + (s ?? "").Replace("\\", "\\\\").Replace("\"", "\\\"") + "\""; }
    static string H(IntPtr h) { return J("0x" + h.ToInt64().ToString("X")); }
    static string R(RECT r) { return "[" + r.left + "," + r.top + "," + r.right + "," + r.bottom + "]"; }
    static string B(bool b) { return b ? "true" : "false"; }

    static IntPtr FindMain(uint pid) {
        IntPtr main = IntPtr.Zero;
        EnumWindows((h, l) => {
            uint p; GetWindowThreadProcessId(h, out p);
            if (p == pid && Cls(h) == "Tauri Window" && !Title(h).Contains("tray menu")) { main = h; return false; }
            return true;
        }, IntPtr.Zero);
        return main;
    }
    static List<IntPtr> Tree(IntPtr main) {
        var wins = new List<IntPtr> { main };
        EnumChildWindows(main, (h, l) => { wins.Add(h); return true; }, IntPtr.Zero);
        return wins;
    }
    // the page's viewport: WebView2's render widget under the main window
    static IntPtr Viewport(List<IntPtr> wins) { return wins.Find(h => Cls(h) == "Chrome_RenderWidgetHostHWND"); }

    public static string Run(uint pid, string[] names, double[] xs, double[] ys, double dpr) {
        IntPtr main = FindMain(pid);
        if (main == IntPtr.Zero) return "{\"error\":\"no main window\"}";
        var wins = Tree(main);
        IntPtr view = Viewport(wins);
        var sb = new StringBuilder("{\"main\":" + H(main) + ",\"windows\":[");
        for (int i = 0; i < wins.Count; i++) {
            uint p; uint t = GetWindowThreadProcessId(wins[i], out p); RECT r; GetWindowRect(wins[i], out r);
            IntPtr rgn = CreateRectRgn(0, 0, 0, 0); int kind = GetWindowRgn(wins[i], rgn); DeleteObject(rgn);
            bool vis = (GetWindowLongW(wins[i], -16) & 0x10000000) != 0;
            sb.Append((i > 0 ? "," : "") + "{\"hwnd\":" + H(wins[i]) + ",\"cls\":" + J(Cls(wins[i])) + ",\"pid\":" + p + ",\"tid\":" + t + ",\"parent\":" + H(GetParent(wins[i])) + ",\"rect\":" + R(r)
                + ",\"visibleStyle\":" + B(vis) + ",\"region\":" + kind + "}");
        }
        sb.Append("]");
        if (view == IntPtr.Zero) return sb.Append(",\"error\":\"no page viewport window\"}").ToString();
        RECT vr; GetWindowRect(view, out vr);
        sb.Append(",\"viewport\":{\"hwnd\":" + H(view) + ",\"rect\":" + R(vr) + "},\"points\":[");
        for (int i = 0; i < names.Length; i++) {
            int sx = vr.left + (int)Math.Round(xs[i] * dpr), sy = vr.top + (int)Math.Round(ys[i] * dpr);
            IntPtr lp = LParam(sx, sy);
            IntPtr hit = Hit(main, sx, sy);
            sb.Append((i > 0 ? "," : "") + "{\"name\":" + J(names[i]) + ",\"screen\":[" + sx + "," + sy + "],\"hit\":{\"hwnd\":" + H(hit) + ",\"cls\":" + J(Cls(hit)) + ",\"ht\":" + Ask(hit, lp) + "},\"answers\":[");
            bool first = true;
            for (int k = 0; k < wins.Count; k++) {
                if (!Takes(wins[k], sx, sy)) continue;
                sb.Append((first ? "" : ",") + "{\"hwnd\":" + H(wins[k]) + ",\"cls\":" + J(Cls(wins[k])) + ",\"ht\":" + Ask(wins[k], lp) + "}");
                first = false;
            }
            sb.Append("]}");
        }
        return sb.Append("]}").ToString();
    }

    // ---------------------------------------------------------------- clicks

    static GUITHREADINFO Info(uint tid) {
        var gi = new GUITHREADINFO(); gi.cbSize = Marshal.SizeOf(typeof(GUITHREADINFO));
        if (!GetGUIThreadInfo(tid, ref gi)) gi.flags = 0xFFFFFFFF;
        return gi;
    }
    static List<IntPtr> Menus(uint tid) {
        var menus = new List<IntPtr>();
        EnumThreadWindows(tid, (h, l) => { if (Cls(h) == "#32768" && (GetWindowLongW(h, -16) & 0x10000000) != 0) menus.Add(h); return true; }, IntPtr.Zero);
        return menus;
    }
    // A menu is open: the UI thread is in menu mode, or shows a menu window.
    static bool Open(uint tid) {
        var gi = Info(tid);
        return (gi.flags != 0xFFFFFFFF && (gi.flags & MENU_FLAGS) != 0) || Menus(tid).Count > 0;
    }
    // The thread's menu state, and each open menu with its items. "system": the
    // window's system menu (GUI_SYSTEMMENUMODE, or SC_MOVE and SC_CLOSE among the items).
    static string State(uint tid) {
        var gi = Info(tid);
        bool sys = gi.flags != 0xFFFFFFFF && (gi.flags & 0x8) != 0;
        var sb = new StringBuilder("{\"flags\":" + (gi.flags == 0xFFFFFFFF ? "null" : gi.flags.ToString()) + ",\"menuOwner\":" + H(gi.hwndMenuOwner)
            + ",\"ownerCls\":" + J(gi.hwndMenuOwner == IntPtr.Zero ? "" : Cls(gi.hwndMenuOwner)) + ",\"capture\":" + H(gi.hwndCapture) + ",\"menus\":[");
        var menus = Menus(tid);
        for (int i = 0; i < menus.Count; i++) {
            RECT mr; GetWindowRect(menus[i], out mr);
            IntPtr hm; IntPtr ok = SendMessageTimeoutW(menus[i], MN_GETHMENU, IntPtr.Zero, IntPtr.Zero, 0x0002, 500, out hm);
            var items = new StringBuilder("[");
            bool move = false, close = false;
            int n = ok != IntPtr.Zero && hm != IntPtr.Zero ? GetMenuItemCount(hm) : -1;
            for (int k = 0; k < n && k < 32; k++) {
                uint id = GetMenuItemID(hm, k);
                items.Append((k > 0 ? "," : "") + J("0x" + id.ToString("X")));
                if (id == SC_MOVE) move = true;
                if (id == SC_CLOSE) close = true;
            }
            if (move && close) sys = true;
            sb.Append((i > 0 ? "," : "") + "{\"hwnd\":" + H(menus[i]) + ",\"rect\":" + R(mr) + ",\"hmenu\":" + H(hm) + ",\"count\":" + n + ",\"items\":" + items.Append("]") + "}");
        }
        return sb.Append("],\"system\":" + B(sys) + "}").ToString();
    }
    // Watches up to ms for a menu; the time it took, or -1.
    static int WaitOpen(uint tid, int ms) {
        var t = DateTime.UtcNow;
        while (true) {
            int el = (int)(DateTime.UtcNow - t).TotalMilliseconds;
            if (Open(tid)) return el;
            if (el >= ms) return -1;
            Thread.Sleep(50);
        }
    }
    // Closes an open menu: WM_CANCELMODE to its owner, then Escape to the menu window.
    static bool Dismiss(uint tid, IntPtr main) {
        for (int round = 0; round < 3 && Open(tid); round++) {
            var gi = Info(tid);
            IntPtr owner = gi.hwndMenuOwner != IntPtr.Zero ? gi.hwndMenuOwner : main;
            IntPtr res; SendMessageTimeoutW(owner, WM_CANCELMODE, IntPtr.Zero, IntPtr.Zero, 0x0002, 1000, out res);
            Thread.Sleep(300);
            if (!Open(tid)) break;
            foreach (var m in Menus(tid)) {
                PostMessageW(m, WM_KEYDOWN, (IntPtr)VK_ESCAPE, (IntPtr)1);
                PostMessageW(m, WM_KEYUP, (IntPtr)VK_ESCAPE, (IntPtr)unchecked((int)0xC0000001));
            }
            Thread.Sleep(300);
        }
        return !Open(tid);
    }

    public static string Clicks(uint pid, string[] names, double[] xs, double[] ys, string[] acts, double dpr) {
        IntPtr main = FindMain(pid);
        if (main == IntPtr.Zero) return "{\"error\":\"no main window\"}";
        uint p; uint tid = GetWindowThreadProcessId(main, out p);
        IntPtr view = Viewport(Tree(main));
        if (view == IntPtr.Zero) return "{\"error\":\"no page viewport window\"}";
        RECT vr; GetWindowRect(view, out vr);
        var sb = new StringBuilder("{\"main\":" + H(main) + ",\"tid\":" + tid + ",\"style\":" + J("0x" + GetWindowLongW(main, -16).ToString("X8")));

        // control: Alt+Space opens the system menu
        string before = State(tid);
        PostMessageW(main, WM_SYSCOMMAND, (IntPtr)(int)SC_KEYMENU, (IntPtr)0x20);
        int ms = WaitOpen(tid, 1500);
        string seen = State(tid);
        bool dismissed = ms < 0 || Dismiss(tid, main);
        sb.Append(",\"control\":{\"before\":" + before + ",\"opened\":" + B(ms >= 0) + ",\"ms\":" + ms + ",\"seen\":" + seen + ",\"dismissed\":" + B(dismissed) + ",\"after\":" + State(tid) + "}");
        Thread.Sleep(300);

        sb.Append(",\"right\":[");
        bool firstRight = true;
        for (int i = 0; i < names.Length; i++) {
            if (!acts[i].Contains("right")) continue;
            int sx = vr.left + (int)Math.Round(xs[i] * dpr), sy = vr.top + (int)Math.Round(ys[i] * dpr);
            IntPtr lp = LParam(sx, sy);
            IntPtr hit = Hit(main, sx, sy);
            string hts = Ask(hit, lp);
            int ht = hts == "null" ? -999 : int.Parse(hts);
            bool wasOpen = Open(tid);
            string pre = State(tid);
            POINT c; c.x = sx; c.y = sy; ScreenToClient(hit, ref c);
            // the press, as Windows delivers it to the window under the point
            if (ht == HTCLIENT) PostMessageW(hit, WM_RBUTTONDOWN, (IntPtr)MK_RBUTTON, LParam(c.x, c.y));
            else PostMessageW(hit, WM_NCRBUTTONDOWN, (IntPtr)ht, lp);
            Thread.Sleep(150);
            // a menu may open on the press already
            bool openAfterPress = Open(tid);
            // the release: to a window that took capture on the press, as a client message
            IntPtr cap = Info(tid).hwndCapture;
            IntPtr upTo = hit; string upMsg;
            if (cap != IntPtr.Zero) {
                POINT cc; cc.x = sx; cc.y = sy; ScreenToClient(cap, ref cc);
                PostMessageW(cap, WM_RBUTTONUP, IntPtr.Zero, LParam(cc.x, cc.y)); upTo = cap; upMsg = "WM_RBUTTONUP";
            } else if (ht == HTCLIENT) {
                PostMessageW(hit, WM_RBUTTONUP, IntPtr.Zero, LParam(c.x, c.y)); upMsg = "WM_RBUTTONUP";
            } else {
                PostMessageW(hit, WM_NCRBUTTONUP, (IntPtr)ht, lp); upMsg = "WM_NCRBUTTONUP";
            }
            int rms = WaitOpen(tid, 1500);
            string rseen = State(tid);
            bool ropened = openAfterPress || rms >= 0;
            bool rdismissed = !ropened || Dismiss(tid, main);
            sb.Append((firstRight ? "" : ",") + "{\"name\":" + J(names[i]) + ",\"screen\":[" + sx + "," + sy + "],\"hit\":{\"hwnd\":" + H(hit) + ",\"cls\":" + J(Cls(hit)) + ",\"ht\":" + hts + "}"
                + ",\"menuBefore\":" + B(wasOpen) + ",\"before\":" + pre + ",\"press\":" + J(ht == HTCLIENT ? "WM_RBUTTONDOWN" : "WM_NCRBUTTONDOWN") + ",\"captureAfterPress\":" + H(cap)
                + ",\"release\":{\"msg\":" + J(upMsg) + ",\"to\":" + H(upTo) + ",\"cls\":" + J(Cls(upTo)) + "}"
                + ",\"openAfterPress\":" + B(openAfterPress) + ",\"opened\":" + B(ropened) + ",\"ms\":" + rms + ",\"seen\":" + rseen + ",\"dismissed\":" + B(rdismissed) + "}");
            firstRight = false;
            Thread.Sleep(200);
        }
        sb.Append("]");

        // a double-click on the caption maximises the window, a second one restores it
        for (int i = 0; i < names.Length; i++) {
            if (!acts[i].Contains("double")) continue;
            int sx = vr.left + (int)Math.Round(xs[i] * dpr), sy = vr.top + (int)Math.Round(ys[i] * dpr);
            RECT r0; GetWindowRect(main, out r0);
            bool z0 = IsZoomed(main);
            var hits = new StringBuilder("[");
            bool[] reached = new bool[2]; int[] took = new int[2];
            for (int k = 0; k < 2; k++) {
                IntPtr lp = LParam(sx, sy);
                IntPtr hit = Hit(main, sx, sy);
                string hts = Ask(hit, lp);
                int ht = hts == "null" ? -999 : int.Parse(hts);
                if (ht == HTCLIENT) { POINT c; c.x = sx; c.y = sy; ScreenToClient(hit, ref c); PostMessageW(hit, WM_LBUTTONDBLCLK, (IntPtr)MK_LBUTTON, LParam(c.x, c.y)); }
                else PostMessageW(hit, WM_NCLBUTTONDBLCLK, (IntPtr)ht, lp);
                bool want = k == 0;
                var t = DateTime.UtcNow; took[k] = -1;
                while ((DateTime.UtcNow - t).TotalMilliseconds < 2500) {
                    if (IsZoomed(main) == want) { took[k] = (int)(DateTime.UtcNow - t).TotalMilliseconds; break; }
                    Thread.Sleep(50);
                }
                reached[k] = took[k] >= 0;
                RECT rk; GetWindowRect(main, out rk);
                hits.Append((k > 0 ? "," : "") + "{\"hit\":{\"hwnd\":" + H(hit) + ",\"cls\":" + J(Cls(hit)) + ",\"ht\":" + hts + "},\"zoomed\":" + B(IsZoomed(main)) + ",\"ms\":" + took[k] + ",\"rect\":" + R(rk) + "}");
                if (!reached[k]) break;
                Thread.Sleep(700);
            }
            hits.Append("]");
            sb.Append(",\"double\":{\"name\":" + J(names[i]) + ",\"screen\":[" + sx + "," + sy + "],\"zoomedBefore\":" + B(z0) + ",\"rectBefore\":" + R(r0)
                + ",\"maximized\":" + B(reached[0]) + ",\"restored\":" + B(reached[0] && reached[1]) + ",\"clicks\":" + hits + "}");
            break;
        }
        return sb.Append("}").ToString();
    }
}
"@

$p = Get-Content -LiteralPath $Points -Raw | ConvertFrom-Json
$names = [string[]]@($p.points | ForEach-Object { $_.name })
$xs = [double[]]@($p.points | ForEach-Object { [double]$_.x })
$ys = [double[]]@($p.points | ForEach-Object { [double]$_.y })
$acts = [string[]]@($p.points | ForEach-Object { if ($_.act) { [string]$_.act } else { '' } })
if ($Mode -eq 'clicks') {
  $json = [DragRegion]::Clicks([uint32]$TargetPid, $names, $xs, $ys, $acts, [double]$p.dpr)
} else {
  $json = [DragRegion]::Run([uint32]$TargetPid, $names, $xs, $ys, [double]$p.dpr)
}
[System.IO.File]::WriteAllText($Out, $json, (New-Object System.Text.UTF8Encoding($false)))
