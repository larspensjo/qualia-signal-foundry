BeforeAll {
    $script:CompletionScript = Join-Path $PSScriptRoot "qsf-completion.ps1"
    . $script:CompletionScript

    function Complete-QsfInput {
        param(
            [Parameter(Mandatory = $true)]
            [string]$InputText
        )

        $result = [System.Management.Automation.CommandCompletion]::CompleteInput(
            $InputText,
            $InputText.Length,
            $null
        )

        return @($result.CompletionMatches | ForEach-Object { $_.CompletionText })
    }
}

Describe "qsf.ps1 argument completion" {
    It "completes launcher commands" {
        $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 "

        $completions | Should -Contain "app"
        $completions | Should -Contain "browser"
        $completions | Should -Contain "ui"
        $completions | Should -Contain "workbench"
        $completions | Should -Contain "realtime"
        $completions | Should -Contain "sleep"
        $completions | Should -Contain "world-ingest"
        $completions | Should -Contain "doctor"
        $completions | Should -Contain "list"
        $completions | Should -Contain "help"
    }

    It "completes ui targets" {
        $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 ui "

        $completions | Should -Contain "browser"
        $completions | Should -Contain "realtime"
    }

    It "completes list targets" {
        $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 list "

        $completions | Should -Contain "experiments"
        $completions | Should -Contain "profiles"
    }

    It "completes profiles from the checked-in profile file" {
        $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 app -LaunchProfile "

        $completions | Should -Contain "mock"
        $completions | Should -Contain "openai-text"
        $completions | Should -Contain "file-memory"
        $completions | Should -Contain "demo-memory"
        $completions | Should -Contain "openai-transcription-mic"
    }

    It "keeps profile completion for the compatibility alias" {
        $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 app -Profile "

        $completions | Should -Contain "mock"
    }

    It "filters profile completions by the current word" {
        $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 app -LaunchProfile open"

        $completions | Should -Contain "openai-text"
        $completions | Should -Contain "openai-transcription-mic"
        $completions | Should -Not -Contain "mock"
    }

    It "completes static experiment names without invoking Cargo" {
        $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 app -Experiment "

        $completions | Should -Contain "multi-turn-text-loop"
        $completions | Should -Contain "text-owned-voice-loop"
        $completions | Should -Contain "streaming-transcription-mvp"
    }

    It "completes likely JSON store paths" {
        $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 browser -Store "

        $completions | Should -Contain "crates/qsf_browser_server/tests/fixtures/small-store.json"
    }

    It "completes positional store paths for browser and workbench" {
        $browserCompletions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 browser "
        $workbenchCompletions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 workbench "

        $browserCompletions | Should -Contain "crates/qsf_browser_server/tests/fixtures/small-store.json"
        $workbenchCompletions | Should -Contain "crates/qsf_browser_server/tests/fixtures/small-store.json"
    }

    It "completes bind host values" {
        $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 browser -BindHost "

        $completions | Should -Contain "127.0.0.1"
        $completions | Should -Contain "0.0.0.0"
    }

    It "completes session memory source values" {
        $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 app -Experiment multi-turn-text-loop -SessionMemorySource "

        $completions | Should -Contain "auto"
        $completions | Should -Contain "empty"
        $completions | Should -Contain "file"
        $completions | Should -Contain "fixture"
    }

    It "completes session memory file paths" {
        $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 app -Experiment multi-turn-text-loop -SessionMemoryFile "

        $completions | Should -Contain "docs/Experiments/Fixtures/session-memory.empty.json"
    }

    It "completes sleep providers" {
        $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 sleep -Provider "

        $completions | Should -Contain "openai"
        $completions | Should -Contain "mock"
    }

    It "completes sleep state directories" {
        $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 sleep -StateDir "

        $completions | Should -Contain "state/realtime"
        $completions | Should -Contain "state/session"
    }

    It "completes the default world corpus ledger" {
        $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 world-ingest -WorldCorpusLedger "

        $completions | Should -Contain "state/world-corpus/index.json"
    }

    It "completes an explicitly configured world corpus path" {
        $previous = [System.Environment]::GetEnvironmentVariable("QSF_WORLD_CORPUS_PATH", "Process")
        $configuredPath = Join-Path $TestDrive "world-corpus-output"
        try {
            [System.Environment]::SetEnvironmentVariable("QSF_WORLD_CORPUS_PATH", $configuredPath, "Process")

            $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 world-ingest -WorldCorpusPath "

            $completions | Should -Contain $configuredPath
        }
        finally {
            [System.Environment]::SetEnvironmentVariable("QSF_WORLD_CORPUS_PATH", $previous, "Process")
        }
    }

    It "completes the restore command" {
        $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 re"

        $completions | Should -Contain "restore"
        $completions | Should -Contain "realtime"
    }

    It "completes latest for restore even without backups" {
        $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 restore "

        $completions | Should -Contain "latest"
    }

    Context "with a hermetic project root that has backups" {
        BeforeEach {
            $script:OriginalCompletionRoot = $script:QsfCompletionProjectRoot
            $script:QsfCompletionProjectRoot = "$TestDrive"
            New-Item -ItemType Directory -Force (Join-Path $TestDrive "state/realtime") | Out-Null
            New-Item -ItemType Directory -Force (Join-Path $TestDrive "state/backups/realtime-20260705-120000") | Out-Null
            New-Item -ItemType Directory -Force (Join-Path $TestDrive "state/backups/session-20260705-120000") | Out-Null
        }

        AfterEach {
            $script:QsfCompletionProjectRoot = $script:OriginalCompletionRoot
            Remove-Item -LiteralPath (Join-Path $TestDrive "state") -Recurse -Force -ErrorAction SilentlyContinue
        }

        It "completes real backup names for restore" {
            $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 restore "

            $completions | Should -Contain "latest"
            $completions | Should -Contain "realtime-20260705-120000"
        }

        It "offers only the default realtime leaf's backups for restore" {
            # A session-leaf backup must never be tab-completed into the default
            # realtime restore; the launcher's leaf guard would reject it anyway.
            $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 restore "

            $completions | Should -Contain "realtime-20260705-120000"
            $completions | Should -Not -Contain "session-20260705-120000"
        }

        It "offers the matching leaf's backups when -StateDir targets another state dir" {
            $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 restore -StateDir state/session "

            $completions | Should -Contain "latest"
            $completions | Should -Contain "session-20260705-120000"
            $completions | Should -Not -Contain "realtime-20260705-120000"
        }

        It "does not offer the backups root as a sleep state dir when it exists" {
            $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 sleep -StateDir "

            $completions | Should -Contain "state/realtime"
            $completions | Should -Not -Contain "state/backups"
        }
    }
}
