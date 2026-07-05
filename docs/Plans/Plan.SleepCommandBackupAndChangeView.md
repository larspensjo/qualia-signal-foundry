# Sleep Command Backup & Change View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `qsf.ps1 sleep` safe and informative: an automatic timestamped backup of the state directory before every sleep run, a `restore` command to roll back, and an itemized console view of what the sleep run changed (memories, associations, goals, files).

**Architecture:** Backup/restore lives entirely in the PowerShell launcher (`scripts/qsf.ps1`) as parameter-driven, Pester-testable functions; the launcher is the documented Windows operator entry point (DecisionLog 2026-05-22). The change view lives in Rust: `commit_cross_session_sleep` in `crates/qsf_app/src/sleep/update.rs` already computes every state change but discards the detail, so it now also returns a serializable `SleepChangeRecord`, rendered by a pure function and written as a `sleep-changes.json` run artifact. `cli.rs` stays a thin wrapper that prints the rendered view.

**Tech Stack:** PowerShell 7.6 + Pester, Rust (serde, anyhow), existing `qsf_app` sleep pipeline.

## Global Constraints

- Build with `cargo build`; finish with `cargo clippy --all-targets -- -D warnings` and `cargo fmt`.
- Reducers/pure logic stays unit-testable; entry points (`cli.rs`, `mod.rs`) stay thin wrappers.
- Backups root: `state/backups/` (inside the already git-ignored `state/`); backup names are `<state-dir-leaf>-<yyyyMMdd-HHmmss>`; keep the **5** newest per leaf.
- The experiment-side sleep path (`experiments/sleep_phase_session_summary.rs` has its own `commit_cross_session_sleep`) is **out of scope**; only the first-class command path in `crates/qsf_app/src/sleep/update.rs` changes.
- This plan file is ephemeral and is deleted after external review (repo convention). Do not reference its phase numbers from durable documents.

## Verification Per Phase

- Phase 1 (backup): `Invoke-Pester scripts/qsf.Tests.ps1 -Output Detailed`
- Phase 2 (restore + completion): both Pester suites; **human test recommended** (restore round-trip on real state).
- Phase 3 (change view): `cargo test -p qsf_app`; **human test recommended**: `.\scripts\qsf.ps1 sleep -Provider mock` against real `state/realtime` and read the console view.
- Phase 4 (docs + final checks): clippy, fmt, both Pester suites.

Artifact-parsing verification (trace-contract analogue): a Rust test parses the generated `sleep-changes.json` and asserts the fields of `SleepChangeRecord` round-trip, so the artifact cannot silently drift from the console view.

---

## Phase 1 — Pre-sleep backup

### Task 1: `New-QsfStateBackup` + wiring into `Invoke-Sleep`

**Files:**
- Modify: `scripts/qsf.ps1` (constants near line 33-47; new function near `Test-RequiredSecret`; `Invoke-Sleep` at line 1128; help text at line 508)
- Test: `scripts/qsf.Tests.ps1` (new Describe block)

**Interfaces:**
- Produces: `New-QsfStateBackup -StateDirPath <abs> -BackupRootPath <abs> [-KeepCount <int>]` → returns the full path of the created backup directory, or `$null` when the state dir does not exist. `-KeepCount 0` (default) means no pruning. Task 2 reuses this for restore's self-backup.
- Produces: script constants `$backupRootRelative = "state/backups"` and `$sleepBackupKeepCount = 5`.

- [ ] **Step 1: Write the failing tests** — append to `scripts/qsf.Tests.ps1`:

```powershell
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
```

Also cover the `Invoke-Sleep` wiring — the feature promise is "backup before every sleep run", which `New-QsfStateBackup` unit tests do not prove. Append to the existing `"qsf.ps1 sleep launcher"` Describe block in `scripts/qsf.Tests.ps1`:

```powershell
    It "backs up the state dir before running cargo" {
        $originalRoot = $projectRoot
        $originalStateDir = $StateDir
        $originalProvider = $Provider
        try {
            $script:projectRoot = "$TestDrive"
            $script:StateDir = "state/realtime"
            $script:Provider = "mock"
            $stateDirPath = Join-Path $TestDrive "state/realtime"
            New-Item -ItemType Directory -Force $stateDirPath | Out-Null
            Set-Content -LiteralPath (Join-Path $stateDirPath "memory-store.json") -Value '{"records":[]}'

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
            $script:projectRoot = $originalRoot
            $script:StateDir = $originalStateDir
            $script:Provider = $originalProvider
            Remove-Item -LiteralPath (Join-Path $TestDrive "state") -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
```

Extend the existing `"requires OPENAI_API_KEY for the default provider"` test (currently only asserts the throw) so it also proves the failed secret check creates no backup:

```powershell
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `Invoke-Pester scripts/qsf.Tests.ps1 -Output Detailed`
Expected: the new Describe block fails with `New-QsfStateBackup` not recognized; all existing tests still pass.

- [ ] **Step 3: Implement** in `scripts/qsf.ps1`:

Add constants after `$emptySessionMemoryFile` (line 35):

```powershell
$backupRootRelative = "state/backups"
$sleepBackupKeepCount = 5
```

Add the function before `Invoke-Sleep`:

```powershell
function New-QsfStateBackup {
    param(
        [Parameter(Mandatory = $true)]
        [string]$StateDirPath,

        [Parameter(Mandatory = $true)]
        [string]$BackupRootPath,

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

    $backupPath = Join-Path $BackupRootPath "$leaf-$timestamp"
    $suffix = 2
    while (Test-Path -LiteralPath $backupPath) {
        $backupPath = Join-Path $BackupRootPath "$leaf-$timestamp-$suffix"
        $suffix++
    }
    Copy-Item -LiteralPath $StateDirPath -Destination $backupPath -Recurse

    if ($KeepCount -gt 0) {
        $existing = @(
            Get-ChildItem -LiteralPath $BackupRootPath -Directory -Filter "$leaf-*" |
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
```

Wire into `Invoke-Sleep` (line 1128), after the `Test-RequiredSecret`/`Write-Host` preamble and before `Invoke-WithEnvironmentDelta` (a failed secret check must not create backups):

```powershell
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
```

Update `Show-Help`: in the Defaults section change the sleep line to

```text
  Sleep update:    state/realtime through the $Provider provider; openai requires OPENAI_API_KEY
                   backs up the state dir to state/backups/<name>-<timestamp> first (keeps last 5)
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `Invoke-Pester scripts/qsf.Tests.ps1 -Output Detailed`
Expected: all tests PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/qsf.ps1 scripts/qsf.Tests.ps1
git commit -m "Back up the sleep state dir before each sleep run"
```

---

## Phase 2 — Restore command and completion

### Task 2: `restore` command

**Files:**
- Modify: `scripts/qsf.ps1` (new functions after `New-QsfStateBackup`; command dispatch at line 1161; help text)
- Test: `scripts/qsf.Tests.ps1`

**Interfaces:**
- Consumes: `New-QsfStateBackup` from Task 1 (self-backup before restoring, `-KeepCount 0` so the restore source can never be pruned away mid-restore).
- Produces: `Restore-QsfStateBackup -BackupName <name|latest> -StateDirPath <abs> -BackupRootPath <abs>` → restores and returns the resolved backup path (throws on unknown name / no backups). `Show-QsfStateBackups -BackupRootPath <abs>` lists backups newest-first. Launcher command `restore [<name>|latest]` using the existing positional `$Subject`; the existing `-StateDir` parameter selects the destination.

- [ ] **Step 1: Write the failing tests** — append to the `"qsf.ps1 state backups"` Describe block:

```powershell
    It "restores a named backup into the state dir" {
        $backupPath = New-QsfStateBackup -StateDirPath $script:TestStateDir -BackupRootPath $script:TestBackupRoot
        Set-Content -LiteralPath (Join-Path $script:TestStateDir "memory-store.json") -Value '{"records":["changed"]}'

        Restore-QsfStateBackup -BackupName (Split-Path -Leaf $backupPath) -StateDirPath $script:TestStateDir -BackupRootPath $script:TestBackupRoot | Out-Null

        Get-Content -LiteralPath (Join-Path $script:TestStateDir "memory-store.json") -Raw |
            Should -Match '"records":\[\]'
    }

    It "restores the newest backup for 'latest'" {
        Set-Content -LiteralPath (Join-Path $script:TestStateDir "memory-store.json") -Value '{"version":"old"}'
        New-QsfStateBackup -StateDirPath $script:TestStateDir -BackupRootPath $script:TestBackupRoot | Out-Null
        Set-Content -LiteralPath (Join-Path $script:TestStateDir "memory-store.json") -Value '{"version":"new"}'
        New-QsfStateBackup -StateDirPath $script:TestStateDir -BackupRootPath $script:TestBackupRoot | Out-Null
        Set-Content -LiteralPath (Join-Path $script:TestStateDir "memory-store.json") -Value '{"version":"live"}'

        Restore-QsfStateBackup -BackupName "latest" -StateDirPath $script:TestStateDir -BackupRootPath $script:TestBackupRoot | Out-Null

        Get-Content -LiteralPath (Join-Path $script:TestStateDir "memory-store.json") -Raw |
            Should -Match '"version":"new"'
    }

    It "backs up the current state before restoring so a restore is undoable" {
        $backupPath = New-QsfStateBackup -StateDirPath $script:TestStateDir -BackupRootPath $script:TestBackupRoot
        Set-Content -LiteralPath (Join-Path $script:TestStateDir "memory-store.json") -Value '{"records":["live-only"]}'
        $countBefore = @(Get-ChildItem -LiteralPath $script:TestBackupRoot -Directory -Filter "realtime-*").Count

        Restore-QsfStateBackup -BackupName (Split-Path -Leaf $backupPath) -StateDirPath $script:TestStateDir -BackupRootPath $script:TestBackupRoot | Out-Null

        $backups = @(Get-ChildItem -LiteralPath $script:TestBackupRoot -Directory -Filter "realtime-*")
        $backups.Count | Should -Be ($countBefore + 1)
        $newest = $backups | Sort-Object CreationTime, Name -Descending | Select-Object -First 1
        Get-Content -LiteralPath (Join-Path $newest.FullName "memory-store.json") -Raw |
            Should -Match 'live-only'
    }

    It "fails on an unknown backup name without touching the state dir" {
        { Restore-QsfStateBackup -BackupName "realtime-19700101-000000" -StateDirPath $script:TestStateDir -BackupRootPath $script:TestBackupRoot } |
            Should -Throw "*No backup named*"
        (Join-Path $script:TestStateDir "memory-store.json") | Should -Exist
    }

    It "leaves the live state dir intact when the staged restore copy fails" {
        $backupPath = New-QsfStateBackup -StateDirPath $script:TestStateDir -BackupRootPath $script:TestBackupRoot
        Set-Content -LiteralPath (Join-Path $script:TestStateDir "memory-store.json") -Value '{"records":["live-only"]}'
        # Fail only the staging copy, not the self-backup that runs first.
        Mock -CommandName Copy-Item -ParameterFilter { "$Destination" -like "*restore-staging*" } -MockWith { throw "simulated copy failure" }

        { Restore-QsfStateBackup -BackupName (Split-Path -Leaf $backupPath) -StateDirPath $script:TestStateDir -BackupRootPath $script:TestBackupRoot } |
            Should -Throw "*simulated copy failure*"

        (Join-Path $script:TestStateDir "memory-store.json") | Should -Exist
        Get-Content -LiteralPath (Join-Path $script:TestStateDir "memory-store.json") -Raw |
            Should -Match 'live-only'
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `Invoke-Pester scripts/qsf.Tests.ps1 -Output Detailed`
Expected: new tests fail with `Restore-QsfStateBackup` not recognized.

- [ ] **Step 3: Implement** in `scripts/qsf.ps1`:

```powershell
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
            Sort-Object CreationTime, Name -Descending
        ) | Select-Object -First 1
        if ($null -eq $newest) {
            Write-Error "No backups found for '$leaf' under $BackupRootPath."
        }
        $sourcePath = $newest.FullName
    }
    else {
        $sourcePath = Join-Path $BackupRootPath $BackupName
        if (-not (Test-Path -LiteralPath $sourcePath -PathType Container)) {
            Write-Error "No backup named '$BackupName' under $BackupRootPath. Run: .\scripts\qsf.ps1 restore"
        }
    }

    # Self-backup without pruning: pruning here could delete the very backup being restored.
    $selfBackup = New-QsfStateBackup -StateDirPath $StateDirPath -BackupRootPath $BackupRootPath
    if ($null -ne $selfBackup) {
        Write-Host "Current state backed up to: $selfBackup"
    }

    # Stage the restore into a temporary sibling first, then swap. A failed copy
    # (locked file, disk error, partial backup) must never leave the operator
    # without a live state directory, so we only remove the live dir once the
    # staged copy is validated.
    $parent = Split-Path -Parent $StateDirPath
    $staging = Join-Path $parent "$leaf.restore-staging-$(Get-Date -Format 'yyyyMMddHHmmssfff')"
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

    if (Test-Path -LiteralPath $StateDirPath) {
        Remove-Item -LiteralPath $StateDirPath -Recurse -Force
    }
    Move-Item -LiteralPath $staging -Destination $StateDirPath
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
```

Add to the command dispatch switch (after the `"sleep"` case):

```powershell
        "restore" {
            Invoke-Restore
        }
```

Update `Show-Help` usage and examples:

```text
  .\scripts\qsf.ps1 restore [<backup-name>|latest] [-StateDir <path>]
```

```text
  .\scripts\qsf.ps1 restore
  .\scripts\qsf.ps1 restore latest
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `Invoke-Pester scripts/qsf.Tests.ps1 -Output Detailed`
Expected: all tests PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/qsf.ps1 scripts/qsf.Tests.ps1
git commit -m "Add restore command for sleep state backups"
```

### Task 3: Tab completion for `restore`

**Files:**
- Modify: `scripts/qsf-completion.ps1` (commands list at line 10; state-dir source at line 149; completer body at line 206)
- Test: `scripts/qsf-completion.Tests.ps1`

**Interfaces:**
- Consumes: backup directory layout from Task 1 (`state/backups/<leaf>-<timestamp>`).
- Produces: `Get-QsfCompletionBackupNames` → `@("latest") + backup dir names, newest first`.

- [ ] **Step 1: Write the failing tests** — append inside the existing Describe block of `scripts/qsf-completion.Tests.ps1`:

```powershell
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

        It "does not offer the backups root as a sleep state dir when it exists" {
            $completions = Complete-QsfInput -InputText ".\scripts\qsf.ps1 sleep -StateDir "

            $completions | Should -Contain "state/realtime"
            $completions | Should -Not -Contain "state/backups"
        }
    }
```

> Note: the hermetic `Context` overrides `$script:QsfCompletionProjectRoot` so the `state/backups` filter and backup-name completion are exercised against a `state/backups/…` directory that definitely exists, rather than passing vacuously on a clean checkout.

- [ ] **Step 2: Run tests to verify they fail**

Run: `Invoke-Pester scripts/qsf-completion.Tests.ps1 -Output Detailed`
Expected: the new tests FAIL — no `restore` command, no backup-name completion, and (in the hermetic `Context`) `state/backups` is offered as a state dir because the exclusion filter is not in place yet.

- [ ] **Step 3: Implement** in `scripts/qsf-completion.ps1`:

Add `"restore"` to `$script:QsfCompletionCommands` (after `"sleep"`).

Add after `Get-QsfCompletionStateDirs`:

```powershell
function Get-QsfCompletionBackupNames {
    $names = [System.Collections.Generic.List[string]]::new()
    $names.Add("latest")

    $backupRoot = Join-Path $script:QsfCompletionProjectRoot "state/backups"
    if (Test-Path -LiteralPath $backupRoot -PathType Container) {
        Get-ChildItem -LiteralPath $backupRoot -Directory -ErrorAction SilentlyContinue |
            Sort-Object CreationTime, Name -Descending |
            ForEach-Object { $names.Add($_.Name) }
    }

    return @($names)
}
```

In `Get-QsfCompletionStateDirs`, exclude the backups root by changing the return line to:

```powershell
    return @($paths | Where-Object { $_ -ne "state/backups" } | Sort-Object -Unique)
```

In the completer, add before the final `if` chain's end (alongside the `"ui"` case):

```powershell
        if ($nativeContext.Arguments.Count -eq 1 -and $nativeContext.Arguments[0] -eq "restore") {
            Select-QsfCompletionMatches -Values (Get-QsfCompletionBackupNames) -WordToComplete $wordToComplete
            return
        }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `Invoke-Pester scripts/qsf-completion.Tests.ps1 -Output Detailed`
Expected: all tests PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/qsf-completion.ps1 scripts/qsf-completion.Tests.ps1
git commit -m "Complete restore command and backup names in qsf completion"
```

---

## Phase 3 — Itemized change view (Rust)

### Task 4: `SleepChangeRecord` + pure renderer

**Files:**
- Create: `crates/qsf_app/src/sleep/change_view.rs`
- Modify: `crates/qsf_app/src/sleep/mod.rs`
- Test: unit tests inside `change_view.rs`

**Interfaces:**
- Produces (Task 5 depends on these exact names):

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SleepStateOutcome { ConsumedSession, AlreadyConsumed, NoPersistedSession }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NewMemoryChange { pub id: String, pub title: String, pub importance: f64 }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NewAssociationChange { pub from_id: String, pub to_id: String, pub weight: f64, pub reason: String }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrengthenedAssociationChange { pub from_id: String, pub to_id: String, pub old_weight: f64, pub new_weight: f64 }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SleepChangeRecord {
    pub state_outcome: SleepStateOutcome,
    pub session_id: Option<String>,
    pub state_dir: String,
    pub new_memories: Vec<NewMemoryChange>,
    pub skipped_duplicates: Vec<String>,
    pub new_associations: Vec<NewAssociationChange>,
    pub strengthened_associations: Vec<StrengthenedAssociationChange>,
    pub admitted_goal_id: Option<String>,
    pub declined_goal_candidate_id: Option<String>,
    pub swept_goal_ids: Vec<String>,
    pub open_question_count: usize,
    pub decision_candidate_count: usize,
    pub state_files_written: Vec<String>,
}

pub fn render_change_view(record: &SleepChangeRecord) -> String
```

- [ ] **Step 1: Write the failing tests** — create `crates/qsf_app/src/sleep/change_view.rs` with the type definitions above (add `use serde::{Deserialize, Serialize};`), a `todo!()`-free stub `pub fn render_change_view(record: &SleepChangeRecord) -> String { String::new() }`, and these tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn full_record() -> SleepChangeRecord {
        SleepChangeRecord {
            state_outcome: SleepStateOutcome::ConsumedSession,
            session_id: Some("realtime-session-1".to_string()),
            state_dir: "state/realtime".to_string(),
            new_memories: vec![NewMemoryChange {
                id: "memory.sleep.run-1.001".to_string(),
                title: "User prefers itemized views.".to_string(),
                importance: 0.8,
            }],
            skipped_duplicates: vec!["Reducers stay pure.".to_string()],
            new_associations: vec![NewAssociationChange {
                from_id: "memory.sleep.run-1.001".to_string(),
                to_id: "memory.a".to_string(),
                weight: 0.42,
                reason: "Both describe sleep UX.".to_string(),
            }],
            strengthened_associations: vec![StrengthenedAssociationChange {
                from_id: "memory.a".to_string(),
                to_id: "memory.c".to_string(),
                old_weight: 0.40,
                new_weight: 0.45,
            }],
            admitted_goal_id: Some("goal.continuity".to_string()),
            declined_goal_candidate_id: None,
            swept_goal_ids: vec![],
            open_question_count: 1,
            decision_candidate_count: 2,
            state_files_written: vec!["state/realtime/memory-store.json".to_string()],
        }
    }

    #[test]
    fn renders_all_sections_for_a_consumed_session() {
        let view = render_change_view(&full_record());

        assert!(view.contains("session `realtime-session-1`"));
        assert!(view.contains("Memories added (1):"));
        assert!(view.contains("+ memory.sleep.run-1.001"));
        assert!(view.contains("\"User prefers itemized views.\""));
        assert!(view.contains("(importance 0.80)"));
        assert!(view.contains("1 duplicate skipped: \"Reducers stay pure.\""));
        assert!(view.contains("+ memory.sleep.run-1.001 -> memory.a  (0.42)  Both describe sleep UX."));
        assert!(view.contains("~ memory.a -> memory.c  weight 0.40 -> 0.45"));
        assert!(view.contains("admitted `goal.continuity`"));
        assert!(view.contains("Open questions (1), decision candidates (2) - see sleep-report.md"));
        assert!(view.contains("state/realtime/memory-store.json"));
    }

    #[test]
    fn renders_no_change_placeholders_when_sections_are_empty() {
        let record = SleepChangeRecord {
            new_memories: vec![],
            skipped_duplicates: vec![],
            new_associations: vec![],
            strengthened_associations: vec![],
            admitted_goal_id: None,
            declined_goal_candidate_id: None,
            state_files_written: vec![],
            ..full_record()
        };

        let view = render_change_view(&record);

        assert!(view.contains("Memories added (0):"));
        assert!(view.contains("(none)"));
        assert!(view.contains("Goals:\n  (no changes)"));
    }

    #[test]
    fn already_consumed_states_that_nothing_changed() {
        let record = SleepChangeRecord {
            state_outcome: SleepStateOutcome::AlreadyConsumed,
            ..full_record()
        };

        let view = render_change_view(&record);

        assert!(view.contains("already consumed"));
        assert!(view.contains("state unchanged"));
    }

    #[test]
    fn no_persisted_session_states_smoke_input() {
        let record = SleepChangeRecord {
            state_outcome: SleepStateOutcome::NoPersistedSession,
            session_id: None,
            ..full_record()
        };

        let view = render_change_view(&record);

        assert!(view.contains("No persisted session"));
        assert!(view.contains("state unchanged"));
    }
}
```

Register the module in `crates/qsf_app/src/sleep/mod.rs`:

```rust
pub mod change_view;
```
and extend the re-exports:
```rust
pub use change_view::{
    NewAssociationChange, NewMemoryChange, SleepChangeRecord, SleepStateOutcome,
    StrengthenedAssociationChange, render_change_view,
};
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p qsf_app change_view`
Expected: 4 FAILED (renderer returns empty string).

- [ ] **Step 3: Implement `render_change_view`:**

```rust
pub fn render_change_view(record: &SleepChangeRecord) -> String {
    let mut view = String::new();

    match record.state_outcome {
        SleepStateOutcome::NoPersistedSession => {
            view.push_str("Sleep update - No persisted session to consume; ran the smoke-test summarization only. State unchanged.\n");
            return view;
        }
        SleepStateOutcome::AlreadyConsumed => {
            view.push_str(&format!(
                "Sleep update - session `{}` was already consumed; state unchanged.\n",
                record.session_id.as_deref().unwrap_or("unknown")
            ));
            return view;
        }
        SleepStateOutcome::ConsumedSession => {
            view.push_str(&format!(
                "Sleep update - session `{}`\n",
                record.session_id.as_deref().unwrap_or("unknown")
            ));
        }
    }

    view.push_str(&format!("\nMemories added ({}):\n", record.new_memories.len()));
    if record.new_memories.is_empty() {
        view.push_str("  (none)\n");
    }
    for memory in &record.new_memories {
        view.push_str(&format!(
            "  + {}  \"{}\"  (importance {:.2})\n",
            memory.id, memory.title, memory.importance
        ));
    }
    match record.skipped_duplicates.len() {
        0 => {}
        1 => view.push_str(&format!(
            "  = 1 duplicate skipped: \"{}\"\n",
            record.skipped_duplicates[0]
        )),
        count => view.push_str(&format!(
            "  = {count} duplicates skipped: {}\n",
            record
                .skipped_duplicates
                .iter()
                .map(|title| format!("\"{title}\""))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }

    view.push_str("\nAssociations:\n");
    if record.new_associations.is_empty() && record.strengthened_associations.is_empty() {
        view.push_str("  (none)\n");
    }
    for association in &record.new_associations {
        view.push_str(&format!(
            "  + {} -> {}  ({:.2})  {}\n",
            association.from_id, association.to_id, association.weight, association.reason
        ));
    }
    for strengthened in &record.strengthened_associations {
        view.push_str(&format!(
            "  ~ {} -> {}  weight {:.2} -> {:.2}\n",
            strengthened.from_id,
            strengthened.to_id,
            strengthened.old_weight,
            strengthened.new_weight
        ));
    }

    view.push_str("\nGoals:\n");
    let mut goal_lines = Vec::new();
    if let Some(admitted) = &record.admitted_goal_id {
        goal_lines.push(format!("  admitted `{admitted}`"));
    }
    if let Some(declined) = &record.declined_goal_candidate_id {
        goal_lines.push(format!("  declined candidate `{declined}`"));
    }
    if !record.swept_goal_ids.is_empty() {
        goal_lines.push(format!("  swept: {}", record.swept_goal_ids.join(", ")));
    }
    if goal_lines.is_empty() {
        view.push_str("  (no changes)\n");
    } else {
        for line in goal_lines {
            view.push_str(&line);
            view.push('\n');
        }
    }

    view.push_str(&format!(
        "\nOpen questions ({}), decision candidates ({}) - see sleep-report.md\n",
        record.open_question_count, record.decision_candidate_count
    ));

    view.push_str(&format!("\nState files written under {}:\n", record.state_dir));
    if record.state_files_written.is_empty() {
        view.push_str("  (none)\n");
    }
    for file in &record.state_files_written {
        view.push_str(&format!("  {file}\n"));
    }

    view
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p qsf_app change_view`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/qsf_app/src/sleep/change_view.rs crates/qsf_app/src/sleep/mod.rs
git commit -m "Add sleep change record and pure console renderer"
```

### Task 5: Populate the record in the sleep pipeline and print it

**Files:**
- Modify: `crates/qsf_app/src/sleep/update.rs` (`SleepUpdateRunSummary` at line 48; `run_sleep_update` at line 59; `run_sleep_update_with_context` at line 155; `commit_cross_session_sleep` at line 278; tests at line 766)
- Modify: `crates/qsf_app/src/cli.rs:75-85`

**Interfaces:**
- Consumes: `SleepChangeRecord`, `SleepStateOutcome`, `NewMemoryChange`, `NewAssociationChange`, `StrengthenedAssociationChange`, `render_change_view` from Task 4.
- Produces: `SleepUpdateRunSummary` gains `pub change_record: SleepChangeRecord`; its derive changes from `Eq, PartialEq` to `PartialEq` (f64 weights). `commit_cross_session_sleep` and `run_sleep_update_with_context` return `anyhow::Result<(SleepUpdateOutcome, SleepChangeRecord)>`. `run_sleep_update` writes `sleep-changes.json` into the run dir.

- [ ] **Step 1: Write the failing tests** — in `crates/qsf_app/src/sleep/update.rs` tests:

Extend `sleep_update_command_consumes_a_pending_session` (after the existing asserts, before `remove_dir_all`):

```rust
        use crate::sleep::change_view::SleepStateOutcome;

        assert_eq!(
            summary.change_record.state_outcome,
            SleepStateOutcome::ConsumedSession
        );
        assert_eq!(
            summary.change_record.session_id.as_deref(),
            Some("realtime-session-to-sleep")
        );
        use crate::sleep::change_view::SleepChangeRecord;

        let changes_path = summary.run_dir.join("sleep-changes.json");
        assert!(changes_path.exists());
        // Parse as the structured record (not untyped Value) so the durable
        // artifact cannot drift from `SleepChangeRecord`'s shape. Also assert the
        // written file round-trips back to the in-memory record.
        let parsed: SleepChangeRecord =
            serde_json::from_str(&fs::read_to_string(&changes_path).unwrap()).unwrap();
        assert_eq!(parsed.state_outcome, SleepStateOutcome::ConsumedSession);
        assert_eq!(parsed.session_id.as_deref(), Some("realtime-session-to-sleep"));
        // At least one nested vector field must survive the round-trip.
        assert_eq!(parsed.new_memories, summary.change_record.new_memories);
        assert_eq!(parsed, summary.change_record);

        // A second run over the same state must report AlreadyConsumed and write
        // nothing. The process is still in `base_dir` from the first run, so the
        // relative `state/realtime` resolves correctly; do not restore cwd yet.
        let second = run_sleep_update(SleepUpdateOptions {
            state_dir: std::path::PathBuf::from("state/realtime"),
            requested_provider: "mock".to_string(),
            workspace_root: None,
        })
        .unwrap();
        assert_eq!(
            second.change_record.state_outcome,
            SleepStateOutcome::AlreadyConsumed
        );
        assert!(second.change_record.state_files_written.is_empty());

        // Restore the captured original cwd before cleanup — never leave the test
        // process in `base_dir` or the system temp dir.
        std::env::set_current_dir(&cwd).unwrap();
```

Note on cwd handling: remove the existing `std::env::set_current_dir(cwd).unwrap();` restore that currently runs immediately after the first run — the process must stay in `base_dir` so the second run's relative `state/realtime` resolves there. Change the first-run restore to borrow the captured path (`std::env::set_current_dir(&cwd).unwrap();` is only needed once, at the very end, as shown above) so `cwd` remains valid. The manifest/field asserts between the two runs read absolute `state_dir` paths and are unaffected by cwd. `fs::remove_dir_all(base_dir)` then runs with the original cwd restored.

Extend `commit_cross_session_sleep_promotes_candidates`: change the destructuring to `let (outcome, change_record) = super::commit_cross_session_sleep(...)` and add:

```rust
        assert_eq!(change_record.new_memories.len(), 1);
        assert_eq!(change_record.new_memories[0].title, "Candidate memory.");
        assert!((change_record.new_memories[0].importance - 0.8).abs() < 1e-9);
        assert_eq!(change_record.open_question_count, 0);
        assert!(
            change_record
                .state_files_written
                .iter()
                .any(|file| file.ends_with("memory-store.json"))
        );
```

Also add a focused test proving `skipped_duplicates` flows into the record (the renderer displays it, so a missing copy would silently show nothing). Seed the state dir's `memory-store.json` with a record whose title equals the candidate summary before committing — mirror the store seeding in `auto_promote`'s `skips_duplicates_of_existing_store_records` test:

```rust
    #[test]
    fn commit_cross_session_sleep_records_skipped_duplicates() {
        // ... same base_dir/state_dir/session/manifest setup as
        // commit_cross_session_sleep_promotes_candidates ...

        // Seed an existing memory whose title matches the report candidate so
        // auto-promotion skips it as a duplicate.
        let store_path = state_dir.join("memory-store.json");
        let mut store = crate::memory::MemoryStore::load_or_empty(&store_path).unwrap();
        store.append_records([/* a MemoryRecord titled "Candidate memory." */]);
        store.persist().unwrap();

        let (_outcome, change_record) = super::commit_cross_session_sleep(
            &mut context,
            "mock",
            &SleepReport {
                // one memory_candidate with summary "Candidate memory."
                ..
            },
            /* outcome */,
            /* state_resolution */,
        )
        .unwrap();

        assert!(change_record.new_memories.is_empty());
        assert!(
            change_record
                .skipped_duplicates
                .iter()
                .any(|title| title == "Candidate memory.")
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p qsf_app sleep::update`
Expected: compile errors (`change_record` field and tuple return do not exist yet) — that is the failing state for structural changes.

- [ ] **Step 3: Implement** in `update.rs`:

1. Imports: `use crate::sleep::change_view::{NewAssociationChange, NewMemoryChange, SleepChangeRecord, SleepStateOutcome, StrengthenedAssociationChange};` — only the types; `render_change_view` is used from `cli.rs`, not here.
2. `SleepUpdateRunSummary`: change derive to `#[derive(Clone, Debug, PartialEq)]` and add `pub change_record: SleepChangeRecord`.
3. `run_sleep_update_with_context` returns `anyhow::Result<(SleepUpdateOutcome, SleepChangeRecord)>`; its early path passes a base record through `commit_cross_session_sleep`.
4. `commit_cross_session_sleep` signature becomes:

```rust
pub(crate) fn commit_cross_session_sleep(
    context: &mut RunContext,
    requested_provider: &str,
    report: &SleepReport,
    mut outcome: SleepUpdateOutcome,
    state_resolution: &StateDirectoryResolution,
) -> anyhow::Result<(SleepUpdateOutcome, SleepChangeRecord)> {
```

Build the record at each exit:

- `previous_session == None` →

```rust
        return Ok((outcome, SleepChangeRecord {
            state_outcome: SleepStateOutcome::NoPersistedSession,
            session_id: None,
            state_dir: state_resolution.persist_state_dir.display().to_string(),
            new_memories: vec![],
            skipped_duplicates: vec![],
            new_associations: vec![],
            strengthened_associations: vec![],
            admitted_goal_id: None,
            declined_goal_candidate_id: None,
            swept_goal_ids: vec![],
            open_question_count: report.open_questions.len(),
            decision_candidate_count: report.decision_candidates.len(),
            state_files_written: vec![],
        }));
```

- already-consumed branch → same shape with `state_outcome: SleepStateOutcome::AlreadyConsumed` and `session_id: Some(session.session_id.clone())`.
- main path: capture strengthened old weights inside the existing mutation loop (read `existing.weight` before assigning):

```rust
    let mut strengthened_changes = Vec::new();
    for (from_id, to_id, new_weight) in &plan.strengthened_associations {
        if let Some(existing) = /* existing iter_mut().find(...) */ {
            strengthened_changes.push(StrengthenedAssociationChange {
                from_id: from_id.clone(),
                to_id: to_id.clone(),
                old_weight: existing.weight,
                new_weight: *new_weight,
            });
            existing.weight = *new_weight;
            existing.last_reinforced_at = as_of;
        }
    }
```

Map the plan into the record before `plan` is consumed:

```rust
    let new_memories = plan
        .new_records
        .iter()
        .map(|record| NewMemoryChange {
            id: record.id.clone(),
            title: record.title.clone(),
            importance: record.importance,
        })
        .collect::<Vec<_>>();
    let new_association_changes = plan
        .new_associations
        .iter()
        .map(|association| NewAssociationChange {
            from_id: association.from_memory_id.clone(),
            to_id: association.to_memory_id.clone(),
            weight: association.weight,
            reason: association.reason.clone(),
        })
        .collect::<Vec<_>>();
    let skipped_duplicates = plan.skipped_duplicates.clone();
```

When building the record for the main (consumed-session) path, set `skipped_duplicates: skipped_duplicates` (clone captured before `plan` is consumed by `append_records`/`append_associations`). The renderer already displays this field and its tests assert it, so the record must carry it through — do not leave it as `vec![]`.

Goal fields come from the existing `maintenance` result (record them where the observations are pushed today; keep the observations too). `state_files_written` collects display paths for: `memory-store.json`, `consolidated-brief.json`, `archive/sleep-<run>.json`, `continuity-manifest.json` (all under the state dir), the volition continuity report json/md when written, and `reviewed-memory-draft.json`/`.md` (run dir) when decision candidates exist — mirroring the existing `extra_artifacts` pushes.

Return `Ok((outcome, change_record))`.

5. `run_sleep_update`: destructure `let (outcome, change_record) = ...`, write the artifact before building the summary:

```rust
    let changes_path = context.run_dir().join("sleep-changes.json");
    fs::write(&changes_path, serde_json::to_string_pretty(&change_record)?).with_context(|| {
        format!(
            "failed to write sleep changes JSON for run {}",
            context.run_id()
        )
    })?;
```

and add `change_record` to the returned `SleepUpdateRunSummary`. Note the error branch of `run_sleep_update` matches on the tuple now: `Ok((outcome, change_record)) => ...`.

6. `cli.rs` sleep arm — print the view before the completion line:

```rust
            println!("{}", crate::sleep::render_change_view(&summary.change_record));
            println!(
                "Sleep update completed. State: {}. Run artifacts: {}",
                summary.state_dir.display(),
                summary.run_dir.display()
            );
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p qsf_app`
Expected: all tests pass, including the extended update tests and Task 4 renderer tests.

- [ ] **Step 5: Commit**

```bash
git add crates/qsf_app/src/sleep/update.rs crates/qsf_app/src/cli.rs
git commit -m "Report an itemized change view from the sleep command"
```

---

## Phase 4 — Docs and final checks

### Task 6: Documentation and verification

**Files:**
- Modify: `docs/Architecture/Architecture.SleepPhase.md` (Manual Sleep section, ~line 191-206; refresh the Implementation Status `Last reviewed:` date)
- Modify: `docs/DecisionLog.md` (append)

**Interfaces:** none (documentation).

- [ ] **Step 1: Update `Architecture.SleepPhase.md`** — after the operational manual trigger code block (line 206), add:

```markdown
Before the launcher-driven run, `qsf.ps1 sleep` copies the state directory to
`state/backups/<name>-<timestamp>/` (keeping the newest five), and
`.\scripts\qsf.ps1 restore [<backup>|latest]` rolls the state directory back;
a restore backs up the current state first, so it is itself undoable. After the
run, the command prints an itemized change view (memories added, associations
added/strengthened, goal changes, files written) and writes the same data as a
`sleep-changes.json` run artifact.
```

Refresh the document's `Last reviewed:` date to 2026-07-05.

- [ ] **Step 2: Append the DecisionLog entry:**

```markdown
## 2026-07-05 - Sleep launcher backs up state and reports an itemized change view

Decision: `qsf.ps1 sleep` backs up the target state directory to
`state/backups/<name>-<timestamp>/` (keeping the newest five) before invoking the
sleep update, and `qsf.ps1 restore` rolls back from those backups, backing up the
current state first so a restore is undoable. The sleep command reports an itemized
change view (memories added, associations added/strengthened, goal changes, state
files written) rendered from a structured `SleepChangeRecord` that is also written
as a `sleep-changes.json` run artifact. Rollback safety was chosen over a `--dry-run`
mode: sleep output depends on a live model call either way, and a backup keeps the
real run as the single code path instead of maintaining a plan/apply split.

Context: Sleep auto-applies memory promotion, association changes, and goal
maintenance (2026-05-20, 2026-05-22, 2026-07-02), so an operator had no way to
preview or undo a bad consolidation, and the command reported only artifact paths.

Consequences: Operators can run sleep freely and roll back regretted consolidations.
The change view and `sleep-changes.json` make each sleep run reviewable at a glance;
backups live inside the git-ignored `state/` tree. The launcher remains the
supported operator surface for backup/restore; raw `cargo run -p qsf_app -- sleep`
does not create backups.
```

- [ ] **Step 3: Full verification**

```powershell
cargo build
cargo test -p qsf_app
cargo clippy --all-targets -- -D warnings
cargo fmt
Invoke-Pester scripts/qsf.Tests.ps1 -Output Detailed
Invoke-Pester scripts/qsf-completion.Tests.ps1 -Output Detailed
```

Expected: everything green.

- [ ] **Step 4: Human test (recommended)**

```powershell
.\scripts\qsf.ps1 sleep -Provider mock    # against real state/realtime
.\scripts\qsf.ps1 restore                  # list shows the new backup
.\scripts\qsf.ps1 restore latest           # round-trips the state dir
```

Confirm: backup line printed before the run, itemized view printed after, restore undoes the sleep commit (manifest `sleep_pending` back to its pre-sleep value).

- [ ] **Step 5: Commit**

```bash
git add docs/Architecture/Architecture.SleepPhase.md docs/DecisionLog.md
git commit -m "Document sleep state backups, restore, and the change view"
```
