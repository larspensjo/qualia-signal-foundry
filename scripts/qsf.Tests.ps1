BeforeAll {
    $script:LauncherScript = Join-Path $PSScriptRoot "qsf.ps1"

    $script:OriginalEnvironment = @{}
    $script:TestEnvironmentNames = @(
        "OPENAI_API_KEY",
        "QSF_ACCEPT_MEMORY_DRAFT",
        "QSF_CUSTOM_API_KEY",
        "QSF_MODEL_PROVIDER",
        "QSF_STATE_DIR",
        "QSF_SESSION_MAX_TURNS",
        "QSF_SESSION_MEMORY_SOURCE"
    )

    foreach ($name in $script:TestEnvironmentNames) {
        $script:OriginalEnvironment[$name] = [System.Environment]::GetEnvironmentVariable($name, "Process")
    }

    function Get-TestEnvironmentDelta {
        param(
            [string]$Experiment = "text-owned-voice-loop",
            [string]$LaunchProfile = ""
        )

        $script:QsfSkipAutoRun = $true
        if (-not [string]::IsNullOrWhiteSpace($LaunchProfile)) {
            . $script:LauncherScript -Command "app" -Experiment $Experiment -LaunchProfile $LaunchProfile
        }
        else {
            . $script:LauncherScript -Command "app" -Experiment $Experiment
        }
        return Get-ProfileEnvironmentDelta
    }
}

AfterAll {
    foreach ($name in $script:TestEnvironmentNames) {
        [System.Environment]::SetEnvironmentVariable($name, $script:OriginalEnvironment[$name], "Process")
    }
}

Describe "qsf.ps1 deterministic environment" {
    BeforeEach {
        foreach ($name in $script:TestEnvironmentNames) {
            [System.Environment]::SetEnvironmentVariable($name, $null, "Process")
        }
    }

    It "clears ambient non-secret QSF variables by default" {
        [System.Environment]::SetEnvironmentVariable("QSF_MODEL_PROVIDER", "openai", "Process")
        [System.Environment]::SetEnvironmentVariable("QSF_SESSION_MAX_TURNS", "2", "Process")
        [System.Environment]::SetEnvironmentVariable("QSF_ACCEPT_MEMORY_DRAFT", "runs/draft.json", "Process")

        $delta = Get-TestEnvironmentDelta

        $delta.Sets.Contains("QSF_MODEL_PROVIDER") | Should -BeFalse
        $delta.Clears | Should -Contain "QSF_MODEL_PROVIDER"
        $delta.Clears | Should -Contain "QSF_SESSION_MAX_TURNS"
        $delta.Clears | Should -Contain "QSF_ACCEPT_MEMORY_DRAFT"
    }

    It "applies launcher defaults after ambient QSF values are cleared" {
        [System.Environment]::SetEnvironmentVariable("QSF_SESSION_MEMORY_SOURCE", "phase_four_fixture", "Process")

        $delta = Get-TestEnvironmentDelta -Experiment "multi-turn-text-loop"

        $delta.Sets["QSF_SESSION_MEMORY_SOURCE"] | Should -Be "file"
        $delta.Sets["QSF_SESSION_MEMORY_FILE"] | Should -Be "docs/Experiments/Fixtures/session-memory.empty.json"
        $delta.Sets["QSF_SESSION_ALLOW_OVER_LIMIT"] | Should -Be "true"
        $delta.Clears | Should -Not -Contain "QSF_SESSION_MEMORY_SOURCE"
    }

    It "overlays profile values onto the managed clear list" {
        [System.Environment]::SetEnvironmentVariable("OPENAI_API_KEY", "test-key", "Process")

        $delta = Get-TestEnvironmentDelta -LaunchProfile "openai-text"

        $delta.Sets["QSF_MODEL_PROVIDER"] | Should -Be "openai"
        $delta.Clears | Should -Not -Contain "QSF_MODEL_PROVIDER"
        $delta.Clears | Should -Contain "QSF_TRANSCRIPT_PROVIDER"
    }

    It "does not manage secret-like QSF variables" {
        [System.Environment]::SetEnvironmentVariable("QSF_CUSTOM_API_KEY", "test-secret", "Process")

        $delta = Get-TestEnvironmentDelta

        $delta.Clears | Should -Not -Contain "QSF_CUSTOM_API_KEY"
    }
}

Describe "qsf.ps1 realtime launcher" {
    BeforeAll {
        $script:QsfSkipAutoRun = $true
        . $script:LauncherScript -Command "help"
    }

    BeforeEach {
        [System.Environment]::SetEnvironmentVariable("OPENAI_API_KEY", $null, "Process")
    }

    It "resolves the browser UI target by default" {
        $target = Get-UiTarget -Target ""

        $target.Name | Should -Be "browser"
        $target.Dir | Should -Be (Join-Path $projectRoot "crates/qsf_browser_server/ui")
    }

    It "resolves the realtime UI target" {
        $target = Get-UiTarget -Target "realtime"

        $target.Name | Should -Be "realtime"
        $target.Dir | Should -Be (Join-Path $projectRoot "crates/qsf_realtime_server/ui")
    }

    It "rejects an unknown ui target" {
        { Get-UiTarget -Target "bogus" } | Should -Throw "*Unknown ui target*"
    }

    It "pins the model provider to openai for the realtime server" {
        $delta = Get-RealtimeEnvironmentDelta

        $delta.Sets["QSF_MODEL_PROVIDER"] | Should -Be "openai"
        $delta.Clears | Should -Not -Contain "QSF_MODEL_PROVIDER"
    }

    It "clears ambient non-secret QSF variables for the realtime server" {
        [System.Environment]::SetEnvironmentVariable("QSF_SESSION_MAX_TURNS", "2", "Process")
        try {
            $delta = Get-RealtimeEnvironmentDelta

            $delta.Clears | Should -Contain "QSF_SESSION_MAX_TURNS"
        }
        finally {
            [System.Environment]::SetEnvironmentVariable("QSF_SESSION_MAX_TURNS", $null, "Process")
        }
    }

    It "fails fast when the required secret is absent" {
        { Test-RequiredSecret -Name "OPENAI_API_KEY" } | Should -Throw "*OPENAI_API_KEY is not set*"
    }

    It "passes silently when the required secret is present without echoing it" {
        [System.Environment]::SetEnvironmentVariable("OPENAI_API_KEY", "super-secret-value", "Process")

        $output = Test-RequiredSecret -Name "OPENAI_API_KEY" 6>&1

        $output | Should -BeNullOrEmpty
        ($output | Out-String) | Should -Not -Match "super-secret-value"
    }

    It "resolves the preferred UI port when it is free" {
        $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
        $listener.Start()
        $freePort = ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
        $listener.Stop()

        Get-AvailablePort -PreferredPort $freePort | Should -Be $freePort
    }

    It "skips an occupied UI port and resolves the next free one" {
        $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
        $listener.Start()
        $occupiedPort = ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
        try {
            $resolved = Get-AvailablePort -PreferredPort $occupiedPort
            $resolved | Should -BeGreaterThan $occupiedPort
        }
        finally {
            $listener.Stop()
        }
    }
}

Describe "qsf.ps1 sleep launcher" {
    BeforeAll {
        $script:QsfSkipAutoRun = $true
        . $script:LauncherScript -Command "help"
    }

    BeforeEach {
        [System.Environment]::SetEnvironmentVariable("OPENAI_API_KEY", $null, "Process")
    }

    It "defaults to openai over realtime state" {
        $Provider | Should -Be "openai"
        $StateDir | Should -Be "state/realtime"
    }

    It "pins the selected model provider for sleep" {
        $delta = Get-SleepEnvironmentDelta

        $delta.Sets["QSF_MODEL_PROVIDER"] | Should -Be "openai"
        $delta.Clears | Should -Not -Contain "QSF_MODEL_PROVIDER"
    }

    It "clears ambient non-secret QSF variables for sleep" {
        [System.Environment]::SetEnvironmentVariable("QSF_STATE_DIR", "ambient-state", "Process")
        try {
            $delta = Get-SleepEnvironmentDelta

            $delta.Clears | Should -Contain "QSF_STATE_DIR"
        }
        finally {
            [System.Environment]::SetEnvironmentVariable("QSF_STATE_DIR", $null, "Process")
        }
    }

    It "requires OPENAI_API_KEY for the default provider and creates no backup" {
        $originalRoot = $projectRoot
        try {
            $script:projectRoot = "$TestDrive"
            New-Item -ItemType Directory -Force (Join-Path $TestDrive "state/realtime") | Out-Null

            { Invoke-Sleep } | Should -Throw "*OPENAI_API_KEY is not set*"

            Test-Path -LiteralPath (Join-Path $TestDrive "state/backups") | Should -BeFalse
        }
        finally {
            $script:projectRoot = $originalRoot
            Remove-Item -LiteralPath (Join-Path $TestDrive "state") -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It "backs up the state dir before running cargo" {
        try {
            $stateDirPath = Join-Path $TestDrive "state/realtime"
            New-Item -ItemType Directory -Force $stateDirPath | Out-Null
            Set-Content -LiteralPath (Join-Path $stateDirPath "memory-store.json") -Value '{"records":[]}'

            # Re-source the launcher into this test's scope so Invoke-Sleep closes
            # over the mock provider (skips the OPENAI_API_KEY check) and the
            # TestDrive project root. Pester promotes the outer BeforeAll's
            # dot-sourced variables, so $script: assignments here do not reach the
            # launcher's own $Provider/$projectRoot/$StateDir.
            . $script:LauncherScript -Command "help" -Provider "mock" -StateDir "state/realtime"
            $projectRoot = "$TestDrive"

            $script:BackupPresentAtRun = $false
            Mock -CommandName Invoke-WithEnvironmentDelta -MockWith { & $ScriptBlock }
            Mock -CommandName Invoke-LoggedCommand -MockWith {
                $root = Join-Path $TestDrive "state/backups"
                $script:BackupPresentAtRun = (Test-Path -LiteralPath $root -PathType Container) -and
                    (@(Get-ChildItem -LiteralPath $root -Directory -Filter "realtime-*").Count -gt 0)
            }

            Invoke-Sleep

            # The backup must exist by the time cargo is invoked, not after.
            $script:BackupPresentAtRun | Should -BeTrue
            Should -Invoke Invoke-LoggedCommand -Times 1
        }
        finally {
            Remove-Item -LiteralPath (Join-Path $TestDrive "state") -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

Describe "qsf.ps1 state backups" {
    BeforeAll {
        $script:QsfSkipAutoRun = $true
        . $script:LauncherScript -Command "help"
    }

    BeforeEach {
        $script:TestStateDir = Join-Path $TestDrive "state/realtime"
        $script:TestBackupRoot = Join-Path $TestDrive "state/backups"
        New-Item -ItemType Directory -Force $script:TestStateDir | Out-Null
        Set-Content -LiteralPath (Join-Path $script:TestStateDir "memory-store.json") -Value '{"records":[]}'
        New-Item -ItemType Directory -Force (Join-Path $script:TestStateDir "archive") | Out-Null
        Set-Content -LiteralPath (Join-Path $script:TestStateDir "archive/old-brief.json") -Value '{}'
    }

    AfterEach {
        Remove-Item -LiteralPath (Join-Path $TestDrive "state") -Recurse -Force -ErrorAction SilentlyContinue
    }

    It "creates a timestamped recursive copy of the state dir" {
        $backupPath = New-QsfStateBackup -StateDirPath $script:TestStateDir -BackupRootPath $script:TestBackupRoot

        $backupPath | Should -Not -BeNullOrEmpty
        (Split-Path -Leaf $backupPath) | Should -Match '^realtime-\d{8}-\d{6}'
        (Join-Path $backupPath "memory-store.json") | Should -Exist
        (Join-Path $backupPath "archive/old-brief.json") | Should -Exist
    }

    It "returns null and creates nothing when the state dir is missing" {
        $backupPath = New-QsfStateBackup -StateDirPath (Join-Path $TestDrive "state/absent") -BackupRootPath $script:TestBackupRoot

        $backupPath | Should -BeNullOrEmpty
        Test-Path -LiteralPath $script:TestBackupRoot | Should -BeFalse
    }

    It "creates distinct backups for back-to-back calls" {
        $first = New-QsfStateBackup -StateDirPath $script:TestStateDir -BackupRootPath $script:TestBackupRoot
        $second = New-QsfStateBackup -StateDirPath $script:TestStateDir -BackupRootPath $script:TestBackupRoot

        $second | Should -Not -Be $first
        Test-Path -LiteralPath $first | Should -BeTrue
        Test-Path -LiteralPath $second | Should -BeTrue
    }

    It "prunes to the keep count, newest kept, and only for the same leaf" {
        New-Item -ItemType Directory -Force (Join-Path $script:TestBackupRoot "other-20260101-000000") | Out-Null
        $paths = @()
        foreach ($i in 1..4) {
            $paths += New-QsfStateBackup -StateDirPath $script:TestStateDir -BackupRootPath $script:TestBackupRoot -KeepCount 3
        }

        $remaining = @(Get-ChildItem -LiteralPath $script:TestBackupRoot -Directory -Filter "realtime-*")
        $remaining.Count | Should -Be 3
        Test-Path -LiteralPath $paths[0] | Should -BeFalse
        Test-Path -LiteralPath $paths[3] | Should -BeTrue
        Test-Path -LiteralPath (Join-Path $script:TestBackupRoot "other-20260101-000000") | Should -BeTrue
    }

    It "rejects a backup root nested inside the state dir" {
        $stateRoot = Join-Path $TestDrive "state"
        { New-QsfStateBackup -StateDirPath $stateRoot -BackupRootPath (Join-Path $stateRoot "backups") } |
            Should -Throw "*must not be inside the state directory*"
    }
}
