param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..\..")).Path,
    [string]$ReportJson = "apps/documentation/scripts/doc-drift/artifacts/terminal-doc-drift-report.json",
    [string]$ReportMarkdown = "apps/documentation/scripts/doc-drift/artifacts/terminal-doc-drift-report.md",
    [string]$PlanMarkdown = "apps/documentation/scripts/doc-drift/artifacts/terminal-doc-remediation-plan.md"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRootResolved = (Resolve-Path $RepoRoot).Path
$reportJsonPath = Join-Path $repoRootResolved $ReportJson
$reportMdPath = Join-Path $repoRootResolved $ReportMarkdown
$planMdPath = Join-Path $repoRootResolved $PlanMarkdown

$report = Get-Content -Path $reportJsonPath -Raw -Encoding UTF8 | ConvertFrom-Json
$entries = @($report.entries)

$countByPriority = @{
    P0 = (@($entries | Where-Object { $_.score.priority -eq "P0" })).Count
    P1 = (@($entries | Where-Object { $_.score.priority -eq "P1" })).Count
    P2 = (@($entries | Where-Object { $_.score.priority -eq "P2" })).Count
    Monitor = (@($entries | Where-Object { $_.score.priority -eq "Monitor" })).Count
}

$reportLines = New-Object System.Collections.Generic.List[string]
$reportLines.Add("# Terminal Docs Drift Report (Phase 1)")
$reportLines.Add("")
$reportLines.Add("- Generated: ``$($report.generatedAt)``")
$reportLines.Add("- Scope size: ``$($entries.Count)`` pages")
$reportLines.Add("- Priority counts: ``P0=$($countByPriority.P0)``, ``P1=$($countByPriority.P1)``, ``P2=$($countByPriority.P2)``, ``Monitor=$($countByPriority.Monitor)``")
$reportLines.Add("- Method: $($report.method).")
$reportLines.Add("")
$reportLines.Add("## Ranked Pages")
$reportLines.Add("")
$reportLines.Add("| Priority | Score | Doc | Primary Issue | Suggested Action |")
$reportLines.Add("|---|---:|---|---|---|")

foreach ($entry in $entries) {
    $primaryIssue = if (@($entry.issues).Count -gt 0) { $entry.issues[0].type } else { "none_detected" }
    $reportLines.Add("| $($entry.score.priority) | $($entry.score.total) | ``$($entry.docPath)`` | ``$primaryIssue`` | ``$($entry.recommendedAction)`` |")
}

$reportLines.Add("")
$reportLines.Add("## Top 10 Investigation Targets")
$reportLines.Add("")
foreach ($entry in ($entries | Select-Object -First 10)) {
    $reportLines.Add("- ``$($entry.docPath)`` -> score ``$($entry.score.total)`` ($($entry.score.priority)), notes ``$($entry.notes)``")
}

$reportLines.Add("")
$reportLines.Add("## Interpretation Notes")
$reportLines.Add("")
$reportLines.Add("- Generated from the split PowerShell audit scripts.")
$reportLines.Add("- High V2 similarity usually indicates carry-over pages that need deeper modernization.")
$reportLines.Add("- Use this ranking for triage; keep final doc decisions manual.")

$reportLines -join "`n" | Set-Content -Path $reportMdPath -Encoding UTF8

$planLines = New-Object System.Collections.Generic.List[string]
$planLines.Add("# Terminal Docs Remediation Plan")
$planLines.Add("")
$planLines.Add("## Wave 1 (P0)")

$p0Entries = @($entries | Where-Object { $_.score.priority -eq "P0" })
if ($p0Entries.Count -eq 0) {
    $planLines.Add("- No P0 pages detected in this pass.")
}
else {
    foreach ($entry in $p0Entries) {
        $planLines.Add("- Rewrite ``$($entry.docPath)``")
    }
}

$planLines.Add("")
$planLines.Add("## Wave 2 (P1)")

$p1Entries = @($entries | Where-Object { $_.score.priority -eq "P1" } | Select-Object -First 12)
if ($p1Entries.Count -eq 0) {
    $planLines.Add("- No P1 pages detected in this pass.")
}
else {
    foreach ($entry in $p1Entries) {
        $issueType = if (@($entry.issues).Count -gt 0) { $entry.issues[0].type } else { "manual_review" }
        $planLines.Add("- Surgical edit ``$($entry.docPath)`` (``$issueType``)")
    }
}

$planLines.Add("")
$planLines.Add("## Review Protocol")
$planLines.Add("- Validate wording against current terminal UI before publishing.")
$planLines.Add("- Refresh stale screenshots/GIFs when labels or workflows changed.")
$planLines.Add("- Re-run this pass after each edit batch.")

$planLines -join "`n" | Set-Content -Path $planMdPath -Encoding UTF8

Write-Host "Wrote markdown report: $ReportMarkdown"
Write-Host "Wrote remediation plan: $PlanMarkdown"
