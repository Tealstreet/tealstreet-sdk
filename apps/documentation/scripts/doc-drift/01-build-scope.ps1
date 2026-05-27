param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..\..")).Path,
    [string]$OutFile = "apps/documentation/scripts/doc-drift/artifacts/terminal-doc-scope.json"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function To-PosixPath {
    param([string]$Path)
    return ($Path -replace "\\", "/")
}

function Get-RelativePosixPath {
    param(
        [string]$BasePath,
        [string]$FullPath
    )

    $baseResolved = (Resolve-Path $BasePath).Path.TrimEnd("\") + "\"
    $fullResolved = (Resolve-Path $FullPath).Path

    $baseUri = New-Object System.Uri($baseResolved)
    $fullUri = New-Object System.Uri($fullResolved)
    $relativeUri = $baseUri.MakeRelativeUri($fullUri)
    $relativePath = [System.Uri]::UnescapeDataString($relativeUri.ToString())

    return (To-PosixPath -Path $relativePath)
}

$repoRootResolved = (Resolve-Path $RepoRoot).Path
$docsRoot = Join-Path $repoRootResolved "apps/documentation"
$currentDocsRoot = Join-Path $docsRoot "docs"
$v2DocsRoot = Join-Path $docsRoot "versioned_docs/version-2.0.0"

$allDocs = Get-ChildItem -Path $currentDocsRoot -Recurse -File -ErrorAction SilentlyContinue |
    Where-Object { $_.Extension -in @(".md", ".mdx") }

$uniquePaths = @()
foreach ($docFile in $allDocs) {
    $relativePath = Get-RelativePosixPath -BasePath $currentDocsRoot -FullPath $docFile.FullName
    $fileName = [System.IO.Path]::GetFileName($relativePath)

    $isInTradeModules = $relativePath.StartsWith("trade/modules/")
    $isInDesktopClient = $relativePath.StartsWith("desktopclient/")
    $isFaq = $fileName -like "FAQ*.md" -or $fileName -like "FAQ*.mdx"

    if ($isInTradeModules -or $isInDesktopClient -or $isFaq) {
        $uniquePaths += $docFile.FullName
    }
}

$uniquePaths = $uniquePaths | Sort-Object -Unique
$entries = @()

foreach ($fullPath in $uniquePaths) {
    $docRelativePath = Get-RelativePosixPath -BasePath $currentDocsRoot -FullPath $fullPath
    $docPath = "apps/documentation/docs/$docRelativePath"

    $v2Candidate = Join-Path $v2DocsRoot ($docRelativePath -replace "/", "\")
    $v2Path = $null
    if (Test-Path $v2Candidate) {
        $v2Path = Get-RelativePosixPath -BasePath $repoRootResolved -FullPath $v2Candidate
    }

    $entries += [pscustomobject]@{
        docPath = $docPath
        v2Path  = $v2Path
    }
}

$scope = [pscustomobject]@{
    generatedAt = (Get-Date).ToUniversalTime().ToString("o")
    scope       = @("trade/modules", "desktopclient", "FAQ")
    count       = $entries.Count
    entries     = $entries
}

$outAbsolutePath = Join-Path $repoRootResolved $OutFile
$outDir = Split-Path -Parent $outAbsolutePath
if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

$scope | ConvertTo-Json -Depth 8 | Set-Content -Path $outAbsolutePath -Encoding UTF8
Write-Host "Wrote scope file: $OutFile ($($entries.Count) entries)"
