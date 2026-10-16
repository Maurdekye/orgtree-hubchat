param(
  [string]$OutputRoot = 'C:/orgtree-build/hubchat-jumplist-sol',
  [string]$GradleCache = 'D:/orgtree-work/hubchat-toolchain/gradle-home/caches/modules-2/files-2.1',
  [string]$Toolchain = 'E:/hubchat-toolchain'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
if ([IO.Path]::GetFullPath($OutputRoot) -ne [IO.Path]::GetFullPath('C:/orgtree-build/hubchat-jumplist-sol')) {
  throw 'Output must remain in the assigned agent scope'
}
if ((Get-Content 'D:/orgtree-work/cargo-target/build.lock/owner.txt') -notmatch '^hubchat-jumplist-sol ') {
  throw 'Shared build lock is required'
}
$run = Join-Path $OutputRoot ('kotlin-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmss'))
New-Item -ItemType Directory -Force -Path $run, "$run/tmp", "$run/classes", "$run/policy" | Out-Null
$env:TEMP = "$run/tmp"
$env:TMP = "$run/tmp"
$utf8 = [Text.UTF8Encoding]::new($false)
$jars = @(Get-ChildItem -LiteralPath $GradleCache -Recurse -Filter '*.jar' -File | Select-Object -ExpandProperty FullName)
$compiler = @($jars | Where-Object { $_ -match 'kotlin-compiler-embeddable\\2\.2\.10\\' })
if ($compiler.Count -ne 1) { throw 'Installed Kotlin compiler not found' }
$stdlib = @($jars | Where-Object { $_ -match 'kotlin-stdlib\\2\.2\.10\\' })
$compilerDependencies = @($jars | Where-Object {
  $_ -match 'kotlin-script-runtime\\2\.2\.10\\|kotlin-reflect\\1\.6\.10\\|kotlin-daemon-embeddable\\2\.2\.10\\|kotlinx-coroutines-core-jvm\\1\.8\.0\\|org.jetbrains\\annotations\\'
})
$compilerClasspath = (@($compiler) + @($stdlib) + $compilerDependencies) -join ';'
$android = Join-Path $Toolchain 'android-sdk/platforms/android-37.0/android.jar'
if (!(Test-Path $android)) { throw 'Installed Android37 API not found' }
$classpath = (@($android) + $stdlib + $compilerDependencies) -join ';'
$source = Join-Path $repo 'src-tauri/gen/android/app/src/main/java/dev/orgtree/hubchat'
[IO.File]::WriteAllText("$run/tmp/MainActivity.kt", "package dev.orgtree.hubchat`nclass MainActivity : android.app.Activity()", $utf8)
function Compile([string]$Destination, [string[]]$Sources) {
  $arguments = @('-classpath', $compilerClasspath, 'org.jetbrains.kotlin.cli.jvm.K2JVMCompiler',
    '-no-stdlib', '-no-reflect', '-jvm-target', '1.8', '-classpath', $classpath, '-d', $Destination) + $Sources
  [IO.File]::WriteAllLines("$run/tmp/compiler.args", @($arguments | ForEach-Object { '"' + $_.Replace('\', '/') + '"' }), $utf8)
  & "$Toolchain/jdk-21/bin/java.exe" "@$run/tmp/compiler.args"
  if ($LASTEXITCODE -ne 0) { throw 'Kotlin compile failed' }
}
Compile "$run/classes" @("$source/LauncherShortcuts.kt", "$source/LauncherShortcutPolicy.kt", "$run/tmp/MainActivity.kt")
Write-Output 'PASS Android API compile: real shortcut publisher/policy; only Activity superclass stubbed'
Compile "$run/policy" @("$source/LauncherShortcutPolicy.kt", "$repo/tools/kotlin/LauncherShortcutPolicyChecks.kt")
& "$Toolchain/jdk-21/bin/java.exe" '-cp' ("$run/policy;" + ($stdlib -join ';')) 'dev.orgtree.hubchat.LauncherShortcutPolicyChecksKt'
if ($LASTEXITCODE -ne 0) { throw 'Launcher policy checks failed' }
Write-Output "Evidence build folder: $run"
