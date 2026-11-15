# Flux clean-checkout verification
#
# Runs the deterministic verification suite from the repository root and writes
# machine-readable aggregate results to implementation/evidence/verify.json.
# Exit code is nonzero if any required step fails.
#
# Two rules this script exists to enforce, and once broke:
#
#   1. One Step block = one command, because Step can only read one $LASTEXITCODE.
#      Chaining commands with ';' inside a block silently judged the whole step by its
#      last command. Step therefore clears LASTEXITCODE before running, so a block that
#      launches no external process cannot inherit the previous step's verdict either.
#   2. A missing prerequisite is blocked_external, never a pass. Steps that need an SDK
#      that is not installed record ok=null plus status='blocked_external'; they do not
#      set the aggregate, so a machine without the SDK still exits 0 -- but verify.json
#      can no longer be read as "that gate ran". The printed summary counts all three.

param(
    [switch]$Quick
)

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root
$results = [ordered]@{}
$ok = $true

function Step($name, $block) {
    Write-Host "== $name =="
    # Clear the inherited status first: a block whose commands all get skipped (a
    # PowerShell-only path) would otherwise be judged on the previous step's exit code.
    $global:LASTEXITCODE = 0
    try {
        & $block
        $code = $LASTEXITCODE
        if ($null -eq $code) { $code = 0 }
        if ($code -ne 0) { $script:ok = $false }
        $script:results[$name] = @{ ok = ($code -eq 0); exit = $code }
    } catch {
        $script:ok = $false
        $script:results[$name] = @{ ok = $false; error = "$_" }
    }
}

function Blocked($name, $reason) {
    # ${name} and not $name: a bare "$name:" parses as a drive-qualified variable.
    Write-Host "== ${name}: blocked_external ($reason) =="
    # ok stays null on purpose: not run, so neither passed nor failed.
    $script:results[$name] = @{
        ok     = $null
        status = 'blocked_external'
        skipped = $true
        reason = $reason
    }
}

# Does the NDI runtime exist where crates/integrations/src/ndi.rs would look for it?
# The bare-name candidate is resolved by LoadLibraryW, whose search starts at the test
# binary's directory (target/debug) and then PATH -- Test-Path cannot answer that, so
# PATH is walked the same way.
function Test-NdiRuntime {
    if ($env:FLUX_NDI_LIB -and (Test-Path -LiteralPath $env:FLUX_NDI_LIB -PathType Leaf)) { return $true }
    foreach ($p in @(
            'C:\Program Files\NDI\NDI 6 Tools\Runtime\Processing.NDI.Lib.x64.dll',
            'C:\Program Files\Resolume Arena\Processing.NDI.Lib.x64.dll')) {
        if (Test-Path -LiteralPath $p -PathType Leaf) { return $true }
    }
    foreach ($dir in ($env:PATH -split ';' | Where-Object { $_ })) {
        if (Test-Path -LiteralPath (Join-Path $dir 'Processing.NDI.Lib.x64.dll') -PathType Leaf) { return $true }
    }
    return $false
}

Step 'fmt' { cargo fmt --all -- --check }
Step 'unit-tests' { cargo test -p flux-domain -p flux-catalog -p flux-engine -p flux-media -p flux-audio -p flux-render -p flux-integrations -p flux-app --lib -- --test-threads=1 }
if (-not $Quick) {
    # Three separate steps: these used to share one block, which recorded only the
    # third one's exit code and hid a producer_tests or export_matrix regression.
    Step 'integration-producer-tests' { cargo test -p flux-tests --test producer_tests -- --test-threads=1 }
    Step 'integration-export-matrix' { cargo test -p flux-tests --test export_matrix -- --test-threads=1 }
    Step 'integration-dag-parallel' { cargo test -p flux-tests --test dag_parallel -- --test-threads=1 }
    Step 'golden' { cargo test -p flux-tests --test golden -- --test-threads=1 }
}
Step 'catalog' { cargo run -q -p xtask -- catalog }
Step 'doctor' { cargo run -q -p xtask -- doctor }
Step 'test-gpu' { cargo run -q -p xtask -- test-gpu --profile baseline }
Step 'shaders-export' { cargo run -q -p xtask --bin export_shaders }
Step 'maxpat' { node apps/max-for-live/check-maxpat.cjs }
Step 'fixtures' { cargo run -q -p xtask -- fixtures }
Step 'cli-build' { cargo build -p flux-app }
Step 'gui-build' { cargo build -p flux-desktop }
# Optional Ableton Link check — only when a prebuilt abl_link.lib is available.
if ($env:FLUX_ABLETON_LINK_LIB) {
    Step 'link-integration' { cargo test -p flux-integrations --features link link:: }
} else {
    Blocked 'link-integration' 'FLUX_ABLETON_LINK_LIB unset'
}
# NDI. Renamed 2026-11-15 (D023): it used to be called 'ndi-discovery', which is what the
# two tests *do*, not what they *prove*. The ndi:: filter selects exactly
# runtime_loads_and_initializes + discovery_returns_state_without_crash, and both return
# early when the runtime cannot be dlopened -- so without this gate a machine with no NDI
# recorded ok:true for a step that ran zero assertions. It also excludes
# ndi_io::tests::shim_loads_and_initializes, which needs apps/ndi-bridge/ndi_shim.dll, and
# proves nothing about interop: a discovery count is whatever the network happens to be
# broadcasting at that minute.
if (Test-NdiRuntime) {
    Step 'ndi-runtime-loads' { cargo test -p flux-integrations --features ndi ndi:: }
} else {
    # Compile-only, so the feature flag stays type-checked even on a machine that cannot
    # run it -- a plain build sets default = [] and would otherwise never build ndi at all.
    Step 'ndi-feature-compiles' { cargo test -p flux-integrations --features ndi --no-run }
    Blocked 'ndi-runtime-loads' 'NDI runtime DLL absent (FLUX_NDI_LIB unset, no NDI 6 Tools or Resolume Arena install, bare name not on PATH)'
}
# Spout2 — runs when the C++ shim DLL is available; test skips cleanly otherwise.
if ($env:FLUX_SPOUT_SHIM) {
    Step 'spout-enumeration' { cargo test -p flux-integrations --features spout spout:: }
} else {
    Blocked 'spout-enumeration' 'FLUX_SPOUT_SHIM unset'
}

$passed = @($results.GetEnumerator() | Where-Object { $_.Value.ok -eq $true }).Count
$failed = @($results.GetEnumerator() | Where-Object { $_.Value.ok -eq $false }).Count
$blocked = @($results.GetEnumerator() | Where-Object { $_.Value.status -eq 'blocked_external' }).Count

# F1 asks every evidence file to name the commit it was produced from; this one did not.
$sha = ''
$git = Get-Command git -ErrorAction SilentlyContinue
if ($git) { $sha = (& git rev-parse --short HEAD | Select-Object -First 1) }
# A gate that runs before its own tree is committed names its parent commit, which is how
# `xtask evidence` came to report a fresh run as stamped_other_commit. Record the state of
# the working tree so the SHA can be read correctly instead of trusted.
# The evidence directory is excluded: every gate above rewrites tracked files in it, so an
# unscoped check would call a clean tree dirty for reasons that have nothing to do with the
# source being measured. Same scope xtask's provenance() uses.
$dirty = $null
if ($git) { $dirty = (@(& git status --porcelain -- ':(exclude,top)implementation/evidence').Count -gt 0) }

$evidence = Join-Path $root 'implementation/evidence'
New-Item -ItemType Directory -Force -Path $evidence | Out-Null
$payload = [ordered]@{
    generatedAt = (Get-Date).ToUniversalTime().ToString('o')
    gitHash = $sha
    gitDirty = $dirty
    gitDirtyScope = 'working tree, excluding implementation/evidence/'
    rustc = (& rustc --version)
    quick = [bool]$Quick
    ok = $ok
    counts = @{ passed = $passed; failed = $failed; blocked_external = $blocked }
    steps = $results
} | ConvertTo-Json -Depth 6
# Windows PowerShell's Set-Content -Encoding UTF8 prepends a BOM, and the committed
# verify.json carried one -- so the "machine-readable" evidence failed JSON.parse unless
# the reader stripped it first. Write UTF-8 without BOM.
[System.IO.File]::WriteAllText(
    (Join-Path $evidence 'verify.json'),
    $payload,
    (New-Object System.Text.UTF8Encoding($false)))

Write-Host ""
Write-Host ("steps: passed=" + $passed + " failed=" + $failed + " blocked_external=" + $blocked)
Write-Host ($(if ($ok) { 'VERIFY: PASS' } else { 'VERIFY: FAIL' }))
if ($ok) { exit 0 } else { exit 1 }
