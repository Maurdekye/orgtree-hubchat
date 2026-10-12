# Runs tools/proof-questions.mjs (question cards and multi-reply against the
# browser mock: Vite dev server plus headless Chrome) inside the existing
# Orgtree Windows Job runner, so bundling, the server and every Chrome
# process end with the job. Dependencies are read in place from this
# checkout's node_modules, never copied or junctioned.
#   -JobRunner  Orgtree apps/desktop/renderer/tests/joblimit.ps1
#   -CacheRoot  assigned build folder: Vite cache, TEMP and Chrome profile
#   -OutputRoot evidence folder: screenshots and result.json
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$JobRunner,
    [Parameter(Mandatory)][string]$CacheRoot,
    [Parameter(Mandatory)][string]$OutputRoot,
    [int]$LimitMB = 4096,
    [int]$TimeoutSec = 300
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $JobRunner -PathType Leaf)) { throw 'Existing repository joblimit.ps1 is required.' }
$repo = Split-Path -Parent $PSScriptRoot
$tmp = Join-Path $CacheRoot 'tmp'
New-Item -ItemType Directory -Path $CacheRoot, $OutputRoot, $tmp -Force | Out-Null
$env:HUBCHAT_VITE_CACHE = Join-Path $CacheRoot 'vite-cache'
$env:HUBCHAT_PROOF_OUT = $OutputRoot
$env:HUBCHAT_JOB_CONTAINED = '1'
$env:TEMP = $tmp
$env:TMP = $tmp
$argFile = Join-Path $CacheRoot 'args.txt'
Set-Content -LiteralPath $argFile -Value (Join-Path $PSScriptRoot 'proof-questions.mjs') -Encoding UTF8
& $JobRunner -LimitMB $LimitMB -TimeoutSec $TimeoutSec -WorkDir $repo -Exe (Get-Command node).Source -ArgFile $argFile
exit $LASTEXITCODE
