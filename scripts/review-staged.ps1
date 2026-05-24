#Requires -Version 7.0

# ============================================================================
# Staged Code Review via Gemini
# ============================================================================
#
# Usage:
#   .\Scripts\review-staged.ps1 -Plan <path> -Task <description>
#
# Example:
#   .\Scripts\review-staged.ps1 `
#       -Plan docs\Plans\Plan.VoiceLoopUnification.md `
#       -Task "phase 2 dispatcher wiring"
#
# Sends a prompt to the gemini CLI asking it to review the currently staged
# changes against the given plan, and writes the response to
# docs\Plans\Review.<plan-filename>.
# ============================================================================

param(
    [Parameter(Mandatory = $true)]
    [string]$Plan,

    [Parameter(Mandatory = $true)]
    [string]$Task
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$projectRoot = Split-Path -Parent $PSScriptRoot

$planPath = if ([System.IO.Path]::IsPathRooted($Plan)) {
    $Plan
} else {
    Join-Path $projectRoot $Plan
}

if (-not (Test-Path -LiteralPath $planPath -PathType Leaf)) {
    throw "Plan file not found: $planPath"
}

$planLeaf = Split-Path -Leaf $planPath
$planDir = Split-Path -Parent $planPath
$reviewPath = Join-Path $planDir "Review.$planLeaf"

$prompt = "Please see $Plan Help me review the staged changes where $Task was implemented."

Write-Host "Plan:    $planPath"
Write-Host "Task:    $Task"
Write-Host "Output:  $reviewPath"
Write-Host "Running gemini..."

& gemini -m gemini-3.5-flash -p $prompt | Set-Content -LiteralPath $reviewPath -Encoding UTF8

if ($LASTEXITCODE -ne 0) {
    throw "gemini exited with code $LASTEXITCODE"
}

Write-Host "Review written to $reviewPath"
