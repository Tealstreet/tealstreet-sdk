param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..\..")).Path,
    [string]$ScopeFile = "apps/documentation/scripts/doc-drift/artifacts/terminal-doc-scope.json",
    [string]$ReportJson = "apps/documentation/scripts/doc-drift/artifacts/terminal-doc-drift-report.json",
    [string]$ReportMarkdown = "apps/documentation/scripts/doc-drift/artifacts/terminal-doc-drift-report.md",
    [string]$PlanMarkdown = "apps/documentation/scripts/doc-drift/artifacts/terminal-doc-remediation-plan.md",
    [switch]$TreatDraftAsSignal
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$step1 = Join-Path $PSScriptRoot "01-build-scope.ps1"
$step2 = Join-Path $PSScriptRoot "02-score-drift.ps1"
$step3 = Join-Path $PSScriptRoot "03-render-reports.ps1"

Write-Host "Running terminal docs drift pass (split scripts)..."
Write-Host "Step 1/3: Build scope"
& $step1 -RepoRoot $RepoRoot -OutFile $ScopeFile

Write-Host "Step 2/3: Score drift"
if ($TreatDraftAsSignal.IsPresent) {
    & $step2 -RepoRoot $RepoRoot -ScopeFile $ScopeFile -OutFile $ReportJson -TreatDraftAsSignal
}
else {
    & $step2 -RepoRoot $RepoRoot -ScopeFile $ScopeFile -OutFile $ReportJson
}

Write-Host "Step 3/3: Render markdown outputs"
& $step3 -RepoRoot $RepoRoot -ReportJson $ReportJson -ReportMarkdown $ReportMarkdown -PlanMarkdown $PlanMarkdown

Write-Host "Done."
Write-Host " - Scope: $ScopeFile"
Write-Host " - Report JSON: $ReportJson"
Write-Host " - Report MD: $ReportMarkdown"
Write-Host " - Plan MD: $PlanMarkdown"
