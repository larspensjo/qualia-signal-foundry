#Requires -Version 7.6

param(
    [Parameter(Position = 0)]
    [string]$Command = "help",

    [Parameter(Position = 1)]
    [string]$Subject = "",

    [string]$Experiment = "",
    [string]$Store = "state/text-loop/memory-store.json",
    [string]$BindHost = "127.0.0.1",
    [int]$Port = 3939
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$projectRoot = Split-Path -Parent $PSScriptRoot
$defaultStore = "state/text-loop/memory-store.json"
$sampleStore = "crates/qsf_browser_server/tests/fixtures/small-store.json"
$uiDir = Join-Path $projectRoot "crates/qsf_browser_server/ui"
$script:QsfExitCode = 0

function Format-Command {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Executable,

        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    $parts = @($Executable)
    foreach ($argument in $Arguments) {
        if ($argument -match '[\s"`$]') {
            $parts += "'$($argument -replace "'", "''")'"
        }
        else {
            $parts += $argument
        }
    }

    return ($parts -join " ")
}

function Invoke-LoggedCommand {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Executable,

        [Parameter(Mandatory = $true)]
        [string[]]$Arguments,

        [string]$WorkingDirectory = $projectRoot
    )

    Write-Host "Working directory: $WorkingDirectory"
    Write-Host "Command: $(Format-Command -Executable $Executable -Arguments $Arguments)"
    Push-Location $WorkingDirectory
    try {
        & $Executable @Arguments
        if ($null -eq $LASTEXITCODE) {
            $script:QsfExitCode = 0
        }
        else {
            $script:QsfExitCode = $LASTEXITCODE
        }
    }
    finally {
        Pop-Location
    }
}

function Show-Help {
    Write-Host @"
QSF launcher

Usage:
  .\scripts\qsf.ps1 help
  .\scripts\qsf.ps1 app [-Experiment <name>]
  .\scripts\qsf.ps1 browser [-Store <path>] [-BindHost <ip>] [-Port <port>]
  .\scripts\qsf.ps1 ui
  .\scripts\qsf.ps1 workbench [-Store <path>] [-BindHost <ip>] [-Port <port>]
  .\scripts\qsf.ps1 list experiments

Defaults:
  Browser store: $defaultStore
  Browser host:  127.0.0.1
  Browser port:  3939
  UI directory:  crates/qsf_browser_server/ui

Examples:
  .\scripts\qsf.ps1 app -Experiment multi-turn-text-loop
  .\scripts\qsf.ps1 browser -Store $sampleStore -BindHost 127.0.0.1 -Port 3939
  .\scripts\qsf.ps1 ui
  .\scripts\qsf.ps1 workbench
  .\scripts\qsf.ps1 list experiments
"@
}

function Test-UiDependencies {
    $nodeModules = Join-Path $uiDir "node_modules"
    if (-not (Test-Path -LiteralPath $nodeModules -PathType Container)) {
        Write-Error "UI dependencies are missing. Run: cd crates/qsf_browser_server/ui; npm install"
    }
}

function Test-BrowserStore {
    param(
        [Parameter(Mandatory = $true)]
        [string]$StorePath
    )

    if ($StorePath -eq $defaultStore) {
        $resolvedDefaultStore = Join-Path $projectRoot $defaultStore
        if (-not (Test-Path -LiteralPath $resolvedDefaultStore -PathType Leaf)) {
            Write-Error "Default browser store is missing: $defaultStore. Try the sample store: $sampleStore"
        }
    }
}

function Invoke-App {
    if ([string]::IsNullOrWhiteSpace($Experiment)) {
        Show-Help
        Write-Host ""
        Write-Host "Available experiments:"
        Invoke-LoggedCommand -Executable "cargo" -Arguments @("run", "-p", "qsf_app", "--", "list-experiments")
        return
    }

    Invoke-LoggedCommand -Executable "cargo" -Arguments @("run", "-p", "qsf_app", "--", "experiment", $Experiment)
}

function Invoke-Browser {
    Test-BrowserStore -StorePath $Store
    Invoke-LoggedCommand -Executable "cargo" -Arguments @(
        "run",
        "-p",
        "qsf_browser_server",
        "--",
        "--store",
        $Store,
        "--host",
        $BindHost,
        "--port",
        $Port.ToString([System.Globalization.CultureInfo]::InvariantCulture)
    )
}

function Invoke-Ui {
    Test-UiDependencies
    Invoke-LoggedCommand -Executable "npm" -Arguments @("run", "dev") -WorkingDirectory $uiDir
}

function Invoke-Workbench {
    Test-BrowserStore -StorePath $Store
    Test-UiDependencies

    $apiUrl = "http://${BindHost}:$Port"
    Write-Host "API: $apiUrl"
    Write-Host "Health: $apiUrl/api/health"
    Write-Host "UI: http://127.0.0.1:5173"

    $psExe = (Get-Command "pwsh" -ErrorAction Stop).Source
    $argumentList = @(
        "-NoExit",
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        $PSCommandPath,
        "ui"
    )
    Write-Host "Starting UI process: $(Format-Command -Executable $psExe -Arguments $argumentList)"
    Start-Process -FilePath $psExe -ArgumentList $argumentList -WorkingDirectory $projectRoot

    Invoke-Browser
}

switch ($Command.ToLowerInvariant()) {
    "help" {
        Show-Help
    }
    "app" {
        Invoke-App
    }
    "browser" {
        Invoke-Browser
    }
    "ui" {
        Invoke-Ui
    }
    "workbench" {
        Invoke-Workbench
    }
    "list" {
        if ($Subject.ToLowerInvariant() -ne "experiments") {
            Write-Error "Unknown list target '$Subject'. Supported target: experiments"
        }
        Invoke-LoggedCommand -Executable "cargo" -Arguments @("run", "-p", "qsf_app", "--", "list-experiments")
    }
    default {
        Write-Error "Unknown command '$Command'. Run .\scripts\qsf.ps1 help for usage."
    }
}

exit $script:QsfExitCode
