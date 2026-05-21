#Requires -Version 5.1

# ============================================================================
# Project Statistics Report Generator
# ============================================================================
#
# Usage: .\scripts\project-stats.ps1
#
# Generates a comprehensive statistics report for the qualia-signal-foundry
# project, including:
# - Rust source code lines by crate
# - Authored frontend source lines
# - Config and test data file counts
# - PowerShell script lines
# - Documentation lines (plans/designs/ideas, other)
# - External dependency count
# ============================================================================

# --- Configuration ---
$ErrorActionPreference = "Stop"
$projectRoot = Split-Path -Parent $PSScriptRoot

# --- Helper Functions ---

function Format-Number {
    param(
        [double]$Value,
        [int]$Width = 0
    )
    $formatted = $Value.ToString("N0", [System.Globalization.CultureInfo]::InvariantCulture)
    if ($Width -gt 0) {
        $formatted = $formatted.PadLeft($Width)
    }
    return $formatted
}


function Count-Lines {
    param(
        [System.IO.FileInfo[]]$Files = @()
    )

    if (-not $Files -or $Files.Count -eq 0) {
        return 0
    }

    $totalLines = 0
    foreach ($file in $Files) {
        try {
            $totalLines += (Get-Content $file.FullName -ErrorAction SilentlyContinue).Count
        } catch {
            Write-Warning "Could not read file: $($file.FullName)"
        }
    }

    return $totalLines
}

function Count-RustTests {
    param(
        [System.IO.FileInfo[]]$Files = @()
    )

    if (-not $Files -or $Files.Count -eq 0) {
        return 0
    }

    $totalTests = 0
    foreach ($file in $Files) {
        try {
            $content = Get-Content $file.FullName -Raw -ErrorAction SilentlyContinue
            if ($content) {
                # Match test attributes: #[test], #[tokio::test], #[rstest], etc.
                $testMatches = [regex]::Matches($content, '#\[(?:[\w:]+::)?test[^\]]*\]')
                $totalTests += $testMatches.Count
            }
        } catch {
            Write-Warning "Could not read file: $($file.FullName)"
        }
    }

    return $totalTests
}

function Test-GeneratedOrVendorPath {
    param(
        [string]$Path
    )

    return $Path -match '\\node_modules\\' -or
        $Path -match '\\dist\\' -or
        $Path -match '\\target\\' -or
        $Path -match '\\runs\\' -or
        $Path -match '\\state\\'
}

function Get-WorkspaceCrates {
    $workspaceCargoToml = Join-Path $projectRoot "Cargo.toml"
    if (-not (Test-Path $workspaceCargoToml)) {
        return @()
    }

    $workspaceContent = Get-Content $workspaceCargoToml -Raw -ErrorAction Stop
    if ($workspaceContent -notmatch '(?s)\[workspace\](.*?)(\r?\n\[|$)') {
        return @()
    }

    $workspaceSection = $matches[1]
    if ($workspaceSection -notmatch '(?s)members\s*=\s*\[(.*?)\]') {
        return @()
    }

    $membersSection = $matches[1]
    $memberMatches = [regex]::Matches($membersSection, '"([^"]+)"')
    $crates = @()

    foreach ($memberMatch in $memberMatches) {
        $relativeMemberPath = $memberMatch.Groups[1].Value.Replace('/', '\')
        
        # Resolve wildcard/glob patterns (like "crates/*") if present
        $resolvedPaths = @()
        if ($relativeMemberPath -like '*[*?]*') {
            $resolvedPaths = Get-ChildItem -Path (Join-Path $projectRoot $relativeMemberPath) -Directory -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName }
        } else {
            $resolvedPaths = @(Join-Path $projectRoot $relativeMemberPath)
        }

        foreach ($cratePath in $resolvedPaths) {
            if (-not (Test-Path $cratePath)) {
                continue
            }

            $crateCargoToml = Join-Path $cratePath "Cargo.toml"
            $crateName = Split-Path $cratePath -Leaf
            if (Test-Path $crateCargoToml) {
                $crateCargoContent = Get-Content $crateCargoToml -Raw -ErrorAction SilentlyContinue
                if ($crateCargoContent -match '(?s)\[package\](.*?)(\r?\n\[|$)') {
                    $packageSection = $matches[1]
                    if ($packageSection -match '(?m)^\s*name\s*=\s*"([^"]+)"') {
                        $crateName = $matches[1]
                    }
                }
            }

            $crates += @{
                Name = $crateName
                CratePath = $cratePath
            }
        }
    }

    return $crates
}

function Get-RustStats {
    $modules = Get-WorkspaceCrates

    $stats = @{}

    foreach ($module in $modules) {
        $modulePath = Join-Path $module.CratePath "src"
        $testsPath = Join-Path $module.CratePath "tests"
        $allFiles = @()

        # Collect source files
        if (Test-Path $modulePath) {
            $allFiles += Get-ChildItem -Path $modulePath -Filter "*.rs" -Recurse -File
        }

        # Also check for integration tests directory at crate level.
        if (Test-Path $testsPath) {
            $allFiles += Get-ChildItem -Path $testsPath -Filter "*.rs" -Recurse -File
        }

        if ($allFiles.Count -gt 0) {
            $lineCount = Count-Lines -Files $allFiles
            $testCount = Count-RustTests -Files $allFiles
            $stats[$module.Name] = @{
                Lines = $lineCount
                Files = $allFiles.Count
                Tests = $testCount
            }
        } else {
            $stats[$module.Name] = @{
                Lines = 0
                Files = 0
                Tests = 0
            }
        }
    }

    return $stats
}

function Get-FrontendStats {
    $stats = @{
        TypeScript = @{Lines = 0; Files = 0}
        Css = @{Lines = 0; Files = 0}
        Html = @{Lines = 0; Files = 0}
    }

    $cratesPath = Join-Path $projectRoot "crates"
    if (-not (Test-Path $cratesPath)) {
        return $stats
    }

    $uiRoots = @(Get-ChildItem -Path $cratesPath -Directory -Recurse -ErrorAction SilentlyContinue | Where-Object {
        $_.Name -eq "ui" -and -not (Test-GeneratedOrVendorPath -Path $_.FullName)
    })

    foreach ($uiRoot in $uiRoots) {
        $tsFiles = @(Get-ChildItem -Path $uiRoot.FullName -Include "*.ts", "*.tsx" -Recurse -File -ErrorAction SilentlyContinue | Where-Object {
            -not (Test-GeneratedOrVendorPath -Path $_.FullName) -and
            $_.Name -notmatch '\.d\.ts$'
        })
        $cssFiles = @(Get-ChildItem -Path $uiRoot.FullName -Filter "*.css" -Recurse -File -ErrorAction SilentlyContinue | Where-Object {
            -not (Test-GeneratedOrVendorPath -Path $_.FullName)
        })
        $htmlFiles = @(Get-ChildItem -Path $uiRoot.FullName -Filter "*.html" -Recurse -File -ErrorAction SilentlyContinue | Where-Object {
            -not (Test-GeneratedOrVendorPath -Path $_.FullName)
        })

        $stats.TypeScript.Lines += Count-Lines -Files $tsFiles
        $stats.TypeScript.Files += $tsFiles.Count
        $stats.Css.Lines += Count-Lines -Files $cssFiles
        $stats.Css.Files += $cssFiles.Count
        $stats.Html.Lines += Count-Lines -Files $htmlFiles
        $stats.Html.Files += $htmlFiles.Count
    }

    return $stats
}

function Get-ConfigDataStats {
    $stats = @{
        Json = @{Lines = 0; Files = 0; Lockfiles = 0}
    }

    $searchRoots = @(
        (Join-Path $projectRoot "crates"),
        (Join-Path $projectRoot "docs\Experiments\Fixtures")
    ) | Where-Object { Test-Path $_ }

    $jsonFiles = @()
    foreach ($searchRoot in $searchRoots) {
        $jsonFiles += Get-ChildItem -Path $searchRoot -Filter "*.json" -Recurse -File -ErrorAction SilentlyContinue | Where-Object {
            -not (Test-GeneratedOrVendorPath -Path $_.FullName)
        }
    }

    $jsonLockFiles = @($jsonFiles | Where-Object { $_.Name -match 'lock' })
    $jsonCountedFiles = @($jsonFiles | Where-Object { $_.Name -notmatch 'lock' })

    $stats.Json.Lines = Count-Lines -Files $jsonCountedFiles
    $stats.Json.Files = $jsonCountedFiles.Count
    $stats.Json.Lockfiles = $jsonLockFiles.Count

    return $stats
}

function Get-DocumentationStats {
    param(
        [string]$RootPath = $projectRoot
    )

    $stats = @{
        Planning = @{Lines = 0; Files = 0}
        Other = @{Lines = 0; Files = 0}
    }

    $docsPath = Join-Path $RootPath "docs"
    if (Test-Path $docsPath) {
        # Get all Markdown files recursively
        $allDocs = @(Get-ChildItem -Path $docsPath -Filter "*.md" -Recurse -File)

        # Planning documents (Plan.*.md, Design.*.md, Idea.*.md)
        $planFiles = @($allDocs | Where-Object {
            $_.Name -match "^(Plan|Design|Idea)\."
        })
        $stats.Planning.Lines = Count-Lines -Files $planFiles
        $stats.Planning.Files = $planFiles.Count

        # Other documentation (everything else in docs/)
        $otherDocs = @($allDocs | Where-Object {
            $_.Name -notmatch "^(Plan|Design|Idea)\."
        })
        $stats.Other.Lines = Count-Lines -Files $otherDocs
        $stats.Other.Files = $otherDocs.Count
    }

    # Include README.md and other top-level docs (e.g. Agents.md, CLAUDE.md)
    $topLevelDocs = @(Get-ChildItem -Path $RootPath -Filter "*.md" -File -ErrorAction SilentlyContinue)
    if ($topLevelDocs) {
        $stats.Other.Lines += Count-Lines -Files $topLevelDocs
        $stats.Other.Files += $topLevelDocs.Count
    }

    return $stats
}

function Get-PowerShellStats {
    $allPsFiles = @()

    # Scripts directory
    $scriptsPath = Join-Path $projectRoot "scripts"
    if (Test-Path $scriptsPath) {
        $allPsFiles += Get-ChildItem -Path $scriptsPath -Filter "*.ps1" -File
    }

    # Root directory scripts
    $rootPsFiles = Get-ChildItem -Path $projectRoot -Filter "*.ps1" -File -ErrorAction SilentlyContinue
    if ($rootPsFiles) {
        $allPsFiles += $rootPsFiles
    }

    return @{
        Lines = Count-Lines -Files $allPsFiles
        Files = $allPsFiles.Count
    }
}

function Get-DependencyCount {
    $cargoTomlPath = Join-Path $projectRoot "Cargo.toml"

    if (-not (Test-Path $cargoTomlPath)) {
        return 0
    }

    $content = Get-Content $cargoTomlPath -Raw

    # Extract [workspace.dependencies] section (for workspace root)
    if ($content -match '(?s)\[workspace\.dependencies\](.*?)(\r?\n\[|$)') {
        $depsSection = $matches[1]

        # Count lines that look like dependencies (exclude comments and empty lines)
        $deps = $depsSection -split "`n" | Where-Object {
            $_ -match '^\s*[a-zA-Z0-9_-]+\s*='
        }

        return $deps.Count
    }

    # Fallback to [dependencies] section for non-workspace crates
    if ($content -match '(?s)\[dependencies\](.*?)(\[|$)') {
        $depsSection = $matches[1]

        # Count lines that look like dependencies (exclude workspace members and comments)
        $deps = $depsSection -split "`n" | Where-Object {
            $_ -match '^\s*[a-zA-Z0-9_-]+\s*=' -and
            $_ -notmatch 'workspace\s*=\s*true'
        }

        return $deps.Count
    }

    return 0
}

function Show-StatisticsReport {
    param(
        [hashtable]$RustStats,
        [hashtable]$FrontendStats,
        [hashtable]$ConfigDataStats,
        [hashtable]$PowerShellStats,
        [hashtable]$DocStats,
        [int]$DependencyCount
    )

    # Calculate totals
    $totalRustLines = ($RustStats.Values | ForEach-Object { $_.Lines } | Measure-Object -Sum).Sum
    $totalRustTests = ($RustStats.Values | ForEach-Object { $_.Tests } | Measure-Object -Sum).Sum
    $totalFrontendLines = ($FrontendStats.Values | ForEach-Object { $_.Lines } | Measure-Object -Sum).Sum
    $totalDocLines = ($DocStats.Values | ForEach-Object { $_.Lines } | Measure-Object -Sum).Sum
    $totalProjectLines = $totalRustLines + $totalFrontendLines + $PowerShellStats.Lines + $totalDocLines

    # Header
    Write-Host ""
    Write-Host "================================================================" -ForegroundColor Cyan
    Write-Host "          PROJECT STATISTICS REPORT                           " -ForegroundColor Cyan
    Write-Host "          qualia-signal-foundry                               " -ForegroundColor Cyan
    Write-Host "================================================================" -ForegroundColor Cyan
    Write-Host ""

    # Rust Source Code Section
    Write-Host "---------------------------------------------------------------" -ForegroundColor Cyan
    Write-Host "  RUST SOURCE CODE (by crate)" -ForegroundColor Cyan
    Write-Host "---------------------------------------------------------------" -ForegroundColor Cyan
    Write-Host ""

    # Sort by line count descending
    $sortedModules = $RustStats.GetEnumerator() | Sort-Object {$_.Value.Lines} -Descending

    foreach ($module in $sortedModules) {
        if ($module.Value.Lines -gt 0) {
            $dots = "." * (30 - $module.Key.Length)
            Write-Host "  $($module.Key) $dots" -NoNewline
            Write-Host (Format-Number $module.Value.Lines -Width 10) -ForegroundColor Green -NoNewline
            Write-Host " lines" -NoNewline
            Write-Host " $(Format-Number $module.Value.Tests -Width 5) unit tests" -ForegroundColor DarkGreen
        }
    }

    Write-Host "  -----------------------------------------" -ForegroundColor DarkGray
    Write-Host "  TOTAL RUST " -NoNewline
    Write-Host (Format-Number $totalRustLines -Width 28) -ForegroundColor Yellow -NoNewline
    Write-Host " lines" -ForegroundColor Yellow -NoNewline
    Write-Host " $(Format-Number $totalRustTests -Width 5) unit tests" -ForegroundColor Yellow
    Write-Host ""

    # Frontend Section
    if ($totalFrontendLines -gt 0) {
        Write-Host "---------------------------------------------------------------" -ForegroundColor Cyan
        Write-Host "  FRONTEND SOURCE" -ForegroundColor Cyan
        Write-Host "---------------------------------------------------------------" -ForegroundColor Cyan
        Write-Host ""

        Write-Host "  TypeScript ..................." -NoNewline
        Write-Host (Format-Number $FrontendStats.TypeScript.Lines -Width 10) -ForegroundColor Green -NoNewline
        Write-Host " lines ($($FrontendStats.TypeScript.Files) files)"

        Write-Host "  CSS .........................." -NoNewline
        Write-Host (Format-Number $FrontendStats.Css.Lines -Width 10) -ForegroundColor Green -NoNewline
        Write-Host " lines ($($FrontendStats.Css.Files) files)"

        Write-Host "  HTML ........................." -NoNewline
        Write-Host (Format-Number $FrontendStats.Html.Lines -Width 10) -ForegroundColor Green -NoNewline
        Write-Host " lines ($($FrontendStats.Html.Files) files)"

        Write-Host "  -----------------------------------------" -ForegroundColor DarkGray
        Write-Host "  TOTAL FRONTEND " -NoNewline
        Write-Host (Format-Number $totalFrontendLines -Width 24) -ForegroundColor Yellow -NoNewline
        Write-Host " lines" -ForegroundColor Yellow
        Write-Host ""
    }

    # Scripts Section
    Write-Host "---------------------------------------------------------------" -ForegroundColor Cyan
    Write-Host "  SCRIPTS" -ForegroundColor Cyan
    Write-Host "---------------------------------------------------------------" -ForegroundColor Cyan
    Write-Host ""

    Write-Host "  PowerShell Scripts ..........." -NoNewline
    Write-Host (Format-Number $PowerShellStats.Lines -Width 10) -ForegroundColor Green -NoNewline
    Write-Host " lines ($($PowerShellStats.Files) files)"
    Write-Host ""

    # Documentation Section
    Write-Host "---------------------------------------------------------------" -ForegroundColor Cyan
    Write-Host "  DOCUMENTATION" -ForegroundColor Cyan
    Write-Host "---------------------------------------------------------------" -ForegroundColor Cyan
    Write-Host ""

    Write-Host "  Planning Documents ..........." -NoNewline
    Write-Host (Format-Number $DocStats.Planning.Lines -Width 10) -ForegroundColor Green -NoNewline
    Write-Host " lines ($($DocStats.Planning.Files) files)"

    Write-Host "  Other Documentation .........." -NoNewline
    Write-Host (Format-Number $DocStats.Other.Lines -Width 10) -ForegroundColor Green -NoNewline
    Write-Host " lines ($($DocStats.Other.Files) files)"

    Write-Host "  -----------------------------------------" -ForegroundColor DarkGray
    Write-Host "  TOTAL DOCUMENTATION " -NoNewline
    Write-Host (Format-Number $totalDocLines -Width 19) -ForegroundColor Yellow -NoNewline
    Write-Host " lines" -ForegroundColor Yellow
    Write-Host ""

    # Dependencies Section
    Write-Host "---------------------------------------------------------------" -ForegroundColor Cyan
    Write-Host "  DEPENDENCIES" -ForegroundColor Cyan
    Write-Host "---------------------------------------------------------------" -ForegroundColor Cyan
    Write-Host ""

    Write-Host "  External Crates .............." -NoNewline
    Write-Host (Format-Number $DependencyCount -Width 10) -ForegroundColor Green -NoNewline
    Write-Host " dependencies"
    Write-Host ""

    # Config and Data Section
    Write-Host "---------------------------------------------------------------" -ForegroundColor Cyan
    Write-Host "  CONFIG AND TEST DATA" -ForegroundColor Cyan
    Write-Host "---------------------------------------------------------------" -ForegroundColor Cyan
    Write-Host ""

    Write-Host "  JSON Config/Data ............." -NoNewline
    Write-Host (Format-Number $ConfigDataStats.Json.Lines -Width 10) -ForegroundColor Green -NoNewline
    Write-Host " lines ($($ConfigDataStats.Json.Files) files)"

    if ($ConfigDataStats.Json.Lockfiles -gt 0) {
        $lockfileLabel = "files"
        if ($ConfigDataStats.Json.Lockfiles -eq 1) {
            $lockfileLabel = "file"
        }

        Write-Host "  JSON Lockfiles ..............." -NoNewline
        Write-Host (Format-Number $ConfigDataStats.Json.Lockfiles -Width 10) -ForegroundColor DarkGreen -NoNewline
        Write-Host " $lockfileLabel excluded from line totals"
    }
    Write-Host ""

    # Summary Section
    Write-Host "---------------------------------------------------------------" -ForegroundColor Cyan
    Write-Host "  SUMMARY" -ForegroundColor Cyan
    Write-Host "---------------------------------------------------------------" -ForegroundColor Cyan
    Write-Host ""

    Write-Host "  Total Project Lines .........." -NoNewline
    Write-Host (Format-Number $totalProjectLines -Width 10) -ForegroundColor Magenta -NoNewline
    Write-Host " lines" -ForegroundColor Magenta

    if ($totalProjectLines -gt 0) {
        $rustPercent = [math]::Round(($totalRustLines / $totalProjectLines) * 100)
    } else {
        $rustPercent = 0
    }
    Write-Host $("  Primary Language ............. Rust ({0}% of codebase)" -f $rustPercent) -ForegroundColor Magenta

    if (($totalRustLines + $totalDocLines) -gt 0) {
        $docPercent = [math]::Round(($totalDocLines / ($totalRustLines + $totalDocLines)) * 100)
    } else {
        $docPercent = 0
    }
    Write-Host $("  Documentation Ratio .......... {0}% (docs vs code)" -f $docPercent) -ForegroundColor Magenta
    Write-Host ""

    Write-Host "---------------------------------------------------------------" -ForegroundColor Cyan
    Write-Host ""

    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    Write-Host "Generated: $timestamp" -ForegroundColor DarkGray
    Write-Host ""
}

function Invoke-ProjectStatsReport {
    try {
        if (-not (Test-Path $projectRoot)) {
            throw "Project root not found: $projectRoot"
        }

        # Collect all statistics
        Write-Host "Collecting project statistics..." -ForegroundColor Yellow

        $rustStats = Get-RustStats
        $frontendStats = Get-FrontendStats
        $configDataStats = Get-ConfigDataStats
        $powerShellStats = Get-PowerShellStats
        $docStats = Get-DocumentationStats
        $dependencyCount = Get-DependencyCount

        # Display formatted report
        Show-StatisticsReport `
            -RustStats $rustStats `
            -FrontendStats $frontendStats `
            -ConfigDataStats $configDataStats `
            -PowerShellStats $powerShellStats `
            -DocStats $docStats `
            -DependencyCount $dependencyCount

    } catch {
        Write-Host $("ERROR: {0}" -f $_) -ForegroundColor Red
        Write-Host $_.ScriptStackTrace -ForegroundColor Red
        exit 1
    }
}

# Run report when invoked directly, but not when dot-sourced for tests.
if ($MyInvocation.InvocationName -ne '.') {
    Invoke-ProjectStatsReport
}
