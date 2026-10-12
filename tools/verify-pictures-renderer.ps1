[CmdletBinding()]
param(
  [Parameter(Mandatory)][string]$JobRunner,
  [Parameter(Mandatory)][string]$DependencyRoot,
  [Parameter(Mandatory)][string]$DomDependencyRoot,
  [Parameter(Mandatory)][string]$OutputRoot,
  [Parameter(Mandatory)][string]$EvidencePath
)
$ErrorActionPreference='Stop'
$repo=Split-Path -Parent $PSScriptRoot
New-Item -ItemType Directory -Path $OutputRoot -Force | Out-Null
$env:HUBCHAT_DEPENDENCY_ROOT=$DependencyRoot
$env:HUBCHAT_DOM_DEPENDENCY_ROOT=$DomDependencyRoot
$env:HUBCHAT_RENDERER_OUT=$OutputRoot
$env:HUBCHAT_RENDERER_EVIDENCE=$EvidencePath
$env:HUBCHAT_JOB_CONTAINED='1'
$env:TEMP=$OutputRoot
$env:TMP=$OutputRoot
$argFile=Join-Path $OutputRoot 'args.txt'
Set-Content -LiteralPath $argFile -Value (Join-Path $PSScriptRoot 'proof-pictures-renderer.mjs') -Encoding UTF8
& $JobRunner -LimitMB 1536 -TimeoutSec 60 -WorkDir $repo -Exe (Get-Command node).Source -ArgFile $argFile
exit $LASTEXITCODE
