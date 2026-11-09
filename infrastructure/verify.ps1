# Flux clean-checkout verification
#
# Runs the deterministic verification suite from the repository root and writes
# machine-readable aggregate results to implementation/evidence/verify.json.
# Exit code is nonzero if any required step fails. GPU/media steps report
# `unavailable` rather than failing when hardware is absent.

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

Step 'fmt' { cargo fmt --all -- --check }
Step 'unit-tests' { cargo test -p flux-domain -p flux-catalog -p flux-engine -p flux-media -p flux-audio -p flux-render -p flux-integrations -p flux-app --lib }
if (-not $Quick) {
    Step 'integration-tests' { cargo test -p flux-tests --test producer_tests; cargo test -p flux-tests --test export_matrix }
    Step 'golden' { cargo test -p flux-tests --test golden }
}
Step 'catalog' { cargo run -q -p xtask -- catalog }
Step 'doctor' { cargo run -q -p xtask -- doctor }
Step 'test-gpu' { cargo run -q -p xtask -- test-gpu --profile baseline }
Step 'fixtures' { cargo run -q -p xtask -- fixtures }
Step 'cli-build' { cargo build -p flux-app }
Step 'gui-build' { cargo build -p flux-desktop }

$evidence = Join-Path $root 'implementation/evidence'
New-Item -ItemType Directory -Force -Path $evidence | Out-Null
$payload = [ordered]@{
    generatedAt = (Get-Date).ToUniversalTime().ToString('o')
    rustc = (& rustc --version)
    quick = [bool]$Quick
    ok = $ok
    steps = $results
} | ConvertTo-Json -Depth 6
Set-Content -Path (Join-Path $evidence 'verify.json') -Value $payload -Encoding UTF8

Write-Host ""
Write-Host ($(if ($ok) { 'VERIFY: PASS' } else { 'VERIFY: FAIL' }))
if ($ok) { exit 0 } else { exit 1 }

