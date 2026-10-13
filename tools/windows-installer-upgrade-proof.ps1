# Proves the Hubchat Windows installer's Upgrade against a DISPOSABLE product.
# Never run it against the real "Hubchat": the product name must differ and the
# script refuses the real name, the real install folder and the real app data.
#
#   powershell -File tools/windows-installer-upgrade-proof.ps1 `
#     -PriorSetup <setup.exe of the older version> -NewSetup <setup.exe of the newer one> `
#     -ProductName "Hubchat Upgrade Test" -Identifier dev.orgtree.hubchat.upgrade.test `
#     -Evidence <folder> [-Mode legacy|hide|stubborn|graceful] [-NoMarker] [-Cancel]
#
# Both setups must be built for that same disposable identity (see
# docs/windows-installer-upgrade.md). Steps: silent prior install (/S /NS), seed a
# canary in the app data folder, start the installed app, run the NEW setup with
# its window, click Upgrade on the Upgrade page, wait for it, then check:
# the old process is gone and how it was closed, the files and DisplayVersion are
# the new ones, the canary is untouched, exactly one Hubchat was relaunched from
# the install folder, and the real Hubchat was not touched. Finally uninstall
# the disposable product and delete what it created.
param(
  [Parameter(Mandatory)] [string] $PriorSetup,
  [Parameter(Mandatory)] [string] $NewSetup,
  [Parameter(Mandatory)] [string] $ProductName,
  [Parameter(Mandatory)] [string] $Identifier,
  [Parameter(Mandatory)] [string] $Evidence,
  [string] $Mode = '',          # stand-in apps only: standin-mode.txt content
  [switch] $NoMarker,           # delete HubchatQuitControl after the prior install (an old Hubchat)
  [switch] $Cancel,             # press Cancel on the "could not close" prompt instead of expecting success
  [switch] $AppNotRunning,      # do not start the installed app before upgrading (the upgrade must still relaunch it)
  [switch] $RequireGracefulQuit, # real app: prompt primary exit AND rung-1 helper evidence
  [switch] $KeepForInspection,
  [int] $UiSeconds = 60
)
$ErrorActionPreference = 'Stop'
if ($RequireGracefulQuit -and ($NoMarker -or $AppNotRunning -or $Cancel -or $Mode)) { throw 'RequireGracefulQuit needs a running real app with the quit-control marker' }
if ($ProductName -eq 'Hubchat' -or $ProductName -notmatch 'Test') { throw "refusing: '$ProductName' is not a disposable test product name" }
if ($Identifier -notmatch '\.test$') { throw "refusing: identifier '$Identifier' must end with .test" }
New-Item -ItemType Directory -Force $Evidence | Out-Null
$log = Join-Path $Evidence 'proof.log'
function Say([string] $m) { $l = '{0:HH:mm:ss} {1}' -f (Get-Date), $m; Add-Content $log $l; Write-Host $l }
$results = [ordered]@{}
function Check([string] $name, [bool] $ok, [string] $detail = '') { $results[$name] = [ordered]@{ pass = $ok; detail = $detail }; Say ("{0}: {1} {2}" -f $(if ($ok) { 'PASS' } else { 'FAIL' }), $name, $detail) }

$uninstKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$ProductName"
$installDir = Join-Path $env:LOCALAPPDATA $ProductName
$roaming = Join-Path $env:APPDATA $Identifier
$local = Join-Path $env:LOCALAPPDATA $Identifier
foreach ($p in $installDir, $roaming, $local) { if (Test-Path $p) { throw "refusing: $p already exists (not mine to reuse)" } }
if (Test-Path $uninstKey) { throw "refusing: $uninstKey already exists" }

# The real Hubchat, which must come out of this exactly as it went in.
$realKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Hubchat'
$realBefore = $null
if (Test-Path $realKey) {
  $rd = (Get-ItemProperty $realKey).InstallLocation.Trim('"')
  $realBefore = [ordered]@{
    version = (Get-ItemProperty $realKey).DisplayVersion
    exeHash = (Get-FileHash (Join-Path $rd 'hubchat.exe')).Hash
    pids = @(Get-Process hubchat -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq (Join-Path $rd 'hubchat.exe') } | ForEach-Object { $_.Id })
    scheme = (Get-ItemProperty 'HKCU:\Software\Classes\hubchat\shell\open\command' -ErrorAction SilentlyContinue).'(default)'
    run = (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue).Hubchat
  }
  Say ("real Hubchat before: " + ($realBefore | ConvertTo-Json -Compress))
}

Add-Type -TypeDefinition @'
using System; using System.Collections.Generic; using System.Runtime.InteropServices; using System.Text;
public static class Ui {
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr parent, EnumProc p, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  public static string Text(IntPtr h) { var s = new StringBuilder(512); GetWindowText(h, s, 512); return s.ToString(); }
  public static string Cls(IntPtr h) { var s = new StringBuilder(128); GetClassName(h, s, 128); return s.ToString(); }
  public static List<IntPtr> Tops(int pid) {
    var r = new List<IntPtr>();
    EnumWindows(delegate (IntPtr h, IntPtr l) { uint p; GetWindowThreadProcessId(h, out p); if (p == pid) r.Add(h); return true; }, IntPtr.Zero);
    return r;
  }
  public static List<string> Controls(IntPtr top) {
    var r = new List<string>();
    EnumChildWindows(top, delegate (IntPtr h, IntPtr l) { r.Add(h.ToInt64() + "|" + Cls(h) + "|" + Text(h)); return true; }, IntPtr.Zero);
    return r;
  }
  public static bool Click(IntPtr top, string text) {
    bool hit = false;
    EnumChildWindows(top, delegate (IntPtr h, IntPtr l) {
      if (!hit && Cls(h) == "Button" && Text(h) == text) { SendMessage(h, 0x00F5, IntPtr.Zero, IntPtr.Zero); hit = true; }
      return true;
    }, IntPtr.Zero);
    return hit;
  }
}
'@

function Setup-Windows([int] $setupPid) {
  # The setup's own processes: the pid and any elevated/extracted children share the exe name.
  $ids = @($setupPid) + @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$setupPid" | ForEach-Object { $_.ProcessId })
  foreach ($i in $ids) { foreach ($h in [Ui]::Tops($i)) { [pscustomobject]@{ H = $h; Pid = $i; Text = [Ui]::Text($h); Cls = [Ui]::Cls($h) } } }
}

$fail = $null
$installedExe = Join-Path $installDir 'hubchat.exe'
try {
  # 1. prior install, silent, no shortcuts
  Say "prior install: $PriorSetup"
  $p1 = Start-Process -FilePath $PriorSetup -ArgumentList '/S', '/NS' -Wait -PassThru
  Check 'prior install exit 0' ($p1.ExitCode -eq 0) "exit $($p1.ExitCode)"
  $u = Get-ItemProperty $uninstKey
  Check 'prior install registered' ($u.DisplayName -eq $ProductName) "DisplayVersion $($u.DisplayVersion) at $($u.InstallLocation)"
  $oldHash = (Get-FileHash $installedExe).Hash
  $oldVersion = $u.DisplayVersion
  Check 'prior install wrote the quit-control marker' ($u.HubchatQuitControl -eq '1')
  if ($NoMarker) { Remove-ItemProperty -Path $uninstKey -Name HubchatQuitControl; Say 'marker removed: an old Hubchat that cannot be asked to quit' }
  if ($Mode) { Set-Content -Path (Join-Path $installDir 'standin-mode.txt') -Value $Mode; Say "stand-in mode: $Mode" }

  # 2. canary in the app data folder (what the upgrade must not touch)
  New-Item -ItemType Directory -Force $roaming | Out-Null
  Set-Content (Join-Path $roaming 'canary.txt') 'settings, hubs, identities, history, keys stay'
  $canary = (Get-FileHash (Join-Path $roaming 'canary.txt')).Hash

  # 3. the app is running
  if ($AppNotRunning) {
    $app = [pscustomobject]@{ Id = 0 }
    Say 'the installed app is not started'
  } else {
    $app = Start-Process -FilePath $installedExe -PassThru
    Start-Sleep -Seconds 3
    $app.Refresh()
    Check 'prior app running' (-not $app.HasExited) "pid $($app.Id)"
    if ($RequireGracefulQuit) {
      # This control has NO WM_CLOSE or termination fallback. A primary that
      # only exits at installer rung 2 must fail before any upgrade is attempted.
      $quitTimer = [Diagnostics.Stopwatch]::StartNew()
      $request = Start-Process -FilePath $installedExe -ArgumentList '--quit-for-upgrade' -PassThru -WindowStyle Hidden
      $primaryGone = $app.WaitForExit(5000)
      $elapsed = $quitTimer.ElapsedMilliseconds
      $requestGone = $request.WaitForExit(5000)
      Check 'real quit request exits the primary within five seconds without fallback' $primaryGone "elapsedMs=$elapsed primary=$($app.Id)"
      Check 'quit request forwarding process exits successfully' ($requestGone -and $request.ExitCode -eq 0) "request=$($request.Id)"
      if (-not $primaryGone -or -not $requestGone) { throw 'real quit request did not settle promptly' }
      $app = Start-Process -FilePath $installedExe -PassThru -WindowStyle Hidden
      Start-Sleep -Seconds 3
      $app.Refresh()
      if ($app.HasExited) { throw 'real app did not restart for the installer control' }
    }
  }
  $stamp = Get-Date
  # Read only this attempt's appended helper lines; old success must not pass.
  $helperLog = Join-Path $env:TEMP 'hubchat-installer-upgrade.log'
  $helperOffset = if (Test-Path -LiteralPath $helperLog) { (Get-Item -LiteralPath $helperLog).Length } else { 0 }

  # 4. new setup, with its window
  Say "new setup: $NewSetup"
  $setup = Start-Process -FilePath $NewSetup -PassThru
  $deadline = (Get-Date).AddSeconds($UiSeconds)
  $upgradePage = $null
  while ((Get-Date) -lt $deadline -and -not $upgradePage) {
    Start-Sleep -Milliseconds 500
    foreach ($w in @(Setup-Windows $setup.Id)) {
      $c = [Ui]::Controls($w.H)
      if ($c -match '\|Button\|&Upgrade$') { $upgradePage = $w; $controls = $c; break }
    }
  }
  Check 'Upgrade page shown' ($null -ne $upgradePage)
  if (-not $upgradePage) { throw 'Upgrade page not found' }
  Set-Content (Join-Path $Evidence 'upgrade-page-controls.txt') $controls
  Check 'page names the install location' ([bool]($controls -match [regex]::Escape("Install location: $installDir")))
  Check 'page offers Advanced setup' ([bool]($controls -match '\|Button\|&Advanced setup$'))
  Check 'Upgrade click delivered' ([Ui]::Click($upgradePage.H, '&Upgrade'))

  if ($Cancel) {
    # The close helper cannot close the app: the Retry/Cancel prompt appears. Press
    # Cancel if it is shown (on an interactive desktop); on a non-interactive window
    # station Windows does not display it and it answers by itself, the same as Cancel.
    $dl = (Get-Date).AddSeconds(15); $pressed = $false
    while ((Get-Date) -lt $dl -and -not $pressed -and -not $setup.HasExited) {
      Start-Sleep -Milliseconds 500
      foreach ($w in @(Setup-Windows $setup.Id)) { if ([Ui]::Controls($w.H) -match '\|Button\|&?Retry$') { if ([Ui]::Click($w.H, 'Cancel')) { $pressed = $true; Say 'pressed Cancel on the Retry/Cancel prompt'; break } } }
    }
    Start-Sleep -Seconds 4
    # Cancel leaves everything as it was, on the Upgrade page.
    $stillOpen = -not $setup.HasExited
    $pageAgain = $false
    if ($stillOpen) { foreach ($w in @(Setup-Windows $setup.Id)) { if ([Ui]::Controls($w.H) -match '\|Button\|&Upgrade$') { $pageAgain = $true } } }
    Check 'cancel: still on the Upgrade page (installer open, Upgrade button present)' ($stillOpen -and $pageAgain)
    Check 'cancel: the app was not closed' ($null -ne (Get-Process -Id $app.Id -ErrorAction SilentlyContinue)) "pid $($app.Id)"
    if ($stillOpen) { Stop-Process -Id $setup.Id -Force; Say 'installer closed by the script (the user would press Cancel Setup)' }
    $setup.WaitForExit(20000) | Out-Null
  } else {
    $setup.WaitForExit(120000) | Out-Null
  }
  Check 'new setup finished' $setup.HasExited
  $setupExit = if ($setup.HasExited) { $setup.ExitCode } else { -1 }
  Say "new setup exit code $setupExit"
  Start-Sleep -Seconds 4

  $u2 = Get-ItemProperty $uninstKey
  $newHash = (Get-FileHash $installedExe).Hash
  $oldAlive = ($app.Id -ne 0) -and ($null -ne (Get-Process -Id $app.Id -ErrorAction SilentlyContinue))
  $inDir = @(Get-Process hubchat -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $installedExe })
  $standin = Join-Path $env:TEMP 'hubchat-standin.log'
  if (Test-Path $standin) { Copy-Item $standin (Join-Path $Evidence 'standin.log') -Force }
  $helperLog = Join-Path $env:TEMP 'hubchat-installer-upgrade.log'
  if (Test-Path $helperLog) { Copy-Item $helperLog (Join-Path $Evidence 'installer-upgrade-helper.log') -Force }

  if ($Cancel) {
    Check 'cancel: installation untouched' ($newHash -eq $oldHash -and $u2.DisplayVersion -eq $oldVersion) "version $($u2.DisplayVersion)"
  } else {
    Check 'installer exit 0' ($setupExit -eq 0) "exit $setupExit"
    Check 'old process is gone' (-not $oldAlive) "pid $($app.Id)"
    if ($RequireGracefulQuit) {
      $tail = ''
      if (Test-Path -LiteralPath $helperLog) {
        $bytes = [IO.File]::ReadAllBytes($helperLog)
        # The helper may rotate its bounded log at the start of this attempt.
        $start = if ($bytes.Length -ge $helperOffset) { [int]$helperOffset } else { 0 }
        $tail = [Text.Encoding]::UTF8.GetString($bytes, $start, $bytes.Length - $start)
      }
      $graceful = $tail -match 'closed gracefully after the quit request' -and $tail -notmatch 'posted WM_CLOSE|terminating pid|no graceful quit within'
      Check 'installer used rung-1 graceful quit without window or force fallback' $graceful
      Set-Content -LiteralPath (Join-Path $Evidence 'this-attempt-helper.log') -Value $tail
    }
    Check 'files replaced' ($newHash -ne $oldHash) "$oldHash -> $newHash"
    Check 'DisplayVersion upgraded' ($u2.DisplayVersion -ne $oldVersion) "$oldVersion -> $($u2.DisplayVersion)"
    Check 'quit-control marker written by the new install' ($u2.HubchatQuitControl -eq '1')
    Check 'same install location' (($u2.InstallLocation.Trim('"')) -eq $installDir) $u2.InstallLocation
    $relaunched = @($inDir | Where-Object { $_.StartTime -gt $stamp })
    Check 'relaunched exactly once from the install folder' ($relaunched.Count -eq 1) ("pids " + (($relaunched | ForEach-Object { "$($_.Id)/session$($_.SessionId)" }) -join ','))
  }
  Check 'canary untouched' ((Test-Path (Join-Path $roaming 'canary.txt')) -and ((Get-FileHash (Join-Path $roaming 'canary.txt')).Hash -eq $canary))
  Check 'no Start Menu/desktop entry made for the product' (-not (Get-ChildItem ([Environment]::GetFolderPath('Programs')), ([Environment]::GetFolderPath('Desktop')) -Filter "$ProductName*" -ErrorAction SilentlyContinue))

  if ($realBefore) {
    $rd = (Get-ItemProperty $realKey).InstallLocation.Trim('"')
    $realAfter = [ordered]@{
      version = (Get-ItemProperty $realKey).DisplayVersion
      exeHash = (Get-FileHash (Join-Path $rd 'hubchat.exe')).Hash
      pids = @(Get-Process hubchat -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq (Join-Path $rd 'hubchat.exe') } | ForEach-Object { $_.Id })
      scheme = (Get-ItemProperty 'HKCU:\Software\Classes\hubchat\shell\open\command' -ErrorAction SilentlyContinue).'(default)'
      run = (Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue).Hubchat
    }
    # pids are reported but not compared: the real app may restart itself (tray Restart, its updater) while this runs.
    $b = [ordered]@{}; foreach ($k in $realBefore.Keys) { if ($k -ne 'pids') { $b[$k] = $realBefore[$k] } }
    $a = [ordered]@{}; foreach ($k in $realAfter.Keys) { if ($k -ne 'pids') { $a[$k] = $realAfter[$k] } }
    Check 'real Hubchat untouched (version, exe hash, scheme, Run key)' (($b | ConvertTo-Json -Compress) -eq ($a | ConvertTo-Json -Compress)) ('pids before ' + ($realBefore.pids -join ',') + ', after ' + ($realAfter.pids -join ','))
  }
} catch {
  $fail = $_.Exception.Message
  Say "ERROR: $fail"
} finally {
  if (-not $KeepForInspection) {
    # only what this run made: the disposable product's processes (by exact path), install, data
    Get-Process hubchat -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $installedExe } | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds 1
    $un = Join-Path $installDir 'uninstall.exe'
    if (Test-Path $un) { Start-Process -FilePath $un -ArgumentList '/S' -Wait | Out-Null; Start-Sleep -Seconds 2 }
    foreach ($p in $installDir, $roaming, $local) { if (Test-Path $p) { Remove-Item $p -Recurse -Force -ErrorAction SilentlyContinue } }
    if (Test-Path $uninstKey) { Remove-Item $uninstKey -Recurse -Force -ErrorAction SilentlyContinue }
    Say 'cleanup done'
  }
  $results | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $Evidence 'result.json')
  $failed = @($results.GetEnumerator() | Where-Object { -not $_.Value.pass })
  Say ("SUMMARY: {0} checks, {1} failed{2}" -f $results.Count, $failed.Count, $(if ($fail) { ", error: $fail" } else { '' }))
  if ($failed.Count -gt 0 -or $fail) { exit 1 }
}
