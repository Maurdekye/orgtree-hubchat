# Windows' hit test (WM_NCHITTEST) at points of a Hubchat test build's page,
# asked of the main window and every window under it (WebView2's included).
# For tools/e2e-drag-region.mjs. Read-only: WM_NCHITTEST only asks a window
# which part of it a point is on. ASCII only.
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
            sb.Append((i > 0 ? "," : "") + "{\"hwnd\":" + H(wins[i]) + ",\"cls\":" + J(Cls(wins[i])) + ",\"pid\":" + p + ",\"tid\":" + t + ",\"parent\":" + H(GetParent(wins[i])) + ",\"rect\":" + R(r) + "}");
        }
        sb.Append("]");
        if (view == IntPtr.Zero) return sb.Append(",\"error\":\"no page viewport window\"}").ToString();
        RECT vr; GetWindowRect(view, out vr);
        sb.Append(",\"viewport\":{\"hwnd\":" + H(view) + ",\"rect\":" + R(vr) + "},\"points\":[");
        for (int i = 0; i < names.Length; i++) {
            int sx = vr.left + (int)Math.Round(xs[i] * dpr), sy = vr.top + (int)Math.Round(ys[i] * dpr);
            IntPtr lp = (IntPtr)((sy << 16) | (sx & 0xFFFF));
            sb.Append((i > 0 ? "," : "") + "{\"name\":" + J(names[i]) + ",\"screen\":[" + sx + "," + sy + "],\"answers\":[");
            for (int k = 0; k < wins.Count; k++) {
                IntPtr res; IntPtr ok = SendMessageTimeoutW(wins[k], 0x0084, IntPtr.Zero, lp, 0x0002, 1000, out res);
                string ht = ok == IntPtr.Zero ? "null" : ((int)(long)res).ToString();
                sb.Append((k > 0 ? "," : "") + "{\"hwnd\":" + H(wins[k]) + ",\"cls\":" + J(Cls(wins[k])) + ",\"ht\":" + ht + "}");
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
