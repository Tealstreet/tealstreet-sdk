param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..\..")).Path,
    [string]$ScopeFile = "apps/documentation/scripts/doc-drift/artifacts/terminal-doc-scope.json",
    [string]$OutFile = "apps/documentation/scripts/doc-drift/artifacts/terminal-doc-drift-report.json",
    [switch]$TreatDraftAsSignal
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Convert-ToRepoPath {
    param(
        [string]$RepoRootPath,
        [string]$RelativePosixPath
    )

    $windowsRelative = $RelativePosixPath -replace "/", "\"
    return (Join-Path $RepoRootPath $windowsRelative)
}

function Get-FileContentSafe {
    param([string]$Path)
    return (Get-Content -Path $Path -Raw -Encoding UTF8)
}

function Get-NormalizedTokens {
    param([string]$Text)

    $withoutFrontmatter = $Text -replace "(?s)^---.*?---", " "
    $normalized = $withoutFrontmatter.ToLowerInvariant() -replace "[^a-z0-9]+", " "
    $tokens = $normalized.Split(" ", [System.StringSplitOptions]::RemoveEmptyEntries) |
        Where-Object { $_.Length -ge 3 } |
        Sort-Object -Unique
    return @($tokens)
}

function Get-JaccardSimilarity {
    param(
        [string]$A,
        [string]$B
    )

    $tokensA = Get-NormalizedTokens -Text $A
    $tokensB = Get-NormalizedTokens -Text $B

    if (($tokensA.Count -eq 0) -and ($tokensB.Count -eq 0)) {
        return $null
    }

    $setA = [System.Collections.Generic.HashSet[string]]::new([string[]]$tokensA)
    $setB = [System.Collections.Generic.HashSet[string]]::new([string[]]$tokensB)

    $intersection = [System.Collections.Generic.HashSet[string]]::new($setA)
    $intersection.IntersectWith($setB)

    $union = [System.Collections.Generic.HashSet[string]]::new($setA)
    $union.UnionWith($setB)

    if ($union.Count -eq 0) {
        return $null
    }

    return [Math]::Round($intersection.Count / [double]$union.Count, 3)
}

function Get-LastUpdatedDays {
    param(
        [string]$RepoRootPath,
        [string]$RelativePosixPath
    )

    $result = & git -C $RepoRootPath log -1 --format=%ct -- $RelativePosixPath 2>$null
    if (($LASTEXITCODE -ne 0) -or [string]::IsNullOrWhiteSpace($result)) {
        return $null
    }

    $unixSeconds = [int64]$result.Trim()
    $dt = [DateTimeOffset]::FromUnixTimeSeconds($unixSeconds).UtcDateTime
    return [Math]::Floor(((Get-Date).ToUniversalTime() - $dt).TotalDays)
}

function New-Issue {
    param(
        [string]$Type,
        [string]$Description,
        [string]$DocSnippet
    )

    return [pscustomobject]@{
        type        = $Type
        description = $Description
        evidence    = [pscustomobject]@{
            docSnippet = $DocSnippet
            codePaths  = @()
        }
    }
}

$repoRootResolved = (Resolve-Path $RepoRoot).Path
$scopeAbsolutePath = Join-Path $repoRootResolved $ScopeFile
$scope = Get-Content -Path $scopeAbsolutePath -Raw -Encoding UTF8 | ConvertFrom-Json

$legacyRouteMarkers = @(
    "/docs/connect/okex",
    "/docs/connect/mexc",
    "/docs/connect/gateio",
    "/docs/connect/kucoin"
)

$stalePhrases = @(
    "in the meantime, you will have to",
    "official code signing",
    "as of this writing",
    "open anyways",
    "tealstreet-2.3.5.appimage"
)

$entries = @()

foreach ($entry in $scope.entries) {
    $docPath = [string]$entry.docPath
    $docAbsolutePath = Convert-ToRepoPath -RepoRootPath $repoRootResolved -RelativePosixPath $docPath
    $docText = Get-FileContentSafe -Path $docAbsolutePath
    $docLower = $docText.ToLowerInvariant()

    $v2Path = if ($entry.v2Path) { [string]$entry.v2Path } else { "" }
    $v2Similarity = $null
    if (-not [string]::IsNullOrWhiteSpace($v2Path)) {
        $v2AbsolutePath = Convert-ToRepoPath -RepoRootPath $repoRootResolved -RelativePosixPath $v2Path
        if (Test-Path $v2AbsolutePath) {
            $v2Text = Get-FileContentSafe -Path $v2AbsolutePath
            $v2Similarity = Get-JaccardSimilarity -A $docText -B $v2Text
        }
    }

    $legacyLinks = @([regex]::Matches($docText, '\]\((/docs[^)\s#]+)') | ForEach-Object { $_.Groups[1].Value } |
        Where-Object { $link = $_; $legacyRouteMarkers | Where-Object { $link.Contains($_) } } |
        Sort-Object -Unique)

    $imagePaths = @([regex]::Matches($docText, '!\[[^\]]*\]\(([^)]+)\)') | ForEach-Object { $_.Groups[1].Value })
    $gifCount = @($imagePaths | Where-Object { $_.ToLowerInvariant().Contains(".gif") }).Count

    $stalePhraseHits = 0
    foreach ($phrase in $stalePhrases) {
        if ($docLower.Contains($phrase)) {
            $stalePhraseHits++
        }
    }

    $isDraft = $docLower.Contains("draft: true")

    $issues = @()
    $mismatchEvidence = 0

    if ($null -ne $v2Similarity) {
        if ($v2Similarity -ge 0.85) {
            $mismatchEvidence += 3
            $issues += New-Issue -Type "version_drift" -Description "Page remains very close to V2 copy." -DocSnippet "v2_similarity=$v2Similarity"
        }
        elseif ($v2Similarity -ge 0.65) {
            $mismatchEvidence += 2
            $issues += New-Issue -Type "version_drift" -Description "Page has high V2 overlap and likely needs modernization polish." -DocSnippet "v2_similarity=$v2Similarity"
        }
        elseif ($v2Similarity -ge 0.50) {
            $mismatchEvidence += 1
        }
    }

    if ($legacyLinks.Count -gt 0) {
        $mismatchEvidence += 1
        $issues += New-Issue -Type "link_drift" -Description "Contains links to legacy connect routes." -DocSnippet ($legacyLinks -join ", ")
    }

    if ($stalePhraseHits -ge 2) {
        $mismatchEvidence += 1
        $issues += New-Issue -Type "term_drift" -Description "Contains legacy release-era phrasing." -DocSnippet "stale_phrase_hits=$stalePhraseHits"
    }

    if ($gifCount -ge 3) {
        $mismatchEvidence += 1
        $issues += New-Issue -Type "asset_drift" -Description "Heavy GIF usage may indicate stale UI captures." -DocSnippet "gif_assets=$gifCount"
    }

    if ($isDraft -and $TreatDraftAsSignal.IsPresent) {
        $mismatchEvidence += 1
        $issues += New-Issue -Type "status_drift" -Description "Page is marked draft." -DocSnippet "draft: true"
    }

    if ($mismatchEvidence -gt 5) {
        $mismatchEvidence = 5
    }

    $userImpact = 1
    if ($docPath -match "/FAQ(\.mdx?|/)") {
        $userImpact = 3
    }
    elseif ($docPath.Contains("/desktopclient/")) {
        $userImpact = 3
    }
    elseif ($docPath.Contains("/trade/modules/")) {
        $userImpact = 2
    }
    elseif ($docPath -match "/(position-history|basic-chart|ticker-data|dom|aggr)\.mdx?$") {
        $userImpact = 2
    }

    $confidence = 0
    if ($mismatchEvidence -ge 3) {
        $confidence = 2
    }
    elseif ($mismatchEvidence -ge 2) {
        $confidence = 1
    }

    $ageSignal = 0
    $lastUpdatedDays = Get-LastUpdatedDays -RepoRootPath $repoRootResolved -RelativePosixPath $docPath
    if ($null -ne $lastUpdatedDays) {
        if ($lastUpdatedDays -gt 730) {
            $ageSignal = 2
        }
        elseif ($lastUpdatedDays -gt 365) {
            $ageSignal = 1
        }
    }

    $totalScore = $mismatchEvidence + $userImpact + $confidence + $ageSignal
    $priority = if ($totalScore -ge 9) { "P0" } elseif ($totalScore -ge 6) { "P1" } elseif ($totalScore -ge 3) { "P2" } else { "Monitor" }
    $recommendedAction = if ($priority -eq "P0") { "rewrite" } elseif ($priority -eq "P1") { "surgical_edit" } elseif ($priority -eq "P2") { "cleanup_review" } else { "keep" }

    $titleMatch = [regex]::Match($docText, "^title:\\s*(.+)$", [System.Text.RegularExpressions.RegexOptions]::Multiline)
    $title = if ($titleMatch.Success) { $titleMatch.Groups[1].Value.Trim().Trim('"') } else { [System.IO.Path]::GetFileNameWithoutExtension($docPath) }

    $featureKeywords = @([regex]::Split([System.IO.Path]::GetFileNameWithoutExtension($docPath), "[-_\\s]+") | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    $lastUpdatedDaysText = if ($null -eq $lastUpdatedDays) { "unknown" } else { [string]$lastUpdatedDays }
    $v2SimilarityText = if ($null -eq $v2Similarity) { "n/a" } else { [string]$v2Similarity }
    $isDraftText = if ($isDraft) { "true" } else { "false" }
    $notes = "lastUpdatedDays=$lastUpdatedDaysText; v2Similarity=$v2SimilarityText; draft=$isDraftText"

    $entries += [pscustomobject]@{
        docPath         = $docPath
        v2ComparisonPath = $v2Path
        title           = $title
        featureKeywords = $featureKeywords
        codePathsChecked = @("apps/web/src", "packages/worker-*", "packages/safe-cex")
        codeSignal      = [pscustomobject]@{
            keywordHitCount   = 0
            topMatchingPaths = @()
        }
        score = [pscustomobject]@{
            mismatchEvidence = $mismatchEvidence
            userImpact       = $userImpact
            confidence       = $confidence
            ageSignal        = $ageSignal
            total            = $totalScore
            priority         = $priority
        }
        issues = $issues
        recommendedAction = $recommendedAction
        notes = $notes
    }
}

$sortedEntries = $entries | Sort-Object @{ Expression = { - $_.score.total } }, @{ Expression = { $_.docPath } }

$report = [pscustomobject]@{
    generatedAt = (Get-Date).ToUniversalTime().ToString("o")
    scope = @("trade/modules", "desktopclient", "FAQ")
    method = "split-script rerank (v2 similarity + stale phrasing + route/asset checks; draft ignored by default)"
    entries = $sortedEntries
}

$outAbsolutePath = Join-Path $repoRootResolved $OutFile
$outDir = Split-Path -Parent $outAbsolutePath
if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

$report | ConvertTo-Json -Depth 12 | Set-Content -Path $outAbsolutePath -Encoding UTF8

$prioritySummary = $sortedEntries | Group-Object { $_.score.priority } | Sort-Object Name
$summaryText = $prioritySummary | ForEach-Object { "$($_.Name)=$($_.Count)" }
Write-Host "Wrote report: $OutFile"
Write-Host ("Priority counts: " + ($summaryText -join ", "))
