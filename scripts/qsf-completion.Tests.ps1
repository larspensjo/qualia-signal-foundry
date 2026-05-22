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
        $matches = Complete-QsfInput -InputText ".\scripts\qsf.ps1 "

        $matches | Should -Contain "app"
        $matches | Should -Contain "browser"
        $matches | Should -Contain "ui"
        $matches | Should -Contain "workbench"
        $matches | Should -Contain "doctor"
        $matches | Should -Contain "list"
        $matches | Should -Contain "help"
    }

    It "completes list targets" {
        $matches = Complete-QsfInput -InputText ".\scripts\qsf.ps1 list "

        $matches | Should -Contain "experiments"
        $matches | Should -Contain "profiles"
    }

    It "completes profiles from the checked-in profile file" {
        $matches = Complete-QsfInput -InputText ".\scripts\qsf.ps1 app -Profile "

        $matches | Should -Contain "mock"
        $matches | Should -Contain "openai-text"
        $matches | Should -Contain "file-memory"
        $matches | Should -Contain "openai-transcription-mic"
    }

    It "filters profile completions by the current word" {
        $matches = Complete-QsfInput -InputText ".\scripts\qsf.ps1 app -Profile open"

        $matches | Should -Contain "openai-text"
        $matches | Should -Contain "openai-transcription-mic"
        $matches | Should -Not -Contain "mock"
    }

    It "completes static experiment names without invoking Cargo" {
        $matches = Complete-QsfInput -InputText ".\scripts\qsf.ps1 app -Experiment "

        $matches | Should -Contain "multi-turn-text-loop"
        $matches | Should -Contain "text-owned-voice-loop"
        $matches | Should -Contain "streaming-transcription-mvp"
    }

    It "completes likely JSON store paths" {
        $matches = Complete-QsfInput -InputText ".\scripts\qsf.ps1 browser -Store "

        $matches | Should -Contain "crates/qsf_browser_server/tests/fixtures/small-store.json"
    }

    It "completes bind host values" {
        $matches = Complete-QsfInput -InputText ".\scripts\qsf.ps1 browser -BindHost "

        $matches | Should -Contain "127.0.0.1"
        $matches | Should -Contain "0.0.0.0"
    }
}
