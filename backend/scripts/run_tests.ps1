# Pitch Deck Extractor — Test Runner Script
# Prints JSON output locations and total time after tests complete.
#
# Usage:
#   .\scripts\run_tests.ps1              # Run all tests
#   .\scripts\run_tests.ps1 quick        # Quick tests only (~5 sec)
#   .\scripts\run_tests.ps1 full        # Full pipeline only (~7-15 min)

param(
    [string]$Mode = "all"  # all | quick | full
)

$ErrorActionPreference = "Stop"
$ProjRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$TestStart = Get-Date

Write-Host ""
Write-Host "============================================================"
Write-Host "  Pitch Deck Extractor - Test Run"
Write-Host "  Mode: $Mode | Started: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"
Write-Host "============================================================"
Write-Host ""

Push-Location $ProjRoot

try {
    if ($Mode -eq "quick") {
        cargo test --test integration_tests --test smoke_phase1_extraction -- --nocapture
    }
    elseif ($Mode -eq "full") {
        cargo test --test smoke_full_pre_frontend -- --nocapture
    }
    else {
        cargo test -- --nocapture
    }
    $ExitCode = 0
}
catch {
    $ExitCode = 1
}
finally {
    Pop-Location
}

$Elapsed = (Get-Date) - $TestStart

Write-Host ""
Write-Host "============================================================"
Write-Host "  ARTIFACT LOCATIONS (when WRITE_SMOKE_JSON=1)"
Write-Host "============================================================"
Write-Host "  Full smoke (upload + LLM):  $ProjRoot\tests\smoke_test_upload_output.json"
Write-Host "  Full smoke (search):        $ProjRoot\tests\smoke_test_search_output.json"
Write-Host "  Phase 1 extraction:         $ProjRoot\tests\smoke_phase1_output.json"
Write-Host "  Phase 1 comprehensive:      $ProjRoot\tests\smoke_test_output.json"
Write-Host "  Phase 1 PPTX:               $ProjRoot\tests\smoke_test_output_pptx.json"
Write-Host "  E2E results:                $ProjRoot\tests\smoke_test_results.json"
Write-Host "  Vector index meta (full):   $ProjRoot\tests\tmp_full_smoke_index.meta.json"
Write-Host "  Vector index meta (upload): $ProjRoot\tests\tmp_smoke_upload_index.meta.json"
Write-Host "  (Set WRITE_SMOKE_JSON=1 to have tests write these JSON reports.)"
Write-Host "============================================================"
$secs = [math]::Round($Elapsed.TotalSeconds, 2)
$mins = [math]::Round($Elapsed.TotalMinutes, 2)
Write-Host "  TOTAL TIME: $secs seconds ($mins minutes)"
Write-Host "============================================================"
Write-Host ""

exit $ExitCode
