# Windows' hit test (WM_NCHITTEST) at points of a Hubchat test build's page.
# For tools/e2e-drag-region.mjs. Read-only: WM_NCHITTEST only asks a window
# which part of it a point is on. ASCII only.
# Only windows that contain the point are asked (their rectangle, their window
# region if any, WS_VISIBLE), as Windows does: WebView2 covers a page's drag
# region with child windows of its own that answer HTCAPTION wherever they are
# asked (rig round 447). "hit" is the window Windows would ask: from the main
# window down, each time the topmost child that contains the point.
#   e2e-drag-region.ps1 -TargetPid <pid> -Points <points.json> -Out <answers.json>
# points.json: { "dpr": 1, "points": [ { "name": "...", "x": <css px>, "y": <css px> } ] }
# (page coordinates, from the top-left corner of the page's viewport)
param([Parameter(Mandatory)][int]$TargetPid, [Parameter(Mandatory)][string]$Points, [Parameter(Mandatory)][string]$Out)
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class DragRegion {
    delegate bool EnumProc(IntPtr h, IntPtr l);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int left, top, right, bottom; }
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc cb, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumProc cb, IntPtr l);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] static extern IntPtr GetParent(IntPtr h);
    [DllImport("user32.dll")] static extern IntPtr SendMessageTimeoutW(IntPtr h, uint msg, IntPtr w, IntPtr l, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll")] static extern IntPtr GetWindow(IntPtr h, uint cmd);
    [DllImport("user32.dll")] static extern int GetWindowLongW(IntPtr h, int index);
    [DllImport("user32.dll")] static extern int GetWindowRgn(IntPtr h, IntPtr rgn);
    [DllImport("gdi32.dll")] static extern IntPtr CreateRectRgn(int left, int top, int right, int bottom);
    [DllImport("gdi32.dll")] static extern bool PtInRegion(IntPtr rgn, int x, int y);
    [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr o);

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
        IntPtr res; IntPtr ok = SendMessageTimeoutW(h, 0x0084, IntPtr.Zero, lp, 0x0002, 1000, out res);
        return ok == IntPtr.Zero ? "null" : ((int)(long)res).ToString();
    }

    static string Cls(IntPtr h) { var s = new StringBuilder(128); GetClassNameW(h, s, 128); return s.ToString(); }
    static string Title(IntPtr h) { var s = new StringBuilder(128); GetWindowTextW(h, s, 128); return s.ToString(); }
    static string J(string s) { return "\"" + (s ?? "").Replace("\\", "\\\\").Replace("\"", "\\\"") + "\""; }
    static string H(IntPtr h) { return J("0x" + h.ToInt64().ToString("X")); }
    static string R(RECT r) { return "[" + r.left + "," + r.top + "," + r.right + "," + r.bottom + "]"; }

    public static string Run(uint pid, string[] names, double[] xs, double[] ys, double dpr) {
        IntPtr main = IntPtr.Zero;
        EnumWindows((h, l) => {
            uint p; GetWindowThreadProcessId(h, out p);
            if (p == pid && Cls(h) == "Tauri Window" && !Title(h).Contains("tray menu")) { main = h; return false; }
            return true;
        }, IntPtr.Zero);
        if (main == IntPtr.Zero) return "{\"error\":\"no main window\"}";
        var wins = new List<IntPtr> { main };
        EnumChildWindows(main, (h, l) => { wins.Add(h); return true; }, IntPtr.Zero);
        // the page's viewport: WebView2's render widget under the main window
        IntPtr view = wins.Find(h => Cls(h) == "Chrome_RenderWidgetHostHWND");
        var sb = new StringBuilder("{\"main\":" + H(main) + ",\"windows\":[");
        for (int i = 0; i < wins.Count; i++) {
            uint p; uint t = GetWindowThreadProcessId(wins[i], out p); RECT r; GetWindowRect(wins[i], out r);
            IntPtr rgn = CreateRectRgn(0, 0, 0, 0); int kind = GetWindowRgn(wins[i], rgn); DeleteObject(rgn);
            bool vis = (GetWindowLongW(wins[i], -16) & 0x10000000) != 0;
            sb.Append((i > 0 ? "," : "") + "{\"hwnd\":" + H(wins[i]) + ",\"cls\":" + J(Cls(wins[i])) + ",\"pid\":" + p + ",\"tid\":" + t + ",\"parent\":" + H(GetParent(wins[i])) + ",\"rect\":" + R(r)
                + ",\"visibleStyle\":" + (vis ? "true" : "false") + ",\"region\":" + kind + "}");
        }
        sb.Append("]");
        if (view == IntPtr.Zero) return sb.Append(",\"error\":\"no page viewport window\"}").ToString();
        RECT vr; GetWindowRect(view, out vr);
        sb.Append(",\"viewport\":{\"hwnd\":" + H(view) + ",\"rect\":" + R(vr) + "},\"points\":[");
        for (int i = 0; i < names.Length; i++) {
            int sx = vr.left + (int)Math.Round(xs[i] * dpr), sy = vr.top + (int)Math.Round(ys[i] * dpr);
            IntPtr lp = (IntPtr)((sy << 16) | (sx & 0xFFFF));
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
}
"@

$p = Get-Content -LiteralPath $Points -Raw | ConvertFrom-Json
$names = [string[]]@($p.points | ForEach-Object { $_.name })
$xs = [double[]]@($p.points | ForEach-Object { [double]$_.x })
$ys = [double[]]@($p.points | ForEach-Object { [double]$_.y })
$json = [DragRegion]::Run([uint32]$TargetPid, $names, $xs, $ys, [double]$p.dpr)
[System.IO.File]::WriteAllText($Out, $json, (New-Object System.Text.UTF8Encoding($false)))
