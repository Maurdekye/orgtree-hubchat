param(
  [string]$OutputRoot = 'C:/orgtree-build/hubchat-jumplist-sol',
  [string]$GradleCache = 'D:\orgtree-work\hubchat-toolchain\gradle-home\caches\modules-2\files-2.1',
  [string]$Toolchain = 'E:\hubchat-toolchain',
  [string]$ControlPolicy = ''
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$allowed = [IO.Path]::GetFullPath('C:/orgtree-build/hubchat-jumplist-sol')
$resolved = [IO.Path]::GetFullPath($OutputRoot)
if ($resolved -ne $allowed) { throw 'OutputRoot must be the reserved agent build folder' }
$owner = Get-Content -LiteralPath 'D:\orgtree-work\cargo-target\build.lock\owner.txt'
if ($owner -notmatch '^hubchat-jumplist-sol ') { throw 'Shared build lock is required' }
$run = Join-Path $resolved ('kotlin-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmss'))
New-Item -ItemType Directory -Force -Path $run, "$run\api", "$run\tmp", "$run\classes", "$run\policy" | Out-Null
$env:TEMP = "$run\tmp"
$env:TMP = "$run\tmp"
$utf8 = New-Object System.Text.UTF8Encoding($false)
Add-Type -AssemblyName System.IO.Compression.FileSystem
$jars = @(Get-ChildItem -LiteralPath $GradleCache -Recurse -Filter '*.jar' -File | Select-Object -ExpandProperty FullName)
$compiler = @($jars | Where-Object { $_ -match 'kotlin-compiler-embeddable\\2\.2\.10\\' })
if ($compiler.Count -ne 1) { throw 'Installed Kotlin 2.2.10 compiler was not found' }
$stdlib = @($jars | Where-Object { $_ -match 'kotlin-stdlib\\2\.2\.10\\' })
$compilerDependencies = @($jars | Where-Object {
  $_ -match 'kotlin-script-runtime\\2\.2\.10\\|kotlin-reflect\\1\.6\.10\\|kotlin-daemon-embeddable\\2\.2\.10\\|kotlinx-coroutines-core-jvm\\1\.8\.0\\|org.jetbrains\\annotations\\'
})
$compilerClasspath = (@($compiler) + @($stdlib) + $compilerDependencies) -join ';'
$apis = @()
$aars = @(Get-ChildItem -LiteralPath $GradleCache -Recurse -Filter '*.aar' -File | Sort-Object FullName -Descending)
# The app resolves core1.13.1 and WorkManager2.10.5. Older cached core versions
# must not shadow the API that the app actually compiles against.
$aars = @($aars | Where-Object {
  $_.FullName -notmatch 'androidx.core\\core\\' -or $_.FullName -match 'androidx.core\\core\\1\.13\.1\\'
})
foreach ($aar in $aars) {
  $archive = [IO.Compression.ZipFile]::OpenRead($aar.FullName)
  try {
    $entry = $archive.GetEntry('classes.jar')
    if ($entry) {
      $name = $aar.Directory.Name + '-' + $aar.BaseName + '.jar'
      $dest = Join-Path "$run\api" $name
      [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $dest, $true)
      $apis += $dest
    }
  } finally { $archive.Dispose() }
}
$android = Join-Path $Toolchain 'android-sdk\platforms\android-37.0\android.jar'
if (!(Test-Path -LiteralPath $android)) { throw 'Installed Android37 API was not found' }
$classpath = (@($android) + $apis + $jars) -join ';'
$stubs = @'
package dev.orgtree.hubchat
import android.content.Context
import android.app.Activity
import android.content.BroadcastReceiver
import android.content.Intent
import androidx.work.Worker
import androidx.work.WorkerParameters
class MainActivity : Activity() { companion object { const val inFront = false } }
object R { object drawable { const val ic_stat_hubchat = 1 } }
object LauncherShortcuts { fun update(context: Context, owner: String, json: String) = "" }
object TransferService {
  const val running = false
  fun work(context: Context, on: Boolean) {}
  fun backgroundHint() = ""
  fun coverageChanged(context: Context) {}
}
object PushController {
  fun cachedActive(context: Context) = false
  fun active(context: Context) = false
  fun state(context: Context) = "{}"
  fun set(context: Context, on: Boolean, distributor: String) = ""
  fun refresh(context: Context) {}
}
object DownloadPublisher {
  fun publish(context: Context, key: String, path: String, name: String) = ""
  fun abandoned(context: Context, key: String, source: String, root: String) {}
  fun committed(context: Context, key: String) {}
  fun failure(error: Exception) = ""
}
class CheckWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
  override fun doWork() = Result.success()
}
class InstallReceiver : BroadcastReceiver() { override fun onReceive(context: Context, intent: Intent) {} }
'@
[IO.File]::WriteAllText("$run\tmp\Dependencies.kt", $stubs, $utf8)
$source = Join-Path $repo 'src-tauri\gen\android\app\src\main\java\dev\orgtree\hubchat'
function Run-Compiler([string]$Destination, [string[]]$Sources) {
  $arguments = @('-classpath', $compilerClasspath, 'org.jetbrains.kotlin.cli.jvm.K2JVMCompiler',
    '-no-stdlib', '-no-reflect', '-jvm-target', '1.8', '-classpath', $classpath,
    '-d', $Destination) + $Sources
  $lines = @($arguments | ForEach-Object { '"' + $_.Replace('\', '/') + '"' })
  [IO.File]::WriteAllLines("$run\tmp\compiler.args", $lines, $utf8)
  & "$Toolchain\jdk-21\bin\java.exe" "@$run\tmp\compiler.args"
  if ($LASTEXITCODE -ne 0) { throw 'Kotlin compile failed' }
}
if ($ControlPolicy) {
  $control = [IO.Path]::GetFullPath($ControlPolicy)
  if (!$control.StartsWith($allowed + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Control source must be inside the assigned build scope'
  }
  New-Item -ItemType Directory -Force -Path "$run\control" | Out-Null
  Run-Compiler "$run\control" @($control, "$repo\tools\kotlin\NotificationReplyPolicyChecks.kt")
  $red = @(& "$Toolchain\jdk-21\bin\java.exe" '-cp' ("$run\control;" + ($stdlib -join ';')) 'dev.orgtree.hubchat.NotificationReplyPolicyChecksKt')
  $raw = $LASTEXITCODE
  $red | ForEach-Object { Write-Output $_ }
  Write-Output "RED_RAW_EXIT=$raw"
  if ($raw -ne 1 -or ($red -join "`n") -notmatch '10 checks passed; 2 failed') {
    throw 'Predecessor control did not produce the expected two policy failures'
  }
  Write-Output 'PASS predecessor control: 10 passed / 2 expected failures'
}

Run-Compiler "$run\classes" @("$source\ConnectionService.kt", "$source\SecretBox.kt", "$source\NotificationReply.kt", "$source\NotificationReplyPolicy.kt", "$source\DownloadOpener.kt", "$run\tmp\Dependencies.kt")
Write-Output 'PASS Android API compile: ConnectionService, SecretBox, reply receiver/worker/policy (unrelated app dependencies stubbed)'
Run-Compiler "$run\policy" @("$source\NotificationReplyPolicy.kt", "$repo\tools\kotlin\NotificationReplyPolicyChecks.kt")
& "$Toolchain\jdk-21\bin\java.exe" '-cp' ("$run\policy;" + ($stdlib -join ';')) 'dev.orgtree.hubchat.NotificationReplyPolicyChecksKt'
if ($LASTEXITCODE -ne 0) { throw 'Policy checks failed' }
Write-Output "Evidence build folder: $run"
