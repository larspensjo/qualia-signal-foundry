#Requires -Version 7.6

param(
    [Parameter(Position = 0)]
    [string]$Command = "help",

    [Parameter(Position = 1)]
    [string]$Subject = "",

    [string]$Experiment = "",
    [string]$Profile = "",
    [string]$VoiceMemoryFile = "",
    [string]$Store = "state/text-loop/memory-store.json",
    [string]$BindHost = "127.0.0.1",
    [int]$Port = 3939,
    [switch]$Workbench
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$projectRoot = Split-Path -Parent $PSScriptRoot
$defaultStore = "state/text-loop/memory-store.json"
$sampleStore = "crates/qsf_browser_server/tests/fixtures/small-store.json"
$uiDir = Join-Path $projectRoot "crates/qsf_browser_server/ui"
$profilesPath = Join-Path $PSScriptRoot "qsf.profiles.json"
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

function Test-SecretLikeName {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    return $Name -match '(?i)(SECRET|TOKEN|KEY|PASSWORD|AUTH|CREDENTIAL)'
}

function Format-EnvValue {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,

        [AllowEmptyString()]
        [string]$Value
    )

    if (Test-SecretLikeName -Name $Name) {
        return "<redacted>"
    }

    return $Value
}

function Get-ProfileDefinitions {
    if (-not (Test-Path -LiteralPath $profilesPath -PathType Leaf)) {
        Write-Error "Profile file is missing: scripts/qsf.profiles.json"
    }

    $document = Get-Content -Raw -LiteralPath $profilesPath | ConvertFrom-Json
    if ($null -eq $document.profiles -or -not ($document.profiles -is [array])) {
        Write-Error "Profile file must contain a 'profiles' array."
    }

    $seen = @{}
    foreach ($profileDefinition in $document.profiles) {
        if ([string]::IsNullOrWhiteSpace($profileDefinition.name)) {
            Write-Error "Every profile must have a non-empty name."
        }
        if ($seen.ContainsKey($profileDefinition.name)) {
            Write-Error "Duplicate profile name '$($profileDefinition.name)' in scripts/qsf.profiles.json."
        }
        $seen[$profileDefinition.name] = $true
        if ([string]::IsNullOrWhiteSpace($profileDefinition.description)) {
            Write-Error "Profile '$($profileDefinition.name)' must have a description."
        }
        if ($null -eq $profileDefinition.env) {
            Write-Error "Profile '$($profileDefinition.name)' must have an env object."
        }
    }

    return $document.profiles
}

function Get-ProfileDefinition {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    $profiles = Get-ProfileDefinitions
    $matched = @($profiles | Where-Object { $_.name -eq $Name })
    if ($matched.Count -eq 0) {
        $validNames = ($profiles | ForEach-Object { $_.name }) -join ", "
        Write-Error "Unknown profile '$Name'. Valid profiles: $validNames"
    }

    return $matched[0]
}

function Get-PropertyNames {
    param(
        [object]$Object
    )

    if ($null -eq $Object) {
        return @()
    }

    return @($Object.PSObject.Properties | ForEach-Object { $_.Name })
}

function Get-StringList {
    param(
        [object]$Value
    )

    if ($null -eq $Value) {
        return @()
    }

    return @($Value | ForEach-Object { [string]$_ })
}

function Get-ProfileEnvironmentDelta {
    $envSets = [ordered]@{}
    $clearEnv = @()

    if (-not [string]::IsNullOrWhiteSpace($Profile)) {
        $profileDefinition = Get-ProfileDefinition -Name $Profile
        foreach ($required in @($profileDefinition.requires)) {
            if ($null -eq $required) {
                continue
            }
            if ($required.kind -ne "env") {
                Write-Error "Profile '$Profile' has unsupported requirement kind '$($required.kind)'."
            }
            $requiredValue = [System.Environment]::GetEnvironmentVariable($required.name, "Process")
            if ([string]::IsNullOrEmpty($requiredValue)) {
                Write-Error "Profile '$Profile' requires environment variable '$($required.name)' to be set before launch."
            }
        }

        foreach ($name in Get-PropertyNames -Object $profileDefinition.env) {
            $envSets[$name] = [string]$profileDefinition.env.$name
        }
        $clearEnv = @(Get-StringList -Value $profileDefinition.clear_env)
    }

    if (-not [string]::IsNullOrWhiteSpace($VoiceMemoryFile)) {
        if ([string]::IsNullOrWhiteSpace($Experiment)) {
            Write-Error "-VoiceMemoryFile is only meaningful with app -Experiment."
        }
        $envSets["QSF_VOICE_MEMORY_FILE"] = $VoiceMemoryFile
    }

    if (-not [string]::IsNullOrWhiteSpace($Profile) -and $Profile -eq "file-memory" -and [string]::IsNullOrWhiteSpace($VoiceMemoryFile)) {
        Write-Error "Profile 'file-memory' requires -VoiceMemoryFile <path>."
    }

    return [pscustomobject]@{
        Sets = $envSets
        Clears = $clearEnv
    }
}

function Show-EnvironmentDelta {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Delta
    )

    if ($Delta.Sets.Count -eq 0 -and $Delta.Clears.Count -eq 0) {
        Write-Host "Environment: inherited unchanged"
        return
    }

    if ($Delta.Sets.Count -gt 0) {
        Write-Host "Environment set for child process:"
        foreach ($name in $Delta.Sets.Keys) {
            Write-Host "  $name=$(Format-EnvValue -Name $name -Value $Delta.Sets[$name])"
        }
    }

    if ($Delta.Clears.Count -gt 0) {
        Write-Host "Environment cleared for child process:"
        foreach ($name in $Delta.Clears) {
            Write-Host "  $name"
        }
    }
}

function Invoke-WithEnvironmentDelta {
    param(
        [Parameter(Mandatory = $true)]
        [object]$Delta,

        [Parameter(Mandatory = $true)]
        [scriptblock]$ScriptBlock
    )

    Show-EnvironmentDelta -Delta $Delta

    $names = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
    foreach ($name in $Delta.Sets.Keys) {
        [void]$names.Add($name)
    }
    foreach ($name in $Delta.Clears) {
        [void]$names.Add($name)
    }

    $previousValues = @{}
    foreach ($name in $names) {
        $previousValues[$name] = [System.Environment]::GetEnvironmentVariable($name, "Process")
    }

    try {
        foreach ($name in $Delta.Clears) {
            [System.Environment]::SetEnvironmentVariable($name, $null, "Process")
        }
        foreach ($name in $Delta.Sets.Keys) {
            [System.Environment]::SetEnvironmentVariable($name, $Delta.Sets[$name], "Process")
        }
        & $ScriptBlock
    }
    finally {
        foreach ($name in $names) {
            [System.Environment]::SetEnvironmentVariable($name, $previousValues[$name], "Process")
        }
    }
}

function Show-Profiles {
    $profiles = Get-ProfileDefinitions
    foreach ($profileDefinition in $profiles) {
        Write-Host "$($profileDefinition.name)"
        Write-Host "  $($profileDefinition.description)"

        $envNames = @(Get-PropertyNames -Object $profileDefinition.env)
        if ($envNames.Count -gt 0) {
            Write-Host "  Sets:"
            foreach ($name in $envNames) {
                Write-Host "    $name=$(Format-EnvValue -Name $name -Value ([string]$profileDefinition.env.$name))"
            }
        }
        else {
            Write-Host "  Sets: none"
        }

        $clearEnv = @(Get-StringList -Value $profileDefinition.clear_env)
        if ($clearEnv.Count -gt 0) {
            Write-Host "  Clears: $($clearEnv -join ', ')"
        }
        else {
            Write-Host "  Clears: none"
        }

        $requirements = @($profileDefinition.requires)
        if ($requirements.Count -gt 0 -and $null -ne $requirements[0]) {
            Write-Host "  Requires:"
            foreach ($required in $requirements) {
                if ($required.kind -eq "env") {
                    Write-Host "    env:$($required.name)"
                }
                else {
                    Write-Host "    $($required.kind):$($required.name)"
                }
            }
        }
        else {
            Write-Host "  Requires: none"
        }
        Write-Host ""
    }
}

function Show-Help {
    Write-Host @"
QSF launcher

Usage:
  .\scripts\qsf.ps1 help
  .\scripts\qsf.ps1 app [-Experiment <name>] [-Profile <name>] [-VoiceMemoryFile <path>]
  .\scripts\qsf.ps1 browser [-Store <path>] [-BindHost <ip>] [-Port <port>]
  .\scripts\qsf.ps1 ui
  .\scripts\qsf.ps1 workbench [-Store <path>] [-BindHost <ip>] [-Port <port>]
  .\scripts\qsf.ps1 doctor [-Profile <name>] [-Workbench]
  .\scripts\qsf.ps1 list experiments
  .\scripts\qsf.ps1 list profiles

Defaults:
  Browser store: $defaultStore
  Browser host:  127.0.0.1
  Browser port:  3939
  UI directory:  crates/qsf_browser_server/ui

Examples:
  .\scripts\qsf.ps1 app -Experiment multi-turn-text-loop
  .\scripts\qsf.ps1 app -Experiment multi-turn-text-loop -Profile mock
  .\scripts\qsf.ps1 app -Experiment text-owned-voice-loop -Profile file-memory -VoiceMemoryFile docs/Experiments/Fixtures/voice-memory.example.json
  .\scripts\qsf.ps1 browser -Store $sampleStore -BindHost 127.0.0.1 -Port 3939
  .\scripts\qsf.ps1 ui
  .\scripts\qsf.ps1 workbench
  .\scripts\qsf.ps1 doctor -Workbench
  .\scripts\qsf.ps1 list experiments
  .\scripts\qsf.ps1 list profiles
"@
}

function New-DoctorCheck {
    param(
        [Parameter(Mandatory = $true)]
        [ValidateSet("ok", "warn", "fail")]
        [string]$Status,

        [Parameter(Mandatory = $true)]
        [string]$Name,

        [Parameter(Mandatory = $true)]
        [string]$Message
    )

    [pscustomobject]@{
        Status = $Status
        Name = $Name
        Message = $Message
    }
}

function Test-CommandAvailable {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    return $null -ne (Get-Command $Name -ErrorAction SilentlyContinue)
}

function Get-CommandVersionText {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Executable,

        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    try {
        $output = & $Executable @Arguments 2>$null
        if ($LASTEXITCODE -ne 0 -or $null -eq $output) {
            return ""
        }
        return [string]($output | Select-Object -First 1)
    }
    catch {
        return ""
    }
}

function Test-PortOccupied {
    param(
        [Parameter(Mandatory = $true)]
        [string]$HostName,

        [Parameter(Mandatory = $true)]
        [int]$PortNumber
    )

    $client = [System.Net.Sockets.TcpClient]::new()
    try {
        $connectTask = $client.ConnectAsync($HostName, $PortNumber)
        if (-not $connectTask.Wait(250)) {
            return $false
        }
        return $client.Connected
    }
    catch {
        return $false
    }
    finally {
        $client.Dispose()
    }
}

function Add-DoctorCheck {
    param(
        [Parameter(Mandatory = $true)]
        [AllowEmptyCollection()]
        [System.Collections.Generic.List[object]]$Checks,

        [Parameter(Mandatory = $true)]
        [object]$Check
    )

    [void]$Checks.Add($Check)
}

function Invoke-Doctor {
    $checks = [System.Collections.Generic.List[object]]::new()

    $expectedRoot = Split-Path -Parent $PSScriptRoot
    if ((Test-Path -LiteralPath (Join-Path $expectedRoot "Cargo.toml") -PathType Leaf) -and
        (Test-Path -LiteralPath (Join-Path $expectedRoot "crates") -PathType Container)) {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "ok" -Name "Repository root" -Message $expectedRoot)
    }
    else {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "fail" -Name "Repository root" -Message "Could not identify repository root from scripts directory.")
    }

    if ($PSVersionTable.PSVersion -ge [version]"7.6") {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "ok" -Name "PowerShell" -Message $PSVersionTable.PSVersion.ToString())
    }
    else {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "fail" -Name "PowerShell" -Message "Found $($PSVersionTable.PSVersion); launcher requires 7.6 or newer.")
    }

    if (Test-CommandAvailable -Name "cargo") {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "ok" -Name "cargo" -Message (Get-CommandVersionText -Executable "cargo" -Arguments @("--version")))
    }
    else {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "fail" -Name "cargo" -Message "cargo was not found on PATH.")
    }

    if (Test-CommandAvailable -Name "rustc") {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "ok" -Name "Rust toolchain" -Message (Get-CommandVersionText -Executable "rustc" -Arguments @("--version")))
    }
    else {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "fail" -Name "Rust toolchain" -Message "rustc was not found on PATH.")
    }

    if (Test-CommandAvailable -Name "node") {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "ok" -Name "Node" -Message (Get-CommandVersionText -Executable "node" -Arguments @("--version")))
    }
    elseif ($Workbench) {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "fail" -Name "Node" -Message "node was not found on PATH; workbench requires the Vite UI.")
    }
    else {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "warn" -Name "Node" -Message "node was not found on PATH; only UI/workbench launches need it.")
    }

    if (Test-CommandAvailable -Name "npm") {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "ok" -Name "npm" -Message (Get-CommandVersionText -Executable "npm" -Arguments @("--version")))
    }
    elseif ($Workbench) {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "fail" -Name "npm" -Message "npm was not found on PATH; workbench requires the Vite UI.")
    }
    else {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "warn" -Name "npm" -Message "npm was not found on PATH; only UI/workbench launches need it.")
    }

    $nodeModules = Join-Path $uiDir "node_modules"
    if (Test-Path -LiteralPath $nodeModules -PathType Container) {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "ok" -Name "UI dependencies" -Message "crates/qsf_browser_server/ui/node_modules exists.")
    }
    elseif ($Workbench) {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "fail" -Name "UI dependencies" -Message "Missing. Run: cd crates/qsf_browser_server/ui; npm install")
    }
    else {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "warn" -Name "UI dependencies" -Message "Missing. Run before UI/workbench use: cd crates/qsf_browser_server/ui; npm install")
    }

    $resolvedDefaultStore = Join-Path $projectRoot $defaultStore
    if (Test-Path -LiteralPath $resolvedDefaultStore -PathType Leaf) {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "ok" -Name "Default memory store" -Message $defaultStore)
    }
    elseif ($Workbench) {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "fail" -Name "Default memory store" -Message "Missing: $defaultStore. Try sample store: $sampleStore")
    }
    else {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "warn" -Name "Default memory store" -Message "Missing: $defaultStore. Try sample store: $sampleStore")
    }

    if (Test-PortOccupied -HostName "127.0.0.1" -PortNumber 3939) {
        $status = if ($Workbench) { "fail" } else { "warn" }
        Add-DoctorCheck $checks (New-DoctorCheck -Status $status -Name "Port 3939" -Message "127.0.0.1:3939 appears occupied.")
    }
    else {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "ok" -Name "Port 3939" -Message "127.0.0.1:3939 appears available.")
    }

    if ([string]::IsNullOrEmpty([System.Environment]::GetEnvironmentVariable("OPENAI_API_KEY", "Process"))) {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "warn" -Name "OPENAI_API_KEY" -Message "Not set; only OpenAI-backed profiles need it.")
    }
    else {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "ok" -Name "OPENAI_API_KEY" -Message "Set in process environment; value not shown.")
    }

    if (-not [string]::IsNullOrWhiteSpace($Profile)) {
        try {
            [void](Get-ProfileEnvironmentDelta)
            Add-DoctorCheck $checks (New-DoctorCheck -Status "ok" -Name "Profile '$Profile'" -Message "Prerequisites satisfied.")
        }
        catch {
            Add-DoctorCheck $checks (New-DoctorCheck -Status "fail" -Name "Profile '$Profile'" -Message $_.Exception.Message)
        }
    }

    Write-Host "QSF doctor"
    Write-Host "Project root: $projectRoot"
    if ($Workbench) {
        Write-Host "Mode: workbench prerequisites"
    }
    elseif (-not [string]::IsNullOrWhiteSpace($Profile)) {
        Write-Host "Mode: profile prerequisites"
    }
    else {
        Write-Host "Mode: general prerequisites"
    }
    Write-Host ""

    foreach ($check in $checks) {
        $label = switch ($check.Status) {
            "ok" { "OK" }
            "warn" { "WARN" }
            "fail" { "FAIL" }
        }
        Write-Host ("[{0}] {1}: {2}" -f $label, $check.Name, $check.Message)
    }

    $failures = @($checks | Where-Object { $_.Status -eq "fail" })
    if ($failures.Count -gt 0) {
        $script:QsfExitCode = 1
    }
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
    $delta = Get-ProfileEnvironmentDelta
    if ([string]::IsNullOrWhiteSpace($Experiment)) {
        if (-not [string]::IsNullOrWhiteSpace($Profile) -or -not [string]::IsNullOrWhiteSpace($VoiceMemoryFile)) {
            Write-Error "-Profile and -VoiceMemoryFile require app -Experiment <name>."
        }
        Show-Help
        Write-Host ""
        Write-Host "Available experiments:"
        Invoke-LoggedCommand -Executable "cargo" -Arguments @("run", "-p", "qsf_app", "--", "list-experiments")
        return
    }

    Invoke-WithEnvironmentDelta -Delta $delta -ScriptBlock {
        Invoke-LoggedCommand -Executable "cargo" -Arguments @("run", "-p", "qsf_app", "--", "experiment", $Experiment)
    }
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
    "doctor" {
        Invoke-Doctor
    }
    "list" {
        switch ($Subject.ToLowerInvariant()) {
            "experiments" {
                Invoke-LoggedCommand -Executable "cargo" -Arguments @("run", "-p", "qsf_app", "--", "list-experiments")
            }
            "profiles" {
                Show-Profiles
            }
            default {
                Write-Error "Unknown list target '$Subject'. Supported targets: experiments, profiles"
            }
        }
    }
    default {
        Write-Error "Unknown command '$Command'. Run .\scripts\qsf.ps1 help for usage."
    }
}

exit $script:QsfExitCode
