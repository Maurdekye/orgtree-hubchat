# Launch smoke for the Hubchat NSIS installer, run on a GitHub Windows runner only:
# silent install, start the app, check it stays up, stop it, uninstall.
param(
  [Parameter(Mandatory)] [string] $Installer,
  [int] $Seconds = 30
)
$ErrorActionPreference = 'Stop'

$p = Start-Process -FilePath (Resolve-Path $Installer) -ArgumentList '/S' -Wait -PassThru
if ($p.ExitCode -ne 0) { throw "installer exited with $($p.ExitCode)" }

$uninstallKeys = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
                 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*'
$entry = Get-ItemProperty $uninstallKeys -ErrorAction SilentlyContinue |
  Where-Object { $_.DisplayName -eq 'Hubchat' } | Select-Object -First 1
if (-not $entry) { throw 'no Hubchat uninstall entry after install' }
$dir = $entry.InstallLocation.Trim('"')
"Installed $($entry.DisplayName) $($entry.DisplayVersion) to $dir"
Get-ChildItem $dir -Recurse -File | Format-Table FullName, Length

$exe = Get-ChildItem $dir -Filter *.exe | Where-Object { $_.Name -notmatch '^uninstall' } | Select-Object -First 1
if (-not $exe) { throw "no app .exe in $dir" }
$ver = $exe.VersionInfo
"App: $($exe.Name) FileVersion=$($ver.FileVersion) ProductVersion=$($ver.ProductVersion) ProductName=$($ver.ProductName)"

$app = Start-Process -FilePath $exe.FullName -PassThru
Start-Sleep -Seconds $Seconds
$app.Refresh()
if ($app.HasExited) { throw "$($exe.Name) exited after start with code $($app.ExitCode)" }
"$($exe.Name) still running after $Seconds s (pid $($app.Id))"
Get-CimInstance Win32_Process |
  Where-Object { $_.ParentProcessId -eq $app.Id -or $_.ProcessId -eq $app.Id } |
  Format-Table ProcessId, ParentProcessId, Name

Stop-Process -Id $app.Id -Force
Start-Sleep -Seconds 2
Get-Process msedgewebview2 -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue

$un = $entry.UninstallString.Trim('"')
$u = Start-Process -FilePath $un -ArgumentList '/S' -Wait -PassThru
"Uninstaller exited with $($u.ExitCode)"
