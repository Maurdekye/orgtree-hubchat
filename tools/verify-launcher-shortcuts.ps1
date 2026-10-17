param(
  [string]$OutputRoot = 'C:/orgtree-build/hubchat-jumplist-sol',
  [string]$GradleCache = 'D:/orgtree-work/hubchat-toolchain/gradle-home/caches/modules-2/files-2.1',
  [string]$Toolchain = 'E:/hubchat-toolchain'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
& node "$repo/tools/generate-launcher-avatar.mjs" --fixtures "$OutputRoot/LauncherAvatarParityChecks.kt"
if ($LASTEXITCODE -ne 0) { throw 'Native avatar recipe differs from client source' }
if ([IO.Path]::GetFullPath($OutputRoot) -ne [IO.Path]::GetFullPath('C:/orgtree-build/hubchat-jumplist-sol')) {
  throw 'Output must remain in the assigned agent scope'
}
if ((Get-Content 'D:/orgtree-work/cargo-target/build.lock/owner.txt') -notmatch '^hubchat-jumplist-sol ') {
  throw 'Shared build lock is required'
}
$run = Join-Path $OutputRoot ('kotlin-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmss'))
New-Item -ItemType Directory -Force -Path $run, "$run/tmp", "$run/classes", "$run/policy", "$run/api" | Out-Null
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
Add-Type -AssemblyName System.IO.Compression.FileSystem
$apis = @()
$aars = @(Get-ChildItem -LiteralPath $GradleCache -Recurse -Filter '*.aar' -File | Sort-Object FullName -Descending)
$aars = @($aars | Where-Object {
  $_.FullName -notmatch 'androidx.core\\core\\' -or $_.FullName -match 'androidx.core\\core\\1\.13\.1\\'
})
foreach ($aar in $aars) {
  $archive = [IO.Compression.ZipFile]::OpenRead($aar.FullName)
  try {
    $entry = $archive.GetEntry('classes.jar')
    if ($entry) {
      $dest = Join-Path "$run/api" ($aar.Directory.Name + '-' + $aar.BaseName + '.jar')
      [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $dest, $true)
      $apis += $dest
    }
  } finally { $archive.Dispose() }
}
$classpath = (@($android) + $apis + $jars) -join ';'
$source = Join-Path $repo 'src-tauri/gen/android/app/src/main/java/dev/orgtree/hubchat'
function Compile([string]$Destination, [string[]]$Sources) {
  $arguments = @('-classpath', $compilerClasspath, 'org.jetbrains.kotlin.cli.jvm.K2JVMCompiler',
    '-no-stdlib', '-no-reflect', '-jvm-target', '1.8', '-classpath', $classpath, '-d', $Destination) + $Sources
  [IO.File]::WriteAllLines("$run/tmp/compiler.args", @($arguments | ForEach-Object { '"' + $_.Replace('\', '/') + '"' }), $utf8)
  & "$Toolchain/jdk-21/bin/java.exe" "@$run/tmp/compiler.args"
  if ($LASTEXITCODE -ne 0) { throw 'Kotlin compile failed' }
}
Compile "$run/classes" @("$source/LauncherShortcuts.kt", "$source/LauncherShortcutPolicy.kt", "$source/LauncherAvatar.kt", "$source/LauncherAvatarRecipe.kt", "$source/ConnectionService.kt",
  "$source/SecretBox.kt", "$source/NotificationReply.kt", "$source/NotificationReplyPolicy.kt", "$source/DownloadOpener.kt",
  "$repo/tools/kotlin/LauncherShortcutBridgeStubs.kt")
Write-Output 'PASS Android API compile: real shortcut publisher/policy and ConnectionService bridge; unrelated app dependencies stubbed'
Compile "$run/policy" @("$source/LauncherShortcutPolicy.kt", "$source/LauncherAvatarRecipe.kt", "$repo/tools/kotlin/LauncherShortcutPolicyChecks.kt", "$OutputRoot/LauncherAvatarParityChecks.kt")
& "$Toolchain/jdk-21/bin/java.exe" '-cp' ("$run/policy;" + ($stdlib -join ';')) 'dev.orgtree.hubchat.LauncherShortcutPolicyChecksKt'
if ($LASTEXITCODE -ne 0) { throw 'Launcher policy checks failed' }
Write-Output "Evidence build folder: $run"
