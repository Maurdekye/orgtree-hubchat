# Runs the Hubchat symmetry S3 proof inside the existing repository Windows Job runner. Dependencies are read in place,
# never copied or junctioned. The output folder must be new, under C:/orgtree-build. ASCII only (Windows PowerShell 5.1
# reads this as ANSI).
#   default                       tools/proof-symmetry-s3.mjs: bundling AND the mounted real App in a DOM with no window.
#   -Script ...-capture.mjs       tools/proof-symmetry-s3-capture.mjs: the same checks in an offscreen Electron window plus
#                                 screenshots. SESSION 0 ONLY (it asserts the session); never in the user's session.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$JobRunner,
    [Parameter(Mandatory)][string]$DependencyRoot,
    [Parameter(Mandatory)][string]$DomDependencyRoot,
    [Parameter(Mandatory)][string]$OutputRoot,
    [Parameter(Mandatory)][string]$EvidencePath,
    [ValidateSet('red','green')][string]$Phase = 'green',
    [ValidateSet('proof-symmetry-s3.mjs','proof-symmetry-s3-capture.mjs')][string]$Script = 'proof-symmetry-s3.mjs'
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $JobRunner -PathType Leaf)) { throw 'Existing repository joblimit.ps1 is required.' }
if (Test-Path -LiteralPath $OutputRoot) { throw 'The output folder must be new: ' + $OutputRoot }
$repo = Split-Path -Parent $PSScriptRoot
New-Item -ItemType Directory -Path $OutputRoot -Force | Out-Null
$env:HUBCHAT_DEPENDENCY_ROOT = $DependencyRoot
$env:HUBCHAT_DOM_DEPENDENCY_ROOT = $DomDependencyRoot
# the capture proof insists on creating its own folder, so it gets a child of the new output folder
$proofOut = $OutputRoot
if ($Script -like '*capture*') { $proofOut = Join-Path $OutputRoot 'run' }
$env:HUBCHAT_RENDERER_OUT = $proofOut
$env:HUBCHAT_RENDERER_EVIDENCE = $EvidencePath
$env:HUBCHAT_PROOF_PHASE = $Phase
$env:HUBCHAT_JOB_CONTAINED = '1'
$env:TEMP = $OutputRoot
$env:TMP = $OutputRoot
$argFile = Join-Path $OutputRoot 'args.txt'
Set-Content -LiteralPath $argFile -Value (Join-Path $PSScriptRoot $Script) -Encoding ASCII
& $JobRunner -LimitMB 3072 -TimeoutSec 300 -WorkDir $repo -Exe (Get-Command node).Source -ArgFile $argFile
exit $LASTEXITCODE
