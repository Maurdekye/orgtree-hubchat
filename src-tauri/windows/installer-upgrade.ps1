# Closes the running Hubchat that holds ONE exact installed executable before the
# installer replaces it (src-tauri/windows/installer.nsi, the Upgrade page).
#
# Graceful first, force last:
#   1. -QuitControl (the installed Hubchat understands `--quit-for-upgrade`):
#      start the executable with that argument. Hubchat's single-instance plugin
#      hands it to the running Hubchat, which quits through the same path as the
#      tray's "Quit Hubchat"; wait up to -GracefulSeconds.
#   2. Ask every window of that process to close (WM_CLOSE). A Hubchat that quits
#      on close does; one that hides in the tray does not. Wait -WindowCloseSeconds.
#   3. Only then terminate the process, say so (the first word of the output is
#      FORCED) and wait -ForceSeconds for it to be gone.
# Only processes whose executable path equals -ExecutablePath are touched, so a
# different Hubchat (e.g. "Hubchat Test") or another copy is never closed.
#
# Exit 0: nothing of Hubchat's is running any more (output FORCED... if it took a
# terminate). Exit 1: something still runs. Exit 2: the helper itself failed.
param(
  [Parameter(Mandatory = $true)] [ValidateNotNullOrEmpty()] [string] $ExecutablePath,
  [int] $GracefulSeconds = 2,
  [int] $WindowCloseSeconds = 6,
  [int] $ForceSeconds = 10,
  [switch] $QuitControl,
  [string] $LogPath = ''
)
$ErrorActionPreference = 'Stop'

function Write-Log([string] $Message) {
  if (-not $LogPath) { return }
  try {
    # Bounded: a log past 256 KiB starts again rather than growing forever.
    if ((Test-Path -LiteralPath $LogPath) -and ((Get-Item -LiteralPath $LogPath).Length -gt 262144)) {
      Remove-Item -LiteralPath $LogPath -Force
    }
    Add-Content -LiteralPath $LogPath -Value ('{0:yyyy-MM-dd HH:mm:ss.fff} {1}' -f (Get-Date), $Message)
  } catch { }
}

function Get-Targets {
  $full = [IO.Path]::GetFullPath($ExecutablePath)
  $name = [IO.Path]::GetFileName($full).Replace("'", "''")
  @(Get-CimInstance Win32_Process -Filter ("Name = '{0}'" -f $name) |
    Where-Object {
      $_.ExecutablePath -and
      [string]::Equals([IO.Path]::GetFullPath($_.ExecutablePath), $full, [StringComparison]::OrdinalIgnoreCase)
    })
}

function Wait-Gone([int] $Seconds) {
  $deadline = (Get-Date).AddSeconds($Seconds)
  while ((Get-Date) -lt $deadline) {
    if ((Get-Targets).Count -eq 0) { return $true }
    Start-Sleep -Milliseconds 250
  }
  return ((Get-Targets).Count -eq 0)
}

Add-Type -ErrorAction SilentlyContinue -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class HubchatWindows {
  delegate bool EnumProc(IntPtr hwnd, IntPtr lParam);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  // Posts WM_CLOSE to every top-level window (visible or hidden) of the given processes.
  public static int Close(int[] pids) {
    var set = new HashSet<int>(pids);
    var hits = new List<IntPtr>();
    EnumWindows(delegate (IntPtr h, IntPtr l) {
      uint pid; GetWindowThreadProcessId(h, out pid);
      if (set.Contains((int)pid)) hits.Add(h);
      return true;
    }, IntPtr.Zero);
    foreach (var h in hits) PostMessage(h, 0x0010, IntPtr.Zero, IntPtr.Zero);
    return hits.Count;
  }
}
'@

try {
  $closeTimer = [Diagnostics.Stopwatch]::StartNew()
  $full = [IO.Path]::GetFullPath($ExecutablePath)
  Write-Log "start: executable=[$full] quitControl=$($QuitControl.IsPresent)"
  $targets = Get-Targets
  if ($targets.Count -eq 0) {
    Write-Log 'not running'
    Write-Output 'Hubchat is not running.'
    exit 0
  }
  Write-Log ('running: pid ' + (($targets | ForEach-Object { $_.ProcessId }) -join ','))

  if ($QuitControl) {
    Write-Log 'asking the running Hubchat to quit (--quit-for-upgrade)'
    Start-Process -FilePath $full -ArgumentList '--quit-for-upgrade' | Out-Null
    if (Wait-Gone $GracefulSeconds) {
      Write-Log "closed gracefully after the quit request elapsedMs=$($closeTimer.ElapsedMilliseconds)"
      Write-Output 'Hubchat quit gracefully.'
      exit 0
    }
    Write-Log "no graceful quit within $GracefulSeconds s"
  } else {
    Write-Log 'this installation predates the quit request; asking its windows to close'
  }

  $pids = @((Get-Targets) | ForEach-Object { [int]$_.ProcessId })
  if ($pids.Count -gt 0) {
    $n = [HubchatWindows]::Close($pids)
    Write-Log "posted WM_CLOSE to $n window(s)"
    if (Wait-Gone $WindowCloseSeconds) {
      Write-Log "closed after WM_CLOSE elapsedMs=$($closeTimer.ElapsedMilliseconds)"
      Write-Output 'Hubchat closed.'
      exit 0
    }
  }

  # Last resort. The failure of the graceful close is reported, not hidden.
  $left = @(Get-Targets)
  $ids = ($left | ForEach-Object { $_.ProcessId }) -join ','
  Write-Log "graceful close failed; terminating pid $ids"
  foreach ($p in $left) {
    try { Stop-Process -Id ([int]$p.ProcessId) -Force -ErrorAction Stop } catch { Write-Log "terminate pid $($p.ProcessId) failed: $($_.Exception.Message)" }
  }
  if (Wait-Gone $ForceSeconds) {
    Write-Log 'gone after terminate'
    Write-Output "FORCED: Hubchat did not close gracefully, so it was terminated (pid $ids)."
    exit 0
  }
  Write-Log 'still running after terminate'
  Write-Output "Hubchat is still running (pid $ids) and could not be terminated."
  exit 1
} catch {
  Write-Log ('helper failed: ' + $_.Exception.Message)
  Write-Output ('The close helper failed: ' + $_.Exception.Message)
  exit 2
}
