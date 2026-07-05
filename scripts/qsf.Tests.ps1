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

    It "requires OPENAI_API_KEY for the default provider" {
        { Invoke-Sleep } | Should -Throw "*OPENAI_API_KEY is not set*"
    }
}
