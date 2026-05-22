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
        $completions | Should -Contain "doctor"
        $completions | Should -Contain "list"
        $completions | Should -Contain "help"
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

    It "completes bind host values" {
        $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 browser -BindHost "

        $completions | Should -Contain "127.0.0.1"
        $completions | Should -Contain "0.0.0.0"
    }
}
