#Requires -Version 7.6

param(
    [Parameter(Position = 0)]
    [string]$Command = "help",

    [Parameter(Position = 1)]
    [string]$Subject = "",

    [string]$Experiment = "",
    [Alias("Profile")]
    [string]$LaunchProfile = "",
    [string]$VoiceMemoryFile = "",
    [string]$VolitionDraft = "",
    [ValidateSet("auto", "empty", "file", "fixture")]
    [string]$SessionMemorySource = "auto",
    [string]$SessionMemoryFile = "",
    [switch]$DemoMemory,
    [string]$Store = "state/realtime/continuity/default/memory-store.json",
    [string]$StateDir = "state/realtime",
    [ValidateSet("openai", "mock")]
    [string]$Provider = "openai",
    [string]$BindHost = "127.0.0.1",
    [int]$Port = 3939,
    [switch]$Workbench,
    [switch]$RandomSessionId,
    [switch]$All,
    [switch]$Pretty,
    [switch]$Full,
    [string]$Out = "",
    [string]$WorldCorpusPath = "",
    [string]$WorldCorpusLedger = "state/world-corpus/index.json"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$projectRoot = Split-Path -Parent $PSScriptRoot
$defaultStore = "state/realtime/continuity/default/memory-store.json"
$sampleStore = "crates/qsf_browser_server/tests/fixtures/small-store.json"
$emptySessionMemoryFile = "docs/Experiments/Fixtures/session-memory.empty.json"
$backupRootRelative = "state/backups"
$sleepBackupKeepCount = 5
$uiDir = Join-Path $projectRoot "crates/qsf_browser_server/ui"
$realtimeUiDir = Join-Path $projectRoot "crates/qsf_realtime_server/ui"
# The realtime server port must match qsf_realtime_server's cli.rs DEFAULT_PORT and the
# Vite dev server's /api proxy target in crates/qsf_realtime_server/ui/vite.config.ts
# (pinned to 3940), so the realtime command does not expose -Port/-BindHost overrides.
# The Vite UI port is only where the dev server listens; the realtime launcher resolves
# the first free port at or above this preferred value and opens the browser there.
$realtimeServerPort = 3940
$browserUiPort = 5173
$realtimeUiPort = 5174
$realtimeUiUrl = "http://localhost:$realtimeUiPort"
$profilesPath = Join-Path $PSScriptRoot "qsf.profiles.json"
$script:QsfExitCode = 0
$script:QsfScriptBoundParameters = @{} + $PSBoundParameters

# Launcher controls all non-secret QSF_* environment variables to ensure deterministic behavior.
# Tests can use Get-TestEnvironmentDelta to see the effective changes made by the launcher.
$script:QsfKnownManagedEnvironmentVariables = @(
    "QSF_ACCEPT_MEMORY_DRAFT",
    "QSF_CONVERSATION_MODEL",
    "QSF_MODEL_PROVIDER",
    "QSF_OPENAI_REALTIME_TIMEOUT_MS",
    "QSF_REALTIME_SESSION_INPUT_SOURCE",
    "QSF_REALTIME_SESSION_MIC_DEVICE",
    "QSF_REALTIME_SESSION_MIC_DURATION_MS",
    "QSF_REALTIME_SESSION_PROVIDER",
    "QSF_REALTIME_SESSION_WAV_PATH",
    "QSF_REVIEWED_MEMORY_SLEEP_REPORT",
    "QSF_REVIEWED_VOLITION_DRAFT",
    "QSF_SESSION_ALLOW_OVER_LIMIT",
    "QSF_SESSION_MAX_TURNS",
    "QSF_SESSION_MEMORY_FILE",
    "QSF_SESSION_MEMORY_SOURCE",
    "QSF_SESSION_TURN_MAX_OUTPUT_TOKENS",
    "QSF_SESSION_WARM_THRESHOLD",
    "QSF_SPEECH_OUTPUT_MODE",
    "QSF_SPEECH_OUTPUT_MODEL",
    "QSF_SPEECH_OUTPUT_PROVIDER",
    "QSF_SPEECH_OUTPUT_VOICE",
    "QSF_STATE_DIR",
    "QSF_TRANSCRIPT_INPUT_SOURCE",
    "QSF_TRANSCRIPT_MIC_DEVICE",
    "QSF_TRANSCRIPT_MIC_DURATION_MS",
    "QSF_TRANSCRIPT_PROVIDER",
    "QSF_TRANSCRIPT_WAV_PATH",
    "QSF_VOICE_MEMORY_FILE",
    "QSF_VOICE_MEMORY_SOURCE",
    "QSF_WORLD_CORPUS_PATH"
)

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

function Get-ManagedQsfEnvironmentVariableNames {
    $ambientQsfNames = @(
        [System.Environment]::GetEnvironmentVariables("Process").Keys |
        ForEach-Object { [string]$_ } |
        Where-Object { $_ -match '^QSF_' -and -not (Test-SecretLikeName -Name $_) }
    )

    return @($script:QsfKnownManagedEnvironmentVariables + $ambientQsfNames | Sort-Object -Unique)
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
        if ($null -eq $profileDefinition.PSObject.Properties["clear_env"]) {
            Write-Error "Profile '$($profileDefinition.name)' must have a clear_env array; use [] when there is nothing to clear."
        }
        if ($null -eq $profileDefinition.PSObject.Properties["requires"]) {
            Write-Error "Profile '$($profileDefinition.name)' must have a requires array; use [] when there are no requirements."
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
    $clearEnv = @(Get-ManagedQsfEnvironmentVariableNames)

    if (-not [string]::IsNullOrWhiteSpace($LaunchProfile)) {
        $profileDefinition = Get-ProfileDefinition -Name $LaunchProfile
        foreach ($required in @($profileDefinition.requires)) {
            if ($null -eq $required) {
                continue
            }
            if ($required.kind -ne "env") {
                Write-Error "Profile '$LaunchProfile' has unsupported requirement kind '$($required.kind)'."
            }
            $requiredValue = [System.Environment]::GetEnvironmentVariable($required.name, "Process")
            if ([string]::IsNullOrEmpty($requiredValue)) {
                Write-Error "Profile '$LaunchProfile' requires environment variable '$($required.name)' to be set before launch."
            }
        }

        foreach ($name in Get-PropertyNames -Object $profileDefinition.env) {
            $envSets[$name] = [string]$profileDefinition.env.$name
        }
        $clearEnv += @(Get-StringList -Value $profileDefinition.clear_env)
    }

    if (-not [string]::IsNullOrWhiteSpace($VoiceMemoryFile)) {
        if ([string]::IsNullOrWhiteSpace($Experiment)) {
            Write-Error "-VoiceMemoryFile is only meaningful with app -Experiment."
        }
        $envSets["QSF_VOICE_MEMORY_SOURCE"] = "file"
        $envSets["QSF_VOICE_MEMORY_FILE"] = $VoiceMemoryFile
    }

    if (-not [string]::IsNullOrWhiteSpace($VolitionDraft)) {
        if ([string]::IsNullOrWhiteSpace($Experiment)) {
            Write-Error "-VolitionDraft is only meaningful with app -Experiment."
        }
        $envSets["QSF_REVIEWED_VOLITION_DRAFT"] = $VolitionDraft
    }

    if (-not [string]::IsNullOrWhiteSpace($LaunchProfile) -and $LaunchProfile -eq "file-memory" -and [string]::IsNullOrWhiteSpace($VoiceMemoryFile)) {
        Write-Error "Profile 'file-memory' requires -VoiceMemoryFile <path>."
    }

    $sessionSourceProvided = $script:QsfScriptBoundParameters.ContainsKey("SessionMemorySource")
    if ($DemoMemory -and $sessionSourceProvided) {
        Write-Error "Use either -DemoMemory or -SessionMemorySource, not both."
    }
    if ($DemoMemory -and -not [string]::IsNullOrWhiteSpace($SessionMemoryFile)) {
        Write-Error "-DemoMemory cannot be combined with -SessionMemoryFile."
    }
    if (-not [string]::IsNullOrWhiteSpace($SessionMemoryFile) -and [string]::IsNullOrWhiteSpace($Experiment)) {
        Write-Error "-SessionMemoryFile is only meaningful with app -Experiment."
    }

    $effectiveSessionMemorySource = $SessionMemorySource
    if ($DemoMemory) {
        $effectiveSessionMemorySource = "fixture"
    }
    elseif (-not $sessionSourceProvided -and -not [string]::IsNullOrWhiteSpace($SessionMemoryFile)) {
        $effectiveSessionMemorySource = "file"
    }
    elseif (-not $sessionSourceProvided -and
        $Experiment -eq "multi-turn-text-loop" -and
        -not $envSets.Contains("QSF_SESSION_MEMORY_SOURCE")) {
        $effectiveSessionMemorySource = "empty"
    }

    switch ($effectiveSessionMemorySource) {
        "empty" {
            if ([string]::IsNullOrWhiteSpace($Experiment)) {
                Write-Error "-SessionMemorySource empty is only meaningful with app -Experiment."
            }
            if (-not [string]::IsNullOrWhiteSpace($SessionMemoryFile)) {
                Write-Error "-SessionMemoryFile requires -SessionMemorySource file."
            }
            $envSets["QSF_SESSION_MEMORY_SOURCE"] = "file"
            $envSets["QSF_SESSION_MEMORY_FILE"] = $emptySessionMemoryFile
        }
        "file" {
            if ([string]::IsNullOrWhiteSpace($Experiment)) {
                Write-Error "-SessionMemorySource file is only meaningful with app -Experiment."
            }
            if ([string]::IsNullOrWhiteSpace($SessionMemoryFile)) {
                Write-Error "-SessionMemorySource file requires -SessionMemoryFile <path>."
            }
            $envSets["QSF_SESSION_MEMORY_SOURCE"] = "file"
            $envSets["QSF_SESSION_MEMORY_FILE"] = $SessionMemoryFile
        }
        "fixture" {
            if ([string]::IsNullOrWhiteSpace($Experiment)) {
                Write-Error "-SessionMemorySource fixture is only meaningful with app -Experiment."
            }
            if (-not [string]::IsNullOrWhiteSpace($SessionMemoryFile)) {
                Write-Error "-SessionMemoryFile requires -SessionMemorySource file."
            }
            $envSets["QSF_SESSION_MEMORY_SOURCE"] = "phase_four_fixture"
            $clearEnv += "QSF_SESSION_MEMORY_FILE"
        }
        "auto" {
            if (-not [string]::IsNullOrWhiteSpace($SessionMemoryFile)) {
                Write-Error "-SessionMemoryFile requires -SessionMemorySource file."
            }
        }
    }

    if ($Experiment -eq "multi-turn-text-loop") {
        $envSets["QSF_SESSION_ALLOW_OVER_LIMIT"] = "true"
    }

    $clearEnv = @($clearEnv | Where-Object { -not $envSets.Contains($_) } | Sort-Object -Unique)

    return [pscustomobject]@{
        Sets   = $envSets
        Clears = $clearEnv
    }
}

function Get-RealtimeEnvironmentDelta {
    # The realtime server is OpenAI-backed (the command already requires OPENAI_API_KEY), so
    # the launcher pins QSF_MODEL_PROVIDER instead of letting sideband model roles (e.g. the
    # live goal-formation judge) silently fall back to the mock client when it is unset.
    $envSets = [ordered]@{
        "QSF_MODEL_PROVIDER" = "openai"
    }
    if (-not [string]::IsNullOrWhiteSpace($WorldCorpusPath)) {
        $envSets["QSF_WORLD_CORPUS_PATH"] = $WorldCorpusPath
    }
    $clearEnv = @(
        Get-ManagedQsfEnvironmentVariableNames |
        Where-Object { -not $envSets.Contains($_) } |
        Sort-Object -Unique
    )

    return [pscustomobject]@{
        Sets   = $envSets
        Clears = $clearEnv
    }
}

function Get-SleepEnvironmentDelta {
    $envSets = [ordered]@{
        "QSF_MODEL_PROVIDER" = $Provider
    }
    if (-not [string]::IsNullOrWhiteSpace($WorldCorpusPath)) {
        $envSets["QSF_WORLD_CORPUS_PATH"] = $WorldCorpusPath
    }
    $clearEnv = @(
        Get-ManagedQsfEnvironmentVariableNames |
        Where-Object { -not $envSets.Contains($_) } |
        Sort-Object -Unique
    )

    return [pscustomobject]@{
        Sets   = $envSets
        Clears = $clearEnv
    }
}

function Get-WorldCorpusIngestEnvironmentDelta {
    $envSets = [ordered]@{}
    if (-not [string]::IsNullOrWhiteSpace($WorldCorpusPath)) {
        $envSets["QSF_WORLD_CORPUS_PATH"] = $WorldCorpusPath
    }
    $clearEnv = @(
        Get-ManagedQsfEnvironmentVariableNames |
        Where-Object { -not $envSets.Contains($_) } |
        Sort-Object -Unique
    )

    return [pscustomobject]@{
        Sets   = $envSets
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
  .\scripts\qsf.ps1 app [-Experiment <name>] [-LaunchProfile <name>] [-VoiceMemoryFile <path>] [-VolitionDraft <path>] [-SessionMemorySource <auto|empty|file|fixture>] [-SessionMemoryFile <path>] [-DemoMemory]
  .\scripts\qsf.ps1 browser [<store>] [-Store <path>] [-BindHost <ip>] [-Port <port>]
  .\scripts\qsf.ps1 ui [browser|realtime]
  .\scripts\qsf.ps1 workbench [<store>] [-Store <path>] [-BindHost <ip>] [-Port <port>]
  .\scripts\qsf.ps1 realtime [-RandomSessionId] [-WorldCorpusPath <path>]
  .\scripts\qsf.ps1 sleep [-StateDir <path>] [-Provider <openai|mock>] [-WorldCorpusPath <path>] [-WorldCorpusLedger <path>]
  .\scripts\qsf.ps1 goals [<session-id>] [-StateDir <path>]
  .\scripts\qsf.ps1 transcript [<session-id>] [-StateDir <path>] [-All] [-Pretty] [-Full] [-Out <path>]
  .\scripts\qsf.ps1 world-ingest [-WorldCorpusPath <path>] [-WorldCorpusLedger <path>]
  .\scripts\qsf.ps1 restore [<backup-name>|latest] [-StateDir <path>]
  .\scripts\qsf.ps1 doctor [-LaunchProfile <name>] [-Workbench]
  .\scripts\qsf.ps1 list experiments
  .\scripts\qsf.ps1 list profiles

Defaults:
  Browser store: $defaultStore
  Browser API host:  127.0.0.1
  Browser API port:  3939
  Browser UI port:   first free port >= $browserUiPort
  App workspace root: $projectRoot
    App environment: clears non-secret QSF_* values before applying launcher settings
  Text-loop session memory through launcher: empty file source; persisted store wins
  Text-loop session limit through launcher: allow over limit
  UI directory:  crates/qsf_browser_server/ui
  Realtime server: 127.0.0.1:$realtimeServerPort (state/realtime); requires OPENAI_API_KEY
    Realtime environment: sets QSF_MODEL_PROVIDER=openai, optionally sets QSF_WORLD_CORPUS_PATH,
                          and clears other non-secret QSF_* values
  Browser UI:      crates/qsf_browser_server/ui (Vite on first free port >= $browserUiPort)
  Realtime UI:     crates/qsf_realtime_server/ui (Vite on $realtimeUiUrl)
  Sleep update:    state/realtime through the $Provider provider; openai requires OPENAI_API_KEY
                   backs up the state dir to state/backups/<name>-<timestamp> first (keeps last 5)
  Goals:           prints full read-only volition goal detail from state/realtime; an optional session id bypasses auto-selection
  Transcript:      prints the newest run in state/realtime/diagnostics as JSONL, one line per turn with its
                   volition traces; an optional session id bypasses ledger auto-selection; -All emits every run
  Restore:         creates undo backups as state/backups/<name>-restore-<timestamp>; latest ignores those undo backups

Examples:
  .\scripts\qsf.ps1 app -Experiment multi-turn-text-loop
  .\scripts\qsf.ps1 app -Experiment multi-turn-text-loop -DemoMemory
  .\scripts\qsf.ps1 app -Experiment multi-turn-text-loop -SessionMemorySource file -SessionMemoryFile docs/Experiments/Fixtures/session-memory.empty.json
  .\scripts\qsf.ps1 app -Experiment multi-turn-text-loop -LaunchProfile mock
  .\scripts\qsf.ps1 app -Experiment text-owned-voice-loop -LaunchProfile file-memory -VoiceMemoryFile docs/Experiments/Fixtures/voice-memory.example.json
  .\scripts\qsf.ps1 app -Experiment accept-reviewed-volition-seed -LaunchProfile realtime-state -VolitionDraft docs/Experiments/Fixtures/volition-seed.reviewed.draft.json
  .\scripts\qsf.ps1 browser -Store $sampleStore -BindHost 127.0.0.1 -Port 3939
  .\scripts\qsf.ps1 ui
  .\scripts\qsf.ps1 ui realtime
  .\scripts\qsf.ps1 realtime
  .\scripts\qsf.ps1 realtime -RandomSessionId
  .\scripts\qsf.ps1 realtime -WorldCorpusPath C:\data\web_page_filet_mignon\output
  .\scripts\qsf.ps1 sleep
  .\scripts\qsf.ps1 sleep -Provider mock
  .\scripts\qsf.ps1 goals
  .\scripts\qsf.ps1 goals 01JSESSION
  .\scripts\qsf.ps1 world-ingest
  .\scripts\qsf.ps1 world-ingest -WorldCorpusPath C:\data\web_page_filet_mignon\output
  .\scripts\qsf.ps1 restore
  .\scripts\qsf.ps1 restore latest
  .\scripts\qsf.ps1 workbench $sampleStore
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
        Status  = $Status
        Name    = $Name
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

function Test-PortAvailable {
    param(
        [Parameter(Mandatory = $true)]
        [int]$PortNumber
    )

    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, $PortNumber)
    try {
        $listener.Start()
        return $true
    }
    catch {
        return $false
    }
    finally {
        $listener.Stop()
    }
}

function Get-AvailablePort {
    param(
        [Parameter(Mandatory = $true)]
        [int]$PreferredPort
    )

    for ($candidate = $PreferredPort; $candidate -le 65535; $candidate++) {
        if (Test-PortAvailable -PortNumber $candidate) {
            return $candidate
        }
    }

    Write-Error "No available port was found starting at $PreferredPort."
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

    $realtimeNodeModules = Join-Path $realtimeUiDir "node_modules"
    if (Test-Path -LiteralPath $realtimeNodeModules -PathType Container) {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "ok" -Name "Realtime UI dependencies" -Message "crates/qsf_realtime_server/ui/node_modules exists.")
    }
    else {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "warn" -Name "Realtime UI dependencies" -Message "Missing. Run before realtime use: cd crates/qsf_realtime_server/ui; npm install")
    }

    $resolvedDefaultStore = Join-Path $projectRoot $defaultStore
    if (Test-Path -LiteralPath $resolvedDefaultStore -PathType Leaf) {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "ok" -Name "Default memory store" -Message $defaultStore)
    }
    elseif ($Workbench) {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "fail" -Name "Default memory store" -Message "Missing: $defaultStore. Run realtime then sleep, or try sample store: $sampleStore")
    }
    else {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "warn" -Name "Default memory store" -Message "Missing: $defaultStore. Run realtime then sleep, or try sample store: $sampleStore")
    }

    if (Test-PortOccupied -HostName "127.0.0.1" -PortNumber 3939) {
        $status = if ($Workbench) { "fail" } else { "warn" }
        Add-DoctorCheck $checks (New-DoctorCheck -Status $status -Name "Port 3939" -Message "127.0.0.1:3939 appears occupied.")
    }
    else {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "ok" -Name "Port 3939" -Message "127.0.0.1:3939 appears available.")
    }

    if (Test-PortOccupied -HostName "127.0.0.1" -PortNumber $realtimeServerPort) {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "warn" -Name "Port $realtimeServerPort" -Message "127.0.0.1:$realtimeServerPort appears occupied; the realtime server uses it.")
    }
    else {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "ok" -Name "Port $realtimeServerPort" -Message "127.0.0.1:$realtimeServerPort appears available.")
    }

    if ([string]::IsNullOrEmpty([System.Environment]::GetEnvironmentVariable("OPENAI_API_KEY", "Process"))) {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "warn" -Name "OPENAI_API_KEY" -Message "Not set; only OpenAI-backed profiles need it.")
    }
    else {
        Add-DoctorCheck $checks (New-DoctorCheck -Status "ok" -Name "OPENAI_API_KEY" -Message "Set in process environment; value not shown.")
    }

    if (-not [string]::IsNullOrWhiteSpace($LaunchProfile)) {
        try {
            [void](Get-ProfileEnvironmentDelta)
            Add-DoctorCheck $checks (New-DoctorCheck -Status "ok" -Name "Profile '$LaunchProfile'" -Message "Prerequisites satisfied.")
        }
        catch {
            Add-DoctorCheck $checks (New-DoctorCheck -Status "fail" -Name "Profile '$LaunchProfile'" -Message $_.Exception.Message)
        }
    }

    Write-Host "QSF doctor"
    Write-Host "Project root: $projectRoot"
    if ($Workbench) {
        Write-Host "Mode: workbench prerequisites"
    }
    elseif (-not [string]::IsNullOrWhiteSpace($LaunchProfile)) {
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
    param(
        [string]$Dir = $uiDir
    )

    $nodeModules = Join-Path $Dir "node_modules"
    if (-not (Test-Path -LiteralPath $nodeModules -PathType Container)) {
        $relativeDir = ([System.IO.Path]::GetRelativePath($projectRoot, $Dir)) -replace '\\', '/'
        Write-Error "UI dependencies are missing. Run: cd $relativeDir; npm install"
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
            Write-Error "Default browser store is missing: $defaultStore. Run .\scripts\qsf.ps1 realtime followed by .\scripts\qsf.ps1 sleep, or try the sample store: $sampleStore"
        }
    }
}

function Get-BrowserStoreArgument {
    if ([string]::IsNullOrWhiteSpace($Subject)) {
        return $Store
    }

    if ($script:QsfScriptBoundParameters.ContainsKey("Store")) {
        Write-Error "Specify the browser store either positionally or with -Store, not both."
    }

    return $Subject
}

function Invoke-App {
    $delta = Get-ProfileEnvironmentDelta
    if ([string]::IsNullOrWhiteSpace($Experiment)) {
        if (-not [string]::IsNullOrWhiteSpace($LaunchProfile) -or -not [string]::IsNullOrWhiteSpace($VoiceMemoryFile)) {
            Write-Error "-LaunchProfile and -VoiceMemoryFile require app -Experiment <name>."
        }
        Show-Help
        Write-Host ""
        Write-Host "Available experiments:"
        Invoke-LoggedCommand -Executable "cargo" -Arguments @("run", "-p", "qsf_app", "--", "list-experiments")
        return
    }

    Invoke-WithEnvironmentDelta -Delta $delta -ScriptBlock {
        Invoke-LoggedCommand -Executable "cargo" -Arguments @(
            "run",
            "-p",
            "qsf_app",
            "--",
            "experiment",
            $Experiment,
            "--workspace-root",
            $projectRoot
        )
    }
}

function Invoke-Browser {
    $storePath = Get-BrowserStoreArgument
    Test-BrowserStore -StorePath $storePath
    Invoke-BrowserWorkbench -StorePath $storePath
}

function Invoke-BrowserServer {
    param(
        [Parameter(Mandatory = $true)]
        [string]$StorePath
    )

    Invoke-LoggedCommand -Executable "cargo" -Arguments @(
        "run",
        "-p",
        "qsf_browser_server",
        "--",
        "--store",
        $StorePath,
        "--host",
        $BindHost,
        "--port",
        $Port.ToString([System.Globalization.CultureInfo]::InvariantCulture)
    )
}

function Get-UiTarget {
    param(
        [AllowEmptyString()]
        [string]$Target
    )

    switch ($Target.ToLowerInvariant()) {
        "" { [pscustomobject]@{ Name = "browser"; Dir = $uiDir } }
        "browser" { [pscustomobject]@{ Name = "browser"; Dir = $uiDir } }
        "realtime" { [pscustomobject]@{ Name = "realtime"; Dir = $realtimeUiDir } }
        default { Write-Error "Unknown ui target '$Target'. Supported targets: browser, realtime" }
    }
}

function Invoke-Ui {
    $target = Get-UiTarget -Target $Subject
    Test-UiDependencies -Dir $target.Dir
    Invoke-LoggedCommand -Executable "npm" -Arguments @("run", "dev") -WorkingDirectory $target.Dir
}

function Start-UiDevProcess {
    param(
        [Parameter(Mandatory = $true)]
        [ValidateSet("browser", "realtime")]
        [string]$Target
    )

    $psExe = (Get-Command "pwsh" -ErrorAction Stop).Source
    $argumentList = @(
        "-NoExit",
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-File",
        $PSCommandPath,
        "ui",
        $Target
    )
    Write-Host "Starting UI process: $(Format-Command -Executable $psExe -Arguments $argumentList)"
    $uiProcess = Start-Process -FilePath $psExe -ArgumentList $argumentList -WorkingDirectory $projectRoot -PassThru
    Write-Host "UI process PID: $($uiProcess.Id)"
    return $uiProcess
}

function Stop-SpawnedProcess {
    param(
        [object]$Process
    )

    if ($null -ne $Process -and -not $Process.HasExited) {
        Write-Host "Stopping UI process PID $($Process.Id)"
        if (-not $Process.CloseMainWindow()) {
            Stop-Process -Id $Process.Id -ErrorAction SilentlyContinue
        }
    }
}

function Stop-ProcessTree {
    param(
        [object]$Process
    )

    # taskkill /T terminates the whole tree; a bare Stop-Process would leave npm's
    # child node/Vite (or cargo's child server binary) running and holding the port.
    if ($null -ne $Process -and -not $Process.HasExited) {
        Write-Host "Stopping process tree PID $($Process.Id)"
        & taskkill.exe /PID $Process.Id /T /F | Out-Null
    }
}

function Get-NpmCommandPath {
    $npmCmd = Get-Command "npm.cmd" -ErrorAction SilentlyContinue
    if ($null -ne $npmCmd) {
        return $npmCmd.Source
    }

    $npm = Get-Command "npm" -ErrorAction SilentlyContinue
    if ($null -ne $npm) {
        return $npm.Source
    }

    Write-Error "npm was not found on PATH."
}

function Get-LocalProbeHost {
    param(
        [Parameter(Mandatory = $true)]
        [string]$HostName
    )

    if ($HostName -eq "0.0.0.0" -or $HostName -eq "::") {
        return "127.0.0.1"
    }

    return $HostName
}

function Start-BrowserServerProcess {
    param(
        [Parameter(Mandatory = $true)]
        [string]$StorePath
    )

    $cargo = (Get-Command "cargo" -ErrorAction Stop).Source
    $arguments = @(
        "run",
        "-p",
        "qsf_browser_server",
        "--",
        "--store",
        $StorePath,
        "--host",
        $BindHost,
        "--port",
        $Port.ToString([System.Globalization.CultureInfo]::InvariantCulture)
    )
    Write-Host "Starting browser API server: $(Format-Command -Executable $cargo -Arguments $arguments)"
    $serverProcess = Start-Process -FilePath $cargo -ArgumentList $arguments -WorkingDirectory $projectRoot -NoNewWindow -PassThru
    Write-Host "Browser API server PID: $($serverProcess.Id)"
    return $serverProcess
}

function Start-BrowserUiProcess {
    param(
        [Parameter(Mandatory = $true)]
        [int]$PortNumber,

        [Parameter(Mandatory = $true)]
        [string]$ApiUrl
    )

    $npm = Get-NpmCommandPath
    $arguments = @(
        "run",
        "dev",
        "--",
        "--host",
        "127.0.0.1",
        "--port",
        $PortNumber.ToString([System.Globalization.CultureInfo]::InvariantCulture),
        "--strictPort"
    )
    Write-Host "Starting browser Vite UI: $(Format-Command -Executable $npm -Arguments $arguments)"

    $previousApiUrl = [System.Environment]::GetEnvironmentVariable("QSF_BROWSER_API_URL", "Process")
    try {
        [System.Environment]::SetEnvironmentVariable("QSF_BROWSER_API_URL", $ApiUrl, "Process")
        $uiProcess = Start-Process -FilePath $npm -ArgumentList $arguments -WorkingDirectory $uiDir -WindowStyle Hidden -PassThru
        Write-Host "Browser Vite UI PID: $($uiProcess.Id)"
        return $uiProcess
    }
    finally {
        [System.Environment]::SetEnvironmentVariable("QSF_BROWSER_API_URL", $previousApiUrl, "Process")
    }
}

function Invoke-BrowserWorkbench {
    param(
        [Parameter(Mandatory = $true)]
        [string]$StorePath
    )

    Test-UiDependencies

    $apiUrl = "http://${BindHost}:$Port"
    $apiProbeHost = Get-LocalProbeHost -HostName $BindHost
    $uiPort = Get-AvailablePort -PreferredPort $browserUiPort
    $uiUrl = "http://localhost:$uiPort"

    Write-Host "Browser API: $apiUrl"
    Write-Host "Browser API health: $apiUrl/api/health"
    Write-Host "Browser UI: $uiUrl"
    if ($uiPort -ne $browserUiPort) {
        Write-Host "Preferred browser UI port $browserUiPort was busy; using $uiPort instead."
    }
    Write-Host "Press Ctrl+C to stop both the browser API and the Vite dev server."

    $serverProcess = $null
    $uiProcess = $null
    $browserOpened = $false
    try {
        $serverProcess = Start-BrowserServerProcess -StorePath $StorePath
        $uiProcess = Start-BrowserUiProcess -PortNumber $uiPort -ApiUrl $apiUrl

        while ($true) {
            if ($serverProcess.HasExited) {
                Write-Host "Browser API server exited with code $($serverProcess.ExitCode) before shutdown was requested; see its log above."
                $script:QsfExitCode = $serverProcess.ExitCode
                break
            }
            if ($uiProcess.HasExited) {
                Write-Host "Browser Vite UI exited with code $($uiProcess.ExitCode) before shutdown was requested."
                $script:QsfExitCode = $uiProcess.ExitCode
                break
            }
            if (-not $browserOpened -and
                (Test-PortOccupied -HostName $apiProbeHost -PortNumber $Port) -and
                (Test-PortOccupied -HostName "127.0.0.1" -PortNumber $uiPort)) {
                Write-Host "Both browser servers are up. Opening browser: $uiUrl"
                Start-Process $uiUrl
                $browserOpened = $true
            }
            Start-Sleep -Milliseconds 500
        }
    }
    finally {
        Write-Host "Stopping browser processes..."
        Stop-ProcessTree -Process $uiProcess
        Stop-ProcessTree -Process $serverProcess
    }
}

function Invoke-Workbench {
    $storePath = Get-BrowserStoreArgument
    Test-BrowserStore -StorePath $storePath
    Invoke-BrowserWorkbench -StorePath $storePath
}

function Test-RequiredSecret {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    $value = [System.Environment]::GetEnvironmentVariable($Name, "Process")
    if ([string]::IsNullOrEmpty($value)) {
        Write-Error "$Name is not set in the current environment. Set it before launching; the launcher never prints its value."
    }
}

function Start-RealtimeServerProcess {
    $cargo = (Get-Command "cargo" -ErrorAction Stop).Source
    $arguments = @("run", "-p", "qsf_realtime_server")
    if ($RandomSessionId) {
        $arguments += @("--", "--random-session-id")
    }
    Write-Host "Starting realtime server: $(Format-Command -Executable $cargo -Arguments $arguments)"
    # -NoNewWindow streams the server's cargo build and runtime logs into this console so
    # startup failures stay visible; the monitor loop still supervises it as a child.
    $serverProcess = Start-Process -FilePath $cargo -ArgumentList $arguments -WorkingDirectory $projectRoot -NoNewWindow -PassThru
    Write-Host "Realtime server PID: $($serverProcess.Id)"
    return $serverProcess
}

function Start-RealtimeUiProcess {
    param(
        [Parameter(Mandatory = $true)]
        [int]$PortNumber
    )

    $npm = Get-NpmCommandPath
    # --strictPort makes Vite bind exactly the port we resolved as free, so the URL we
    # open is guaranteed to match instead of Vite silently picking the next free port.
    $arguments = @(
        "run",
        "dev",
        "--",
        "--port",
        $PortNumber.ToString([System.Globalization.CultureInfo]::InvariantCulture),
        "--strictPort"
    )
    Write-Host "Starting Vite dev server: $(Format-Command -Executable $npm -Arguments $arguments)"
    # Vite runs in its own window so its output does not interleave with the server logs
    # streamed into this console. Ctrl+C here cannot reach that window, so the finally
    # block force-kills the tree (taskkill /T) instead of relying on signal propagation.
    $uiProcess = Start-Process -FilePath $npm -ArgumentList $arguments -WorkingDirectory $realtimeUiDir -PassThru
    Write-Host "Vite dev server PID: $($uiProcess.Id)"
    return $uiProcess
}

function Invoke-Realtime {
    Test-RequiredSecret -Name "OPENAI_API_KEY"
    Test-UiDependencies -Dir $realtimeUiDir

    $uiPort = Get-AvailablePort -PreferredPort $realtimeUiPort
    $uiUrl = "http://localhost:$uiPort"

    Write-Host "Realtime server API: http://127.0.0.1:$realtimeServerPort"
    Write-Host "Realtime UI (open this in your browser): $uiUrl"
    if ($uiPort -ne $realtimeUiPort) {
        Write-Host "Preferred UI port $realtimeUiPort was busy; using $uiPort instead."
    }
    Write-Host "OPENAI_API_KEY: present in environment; value not shown"
    Write-Host "Press Ctrl+C to stop both the realtime server and the Vite dev server."

    $serverProcess = $null
    $uiProcess = $null
    $browserOpened = $false
    try {
        # The child process copies its environment at creation, so applying the delta
        # around the start (and restoring afterwards) pins the server's provider settings
        # without leaking them into the launcher's own session.
        $serverProcess = Invoke-WithEnvironmentDelta -Delta (Get-RealtimeEnvironmentDelta) -ScriptBlock {
            Start-RealtimeServerProcess
        }
        $uiProcess = Start-RealtimeUiProcess -PortNumber $uiPort

        # Supervise both children: report the first unexpected exit, open the browser only
        # once both ports answer (so a failed server never yields a broken page), and let a
        # Ctrl+C during Start-Sleep unwind into the finally block for deterministic cleanup.
        while ($true) {
            if ($serverProcess.HasExited) {
                Write-Host "Realtime server exited with code $($serverProcess.ExitCode) before shutdown was requested; see its log above."
                $script:QsfExitCode = $serverProcess.ExitCode
                break
            }
            if ($uiProcess.HasExited) {
                Write-Host "Vite dev server exited with code $($uiProcess.ExitCode) before shutdown was requested."
                $script:QsfExitCode = $uiProcess.ExitCode
                break
            }
            if (-not $browserOpened -and
                (Test-PortOccupied -HostName "127.0.0.1" -PortNumber $realtimeServerPort) -and
                (Test-PortOccupied -HostName "127.0.0.1" -PortNumber $uiPort)) {
                Write-Host "Both servers are up. Opening browser: $uiUrl"
                Start-Process $uiUrl
                $browserOpened = $true
            }
            Start-Sleep -Milliseconds 500
        }
    }
    finally {
        Write-Host "Stopping realtime processes..."
        Stop-ProcessTree -Process $uiProcess
        Stop-ProcessTree -Process $serverProcess
    }
}

function New-QsfStateBackup {
    param(
        [Parameter(Mandatory = $true)]
        [string]$StateDirPath,

        [Parameter(Mandatory = $true)]
        [string]$BackupRootPath,

        [ValidateSet("sleep", "restore")]
        [string]$BackupKind = "sleep",

        [int]$KeepCount = 0
    )

    if (-not (Test-Path -LiteralPath $StateDirPath -PathType Container)) {
        return $null
    }

    # Reject a backup root that is the state dir itself or nested inside it: a
    # recursive copy would otherwise try to copy the backup into itself and can
    # fail partway through. This is exposed by the documented `-StateDir state`.
    $resolvedStateDir = (Resolve-Path -LiteralPath $StateDirPath).Path
    $stateDirWithSep = $resolvedStateDir.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
    $normalizedBackupRoot = [System.IO.Path]::GetFullPath($BackupRootPath)
    if ($normalizedBackupRoot -eq $resolvedStateDir -or
        $normalizedBackupRoot.StartsWith($stateDirWithSep, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Backup root '$BackupRootPath' must not be inside the state directory '$StateDirPath'."
    }

    $leaf = Split-Path -Leaf $StateDirPath
    $timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
    New-Item -ItemType Directory -Force $BackupRootPath | Out-Null

    $backupPrefix = if ($BackupKind -eq "restore") { "$leaf-restore" } else { $leaf }
    $backupPath = Join-Path $BackupRootPath "$backupPrefix-$timestamp"
    $suffix = 2
    while (Test-Path -LiteralPath $backupPath) {
        $backupPath = Join-Path $BackupRootPath "$backupPrefix-$timestamp-$suffix"
        $suffix++
    }
    Copy-Item -LiteralPath $StateDirPath -Destination $backupPath -Recurse

    if ($KeepCount -gt 0) {
        $existing = @(
            Get-ChildItem -LiteralPath $BackupRootPath -Directory -Filter "$leaf-*" |
            Where-Object { Test-QsfSleepBackupName -Name $_.Name -Leaf $leaf } |
            Sort-Object CreationTime, Name -Descending
        )
        if ($existing.Count -gt $KeepCount) {
            foreach ($stale in $existing[$KeepCount..($existing.Count - 1)]) {
                Remove-Item -LiteralPath $stale.FullName -Recurse -Force
            }
        }
    }

    return $backupPath
}

function Test-QsfSleepBackupName {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name,

        [Parameter(Mandatory = $true)]
        [string]$Leaf
    )

    $escapedLeaf = [System.Text.RegularExpressions.Regex]::Escape($Leaf)
    return $Name -match "^$escapedLeaf-\d{8}-\d{6}(-\d+)?$"
}

function Restore-QsfStateBackup {
    param(
        [Parameter(Mandatory = $true)]
        [string]$BackupName,

        [Parameter(Mandatory = $true)]
        [string]$StateDirPath,

        [Parameter(Mandatory = $true)]
        [string]$BackupRootPath
    )

    $leaf = Split-Path -Leaf $StateDirPath
    if ($BackupName -eq "latest") {
        $newest = @(
            Get-ChildItem -LiteralPath $BackupRootPath -Directory -Filter "$leaf-*" -ErrorAction SilentlyContinue |
            Where-Object { Test-QsfSleepBackupName -Name $_.Name -Leaf $leaf } |
            Sort-Object CreationTime, Name -Descending
        ) | Select-Object -First 1
        if ($null -eq $newest) {
            Write-Error "No backups found for '$leaf' under $BackupRootPath."
        }
        $sourcePath = $newest.FullName
    }
    else {
        # Reject a backup whose name is not for this state dir's leaf BEFORE any
        # mutation (self-backup, staging), so a tab-completed cross-leaf name cannot
        # silently overwrite live data with the wrong state dir's contents.
        if ($BackupName -notlike "$leaf-*") {
            Write-Error "Backup '$BackupName' is for a different state dir (expected leaf '$leaf-...'). Pass -StateDir for the matching state dir, or use: .\scripts\qsf.ps1 restore latest"
        }
        $sourcePath = Join-Path $BackupRootPath $BackupName
        if (-not (Test-Path -LiteralPath $sourcePath -PathType Container)) {
            Write-Error "No backup named '$BackupName' under $BackupRootPath. Run: .\scripts\qsf.ps1 restore"
        }
    }

    # Self-backup without pruning: pruning here could delete the very backup being restored.
    $selfBackup = New-QsfStateBackup -StateDirPath $StateDirPath -BackupRootPath $BackupRootPath -BackupKind "restore"
    if ($null -ne $selfBackup) {
        Write-Host "Current state backed up to: $selfBackup"
    }

    # Stage the restore into a temporary sibling first, then swap. Copy or swap
    # failures must not leave the operator without the original live state dir.
    $parent = Split-Path -Parent $StateDirPath
    $staging = Join-Path $parent "$leaf.restore-staging-$(Get-Date -Format 'yyyyMMddHHmmssfff')"
    $oldLive = Join-Path $parent "$leaf.restore-old-live-$(Get-Date -Format 'yyyyMMddHHmmssfff')"
    try {
        Copy-Item -LiteralPath $sourcePath -Destination $staging -Recurse
        if (-not (Test-Path -LiteralPath $staging -PathType Container)) {
            throw "Staged restore copy is missing at $staging."
        }
    }
    catch {
        Remove-Item -LiteralPath $staging -Recurse -Force -ErrorAction SilentlyContinue
        throw
    }

    $renamedLive = $false
    try {
        if (Test-Path -LiteralPath $StateDirPath) {
            Move-Item -LiteralPath $StateDirPath -Destination $oldLive
            $renamedLive = $true
        }
        Move-Item -LiteralPath $staging -Destination $StateDirPath
    }
    catch {
        if ($renamedLive -and -not (Test-Path -LiteralPath $StateDirPath) -and (Test-Path -LiteralPath $oldLive)) {
            Move-Item -LiteralPath $oldLive -Destination $StateDirPath
        }
        Remove-Item -LiteralPath $staging -Recurse -Force -ErrorAction SilentlyContinue
        throw
    }

    if ($renamedLive -and (Test-Path -LiteralPath $oldLive)) {
        try {
            Remove-Item -LiteralPath $oldLive -Recurse -Force
        }
        catch {
            Write-Warning "Restored state, but could not remove old live state at $oldLive. Remove it manually after closing any process that still holds it."
        }
    }
    Write-Host "Restored $StateDirPath from $(Split-Path -Leaf $sourcePath)"

    return $sourcePath
}

function Show-QsfStateBackups {
    param(
        [Parameter(Mandatory = $true)]
        [string]$BackupRootPath
    )

    $backups = @(
        Get-ChildItem -LiteralPath $BackupRootPath -Directory -ErrorAction SilentlyContinue |
        Sort-Object CreationTime, Name -Descending
    )
    if ($backups.Count -eq 0) {
        Write-Host "No backups found under $BackupRootPath."
        Write-Host "Backups are created automatically by: .\scripts\qsf.ps1 sleep"
        return
    }

    Write-Host "Available backups (newest first):"
    foreach ($backup in $backups) {
        Write-Host ("  {0}  (created {1:yyyy-MM-dd HH:mm:ss})" -f $backup.Name, $backup.CreationTime)
    }
    Write-Host ""
    Write-Host "Restore with: .\scripts\qsf.ps1 restore <name>   (or: restore latest)"
}

function Invoke-Restore {
    $backupRootPath = Join-Path $projectRoot $backupRootRelative
    if ([string]::IsNullOrWhiteSpace($Subject)) {
        Show-QsfStateBackups -BackupRootPath $backupRootPath
        return
    }

    [void](Restore-QsfStateBackup `
        -BackupName $Subject `
        -StateDirPath (Join-Path $projectRoot $StateDir) `
        -BackupRootPath $backupRootPath)
}

function Invoke-Sleep {
    if ($Provider -eq "openai") {
        Test-RequiredSecret -Name "OPENAI_API_KEY"
    }

    Write-Host "Sleep state directory: $StateDir"
    Write-Host "Sleep provider: $Provider"
    if ($Provider -eq "openai") {
        Write-Host "OPENAI_API_KEY: present in environment; value not shown"
    }

    $backupPath = New-QsfStateBackup `
        -StateDirPath (Join-Path $projectRoot $StateDir) `
        -BackupRootPath (Join-Path $projectRoot $backupRootRelative) `
        -KeepCount $sleepBackupKeepCount
    if ($null -ne $backupPath) {
        $relativeBackup = [System.IO.Path]::GetRelativePath($projectRoot, $backupPath) -replace '\\', '/'
        Write-Host "State backup: $relativeBackup (keeping last $sleepBackupKeepCount)"
    }
    else {
        Write-Host "State backup: skipped ($StateDir does not exist yet)"
    }

    Invoke-WithEnvironmentDelta -Delta (Get-SleepEnvironmentDelta) -ScriptBlock {
        Invoke-LoggedCommand -Executable "cargo" -Arguments @(
            "run",
            "-p",
            "qsf_app",
            "--",
            "sleep",
            "--state-dir",
            $StateDir,
            "--provider",
            $Provider,
            "--workspace-root",
            $projectRoot,
            "--ledger-path",
            $WorldCorpusLedger
        )
    }
}

function Invoke-Goals {
    $arguments = @(
        "run",
        "-p",
        "qsf_app",
        "--",
        "goals",
        "--state-dir",
        $StateDir
    )
    if (-not [string]::IsNullOrWhiteSpace($Subject)) {
        $arguments += @("--session", $Subject)
    }
    Invoke-LoggedCommand -Executable "cargo" -Arguments $arguments
}

function Get-TranscriptArguments {
    $arguments = @(
        "run",
        "-p",
        "qsf_app",
        "--",
        "transcript",
        "--state-dir",
        $StateDir
    )
    if (-not [string]::IsNullOrWhiteSpace($Subject)) {
        $arguments += @("--session", $Subject)
    }
    if ($All) {
        $arguments += "--all"
    }
    if ($Pretty) {
        $arguments += "--pretty"
    }
    if ($Full) {
        $arguments += "--full"
    }
    if (-not [string]::IsNullOrWhiteSpace($Out)) {
        $arguments += @("--out", $Out)
    }
    return $arguments
}

function Invoke-Transcript {
    Invoke-LoggedCommand -Executable "cargo" -Arguments (Get-TranscriptArguments)
}

function Invoke-WorldIngest {
    Invoke-WithEnvironmentDelta -Delta (Get-WorldCorpusIngestEnvironmentDelta) -ScriptBlock {
        Invoke-LoggedCommand -Executable "cargo" -Arguments @(
            "run",
            "-p",
            "qsf_app",
            "--",
            "ingest-world",
            "--ledger-path",
            $WorldCorpusLedger
        )
    }
}

function Test-QsfAutoRunEnabled {
    $skipAutoRun = Get-Variable -Name "QsfSkipAutoRun" -Scope Script -ValueOnly -ErrorAction SilentlyContinue
    return -not ($skipAutoRun -is [bool] -and $skipAutoRun)
}

if (Test-QsfAutoRunEnabled) {
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
        "realtime" {
            Invoke-Realtime
        }
        "sleep" {
            Invoke-Sleep
        }
        "goals" {
            Invoke-Goals
        }
        "transcript" {
            Invoke-Transcript
        }
        "world-ingest" {
            Invoke-WorldIngest
        }
        "restore" {
            Invoke-Restore
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
}
