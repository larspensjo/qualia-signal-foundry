$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$script:QsfCompletionScriptRoot = $PSScriptRoot
$script:QsfCompletionProjectRoot = Split-Path -Parent $script:QsfCompletionScriptRoot
$script:QsfCompletionProfilesPath = Join-Path $script:QsfCompletionScriptRoot "qsf.profiles.json"
$script:QsfCompletionStorePathCache = $null
$script:QsfCompletionStorePathCacheExpiresAt = [datetime]::MinValue

$script:QsfCompletionCommands = @(
    "app",
    "browser",
    "ui",
    "workbench",
    "doctor",
    "list",
    "help"
)

$script:QsfCompletionListSubjects = @(
    "experiments",
    "profiles"
)

$script:QsfCompletionSessionMemorySources = @(
    "auto",
    "empty",
    "file",
    "fixture"
)

$script:QsfCompletionExperiments = @(
    "framework-skeleton-mvp",
    "audio-preparation-layer",
    "associative-memory-toy-model",
    "context-budget-retrieval-test",
    "model-role-smoke-test",
    "multi-turn-text-loop",
    "realtime-voice-session",
    "accept-reviewed-memory",
    "reviewed-memory-draft",
    "sleep-phase-session-summary",
    "streaming-transcription-mvp",
    "text-owned-voice-loop",
    "tool-as-perception-calculator"
)

function New-QsfCompletionResult {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Value,

        [string]$ToolTip = ""
    )

    if ([string]::IsNullOrWhiteSpace($ToolTip)) {
        $ToolTip = $Value
    }

    [System.Management.Automation.CompletionResult]::new(
        $Value,
        $Value,
        [System.Management.Automation.CompletionResultType]::ParameterValue,
        $ToolTip
    )
}

function Select-QsfCompletionMatches {
    param(
        [Parameter(Mandatory = $true)]
        [string[]]$Values,

        [AllowEmptyString()]
        [string]$WordToComplete
    )

    foreach ($value in $Values) {
        if ($value -like "$WordToComplete*") {
            New-QsfCompletionResult -Value $value
        }
    }
}

function Get-QsfCompletionProfiles {
    try {
        if (-not (Test-Path -LiteralPath $script:QsfCompletionProfilesPath -PathType Leaf)) {
            return @()
        }

        $document = Get-Content -Raw -LiteralPath $script:QsfCompletionProfilesPath | ConvertFrom-Json
        if ($null -eq $document.profiles) {
            return @()
        }

        return @($document.profiles | Where-Object {
            -not [string]::IsNullOrWhiteSpace($_.name)
        } | ForEach-Object {
            [string]$_.name
        })
    }
    catch {
        return @()
    }
}

function Get-QsfCompletionStorePaths {
    if ($null -ne $script:QsfCompletionStorePathCache -and (Get-Date) -lt $script:QsfCompletionStorePathCacheExpiresAt) {
        return $script:QsfCompletionStorePathCache
    }

    $roots = @(
        "state",
        "runs",
        "docs/Experiments/Fixtures",
        "crates/qsf_browser_server/tests/fixtures"
    )

    $paths = [System.Collections.Generic.List[string]]::new()
    foreach ($root in $roots) {
        $fullRoot = Join-Path $script:QsfCompletionProjectRoot $root
        if (-not (Test-Path -LiteralPath $fullRoot -PathType Container)) {
            continue
        }

        Get-ChildItem -LiteralPath $fullRoot -Recurse -Depth 3 -File -Filter "*.json" -ErrorAction SilentlyContinue |
            ForEach-Object {
                $relativePath = [System.IO.Path]::GetRelativePath($script:QsfCompletionProjectRoot, $_.FullName)
                $paths.Add(($relativePath -replace '\\', '/'))
            }
    }

    $script:QsfCompletionStorePathCache = @($paths | Sort-Object -Unique)
    $script:QsfCompletionStorePathCacheExpiresAt = (Get-Date).AddSeconds(15)
    return $script:QsfCompletionStorePathCache
}

function Get-QsfCompletionNativeContext {
    param(
        [Parameter(Mandatory = $true)]
        [System.Management.Automation.Language.CommandAst]$CommandAst,

        [AllowEmptyString()]
        [string]$WordToComplete
    )

    $elements = @($CommandAst.CommandElements | ForEach-Object { $_.Extent.Text })
    $argumentTexts = @()
    if ($elements.Count -gt 1) {
        $argumentTexts = @($elements[1..($elements.Count - 1)])
    }

    $isCompletingExistingWord = -not [string]::IsNullOrEmpty($WordToComplete) -and
        $argumentTexts.Count -gt 0 -and
        $argumentTexts[$argumentTexts.Count - 1] -eq $WordToComplete

    $previousArgument = ""
    $effectiveArguments = $argumentTexts
    if ($isCompletingExistingWord) {
        if ($argumentTexts.Count -gt 1) {
            $previousArgument = $argumentTexts[$argumentTexts.Count - 2]
            $effectiveArguments = @($argumentTexts[0..($argumentTexts.Count - 2)])
        }
        else {
            $effectiveArguments = @()
        }
    }
    elseif ($argumentTexts.Count -gt 0) {
        $previousArgument = $argumentTexts[$argumentTexts.Count - 1]
    }

    [pscustomobject]@{
        Arguments = @($effectiveArguments)
        PreviousArgument = $previousArgument
    }
}

$qsfCompleter = {
    param(
        [string]$wordToComplete,
        [System.Management.Automation.Language.CommandAst]$commandAst,
        [int]$cursorPosition
    )

    try {
        $nativeContext = Get-QsfCompletionNativeContext -CommandAst $commandAst -WordToComplete $wordToComplete
        switch ($nativeContext.PreviousArgument) {
            "-LaunchProfile" {
                Select-QsfCompletionMatches -Values (Get-QsfCompletionProfiles) -WordToComplete $wordToComplete
                return
            }
            "-Profile" {
                Select-QsfCompletionMatches -Values (Get-QsfCompletionProfiles) -WordToComplete $wordToComplete
                return
            }
            "-Experiment" {
                Select-QsfCompletionMatches -Values $script:QsfCompletionExperiments -WordToComplete $wordToComplete
                return
            }
            "-Store" {
                Select-QsfCompletionMatches -Values (Get-QsfCompletionStorePaths) -WordToComplete $wordToComplete
                return
            }
            "-BindHost" {
                Select-QsfCompletionMatches -Values @("127.0.0.1", "0.0.0.0") -WordToComplete $wordToComplete
                return
            }
            "-SessionMemorySource" {
                Select-QsfCompletionMatches -Values $script:QsfCompletionSessionMemorySources -WordToComplete $wordToComplete
                return
            }
            "-SessionMemoryFile" {
                Select-QsfCompletionMatches -Values (Get-QsfCompletionStorePaths) -WordToComplete $wordToComplete
                return
            }
        }

        if ($nativeContext.Arguments.Count -eq 0) {
            Select-QsfCompletionMatches -Values $script:QsfCompletionCommands -WordToComplete $wordToComplete
            return
        }

        if ($nativeContext.Arguments.Count -eq 1 -and $nativeContext.Arguments[0] -eq "list") {
            Select-QsfCompletionMatches -Values $script:QsfCompletionListSubjects -WordToComplete $wordToComplete
            return
        }

        if ($nativeContext.Arguments.Count -eq 1 -and $nativeContext.Arguments[0] -in @("browser", "workbench")) {
            Select-QsfCompletionMatches -Values (Get-QsfCompletionStorePaths) -WordToComplete $wordToComplete
            return
        }

    }
    catch {
        return
    }
}

$qsfScriptPath = Join-Path $script:QsfCompletionScriptRoot "qsf.ps1"
$qsfCommandNames = @(
    "qsf.ps1",
    ".\scripts\qsf.ps1",
    "./scripts/qsf.ps1",
    "scripts\qsf.ps1",
    "scripts/qsf.ps1",
    $qsfScriptPath,
    (Resolve-Path -LiteralPath $qsfScriptPath -ErrorAction SilentlyContinue).Path
) | Where-Object {
    -not [string]::IsNullOrWhiteSpace($_)
} | Sort-Object -Unique

Register-ArgumentCompleter -CommandName $qsfCommandNames -Native -ScriptBlock $qsfCompleter
