<#
.SYNOPSIS
  Differential test of rustty.exe against Ghostty's exe at the pin, on Windows.

.DESCRIPTION
  WHAT "RUNNING RUSTTY ON WINDOWS" MEANS AT THE PIN (.port/tools/gates/ORACLE_PIN)

  Ghostty has no Windows GUI at the pin. `src/apprt/runtime.zig` picks the
  `.none` app runtime everywhere but Linux/FreeBSD, and with `.none`
  `src/main_ghostty.zig` makes the exe a HELPER CLI: `+<action>` runs and exits;
  with no action it prints "Usage: ghostty +<action> [flags] ... This is the
  Ghostty helper CLI ..." and exits. No window ever opens. `ghostty_init` (the
  embedded C API) fails with UnsupportedOSForCApi on Windows. Rustty is the
  same: `rustty.exe` is the helper CLI and nothing else.

  The exe is a GUI-subsystem binary (`GhosttyExe.zig`: `exe.subsystem =
  .Windows` plus `dist/windows/ghostty.rc`). So:
    * double-clicking it, or typing `rustty +version` at a PowerShell or cmd
      prompt, shows NOTHING: Windows gives a GUI-subsystem process no console,
      the shell does not wait for it, and the output is lost;
    * its output can be seen only when it is redirected or piped:
      `rustty +version | Out-String`, `cmd /c "rustty +version > out.txt"`,
      or the process-level redirection this harness uses.
  Ghostty's exe at the pin behaves the same way (measured on Windows 11
  ARM64, x64 emulation, task W-4).

  HOW THE REFERENCE EXE IS OBTAINED (regeneration recipe for the goldens)

  The pin never INSTALLS ghostty.exe with app-runtime=none (build.zig: the exe
  is installed only when `app_runtime != .none`). Forcing it needs two
  one-line scratch changes. Without them the exe cannot be built at the pin:
    1. build.zig, in the `app_runtime == .none` branch: `if (config.emit_exe)
       exe.install();` (install the exe the build already defines);
    2. src/main_ghostty.zig:78: `std.io.getStdOut().writer()` does not exist
       in Zig 0.16. It becomes
       `var w = std.Io.File.stdout().writer(std.Io.Threaded.global_single_threaded.io(), &.{});
        const stdout = &w.interface;` (unbuffered, like 0.14's writer).
  With the reference's default MSVC ABI the exe then fails to link
  (`lld-link: undefined symbol: WinMain`). The GNU ABI is what the
  reference's own Windows CI builds (`build-libghostty-windows-gnu`), so:
    zig build -Dtarget=native-native-gnu -Dapp-runtime=none -Doptimize=ReleaseFast --prefix <dir>
  Use Zig 0.16.0 for x86_64-windows. The aarch64-windows Zig 0.16.0 crashed
  (0xC0000005) on every compile on the W-4 host. The patch is kept beside
  this script (`ghostty-exe-at-pin.patch`). Then:
    exe-differential.ps1 -Mode capture -Exe <dir>\bin\ghostty.exe -Golden .port\tools\windows\golden
  The goldens hold the reference's output after normalisation.

  MODES
    diff     run -Exe (rustty.exe) and -RefExe (ghostty.exe) over the matrix and
             compare the normalised stdout, stderr and exit code of each case.
    capture  run -Exe (the REFERENCE exe) and write its normalised results into
             -Golden (one .stdout/.stderr/.exit per case).
    check    run -Exe (rustty.exe) and compare with the goldens in -Golden (CI).

  NORMALISATION (only this, nothing else):
    * on the REFERENCE's output only, the ghostty -> rustty rename
      (ghostty/Ghostty/GHOSTTY). rustty.exe's output is never renamed, so a
      "ghostty" it prints is a difference (R2; review W-4 F3);
    * the version string each exe reports in `+version` becomes <VERSION>, and
      the toolchain line ("Zig version   : 0.16.0" / "Rust version  : 1.93.1 (..)")
      becomes "<TOOLCHAIN> version: <TOOLCHAIN_VERSION>";
    * the per-run scratch directory becomes <WORK>.
  Two golden lines hold per-host settings, which the goldens keep as
  placeholders and -Mode check fills with THIS host's values (review W-4 F4):
    * <HOST_DOUBLE_CLICK_MS>: `click-repeat-interval` in `+show-config`, which
      the config takes from `GetDoubleClickTime` (a per-user setting);
    * <HOST_SETLOCALE>: the `setlocale from env result=` log line, which is the
      C runtime's `setlocale(LC_ALL, "")` (the user's locale and code page).

  Each case runs with stdin empty, stdout and stderr captured as bytes, a fresh
  LOCALAPPDATA (Ghostty's config/state/cache root on Windows, `os/xdg.zig`),
  and XDG_*, GHOSTTY_* and RUSTTY_* removed from the environment. It also
  checks: the PE subsystem (2, GUI); how the exe behaves when "opened" (shell
  start, no console, no redirection): it must exit promptly and leave no
  process; how it behaves with an inherited console it cannot write to; and
  that Windows Error Reporting recorded no crash of it.

  Exit status: 0 when every case matches or is listed in $Justified, 1 otherwise.
#>
param(
  [Parameter(Mandatory = $true)][string]$Exe,
  [string]$RefExe,
  [string]$Golden,
  [ValidateSet("diff", "capture", "check")][string]$Mode = "diff",
  [string]$Work = (Join-Path ([IO.Path]::GetTempPath()) "rustty-exe-diff"),
  [int]$TimeoutSec = 60
)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version 2

# Differences that remain after normalisation and are recorded as justified,
# keyed by case name. Every entry names its ledger record.
$Justified = @{
  # R13 fixes: Rustty does what Ghostty meant (docs/ghostty-bug-fixes.md).
  "list-colors"             = @{ Streams = @("stdout"); Why = "UD22: the reference never flushes the end of the list" }
  "list-colors-plain"       = @{ Streams = @("stdout"); Why = "UD22: the reference never flushes the end of the list" }
  "list-themes-plain"       = @{ Streams = @("stderr"); Why = "UD23: the reference never flushes its two messages" }
  "toggle-quick-terminal"   = @{ Streams = @("stderr"); Why = "UD340: the reference never flushes its message" }
  "edit-config"             = @{ Streams = @("stderr"); Why = "UD342: the reference announces the config path and prints none" }
  "validate-config-default" = @{ Streams = @("exit"); Why = "UD341: the reference exits 1 on a pipe (File.Writer.end() -> FileTooBig)" }
  "validate-config-valid"   = @{ Streams = @("exit"); Why = "UD341: the reference exits 1 on a pipe (File.Writer.end() -> FileTooBig)" }
  "list-fonts"              = @{ Streams = @("stdout"); Why = "UD344: the reference prints UTF-16BE family names raw (\0A\0r\0i\0a\0l)" }
  "list-fonts-family"       = @{ Streams = @("stdout"); Why = "UD344: the reference prints UTF-16BE family names raw (\0C\0o\0n\0s\0o\0l\0a\0s)" }
  # R2 identity: what the rename changes beyond the normalised spelling.
  "show-config-default"      = @{ Streams = @("stdout"); Why = "R2: the palette entry titled with the product name sorts under G in the reference, R in Rustty" }
  "show-config-default-docs" = @{ Streams = @("stdout"); Why = "R2: the palette entry's sort position, and the class default com.mitchellh.ghostty vs io.airely.rustty" }
}

# ---------------------------------------------------------------- the matrix
# Name -> argument list. {VALID}/{INVALID}/{MISSING} expand to config files
# under the scratch directory. An element "ENV:<NAME>=<value>" is not an
# argument: it sets GHOSTTY_<NAME> for the reference exe and RUSTTY_<NAME> for
# rustty.exe (the exe's file name picks the prefix).
$Cases = [ordered]@{
  "no-args"                      = @()
  "version"                      = @("+version")
  "version-dashdash"             = @("--version")
  "version-help"                 = @("+version", "--help")
  "help"                         = @("+help")
  "help-dashdash"                = @("--help")
  "help-h"                       = @("-h")
  "list-fonts"                   = @("+list-fonts")
  "list-fonts-family"            = @("+list-fonts", "--family=Consolas")
  "list-themes-plain"            = @("+list-themes", "--plain")
  "list-keybinds"                = @("+list-keybinds")
  "list-keybinds-default-plain"  = @("+list-keybinds", "--default", "--plain")
  "list-colors"                  = @("+list-colors")
  "list-colors-plain"            = @("+list-colors", "--plain")
  "list-actions"                 = @("+list-actions")
  "list-actions-docs"            = @("+list-actions", "--docs")
  "show-config"                  = @("+show-config")
  "show-config-default"          = @("+show-config", "--default")
  "show-config-default-docs"     = @("+show-config", "--default", "--docs")
  "explain-config-option"        = @("+explain-config", "--option=font-size")
  "explain-config-positional"    = @("+explain-config", "font-size")
  "explain-config-keybind"       = @("+explain-config", "--keybind=copy_to_clipboard")
  "explain-config-unknown"       = @("+explain-config", "--option=no-such-option")
  "explain-config-none"          = @("+explain-config")
  "show-config-help"             = @("+show-config", "--help")
  "log-stderr-version"           = @("ENV:LOG=stderr", "+version")
  "log-stderr-list-actions"      = @("ENV:LOG=stderr", "+list-actions")
  "validate-config-default"      = @("+validate-config")
  "validate-config-valid"        = @("+validate-config", "--config-file={VALID}")
  "validate-config-invalid"      = @("+validate-config", "--config-file={INVALID}")
  "validate-config-missing"      = @("+validate-config", "--config-file={MISSING}")
  "show-face-cp"                 = @("+show-face", "--cp=65")
  "show-face-string"             = @("+show-face", "--string=A")
  "show-face-none"               = @("+show-face")
  "unknown-action"               = @("+no-such-action")
  "two-actions"                  = @("+version", "+help")
  "bad-flag-no-action"           = @("--no-such-flag")
  "bad-flag-version"             = @("+version", "--no-such-flag")
  "bad-flag-list-actions"        = @("+list-actions", "--no-such-flag")
  "ssh-cache"                    = @("+ssh-cache")
  "ssh-cache-list"               = @("+ssh-cache", "--list")
  "ssh-cache-host"               = @("+ssh-cache", "--host=example.invalid")
  "ssh-cache-add"                = @("+ssh-cache", "--add=user@example.invalid")
  "ssh-none"                     = @("+ssh")
  "ssh-bad-flag"                 = @("+ssh", "--no-such-flag")
  "edit-config"                  = @("+edit-config")
  "crash-report"                 = @("+crash-report")
  "new-window"                   = @("+new-window")
  "new-tab"                      = @("+new-tab")
  "toggle-quick-terminal"        = @("+toggle-quick-terminal")
}

# Cases whose stdout lists what the HOST has (the installed fonts). -Mode diff
# (both exes on one host) compares them in full; -Mode check (goldens from
# another host) compares their stderr and exit code only, and says so. Their
# stdout is justified (UD344), so check mode instead requires Rustty's listing
# to be non-empty and free of NUL bytes: the reference's raw UTF-16 names are
# what UD344 fixes.
$HostDependentStdout = @("list-fonts", "list-fonts-family")

# ---------------------------------------------------------------- helpers
function Get-PeSubsystem([string]$Path) {
  $b = [IO.File]::ReadAllBytes($Path)
  $pe = [BitConverter]::ToInt32($b, 0x3c)
  return [BitConverter]::ToUInt16($b, $pe + 24 + 68)
}

function Quote-Arg([string]$a) {
  if ($a -eq "" -or $a -match '[\s"]') { return '"' + ($a -replace '"', '\"') + '"' }
  return $a
}

function New-RunDir([string]$Tag) {
  $d = Join-Path $Work $Tag
  if (Test-Path $d) { Remove-Item -Recurse -Force $d }
  New-Item -ItemType Directory -Force (Join-Path $d "appdata") | Out-Null
  New-Item -ItemType Directory -Force (Join-Path $d "home") | Out-Null
  [IO.File]::WriteAllText((Join-Path $d "valid.conf"), "font-size = 13`nbackground = #101010`n")
  [IO.File]::WriteAllText((Join-Path $d "invalid.conf"), "font-size = not-a-number`nno-such-key = 1`n")
  return $d
}

function Invoke-Case([string]$ExePath, [string[]]$CaseArgs, [string]$RunDir) {
  $envs = @($CaseArgs | Where-Object { $_.StartsWith("ENV:") })
  $CaseArgs = @($CaseArgs | Where-Object { -not $_.StartsWith("ENV:") })
  $expanded = @($CaseArgs | ForEach-Object {
      $_.Replace("{VALID}", (Join-Path $RunDir "valid.conf")).
         Replace("{INVALID}", (Join-Path $RunDir "invalid.conf")).
         Replace("{MISSING}", (Join-Path $RunDir "missing.conf"))
    })
  $psi = New-Object Diagnostics.ProcessStartInfo
  $psi.FileName = $ExePath
  $psi.Arguments = ($expanded | ForEach-Object { Quote-Arg $_ }) -join " "
  $psi.UseShellExecute = $false
  $psi.CreateNoWindow = $true
  $psi.RedirectStandardInput = $true
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.WorkingDirectory = $RunDir
  foreach ($k in @($psi.EnvironmentVariables.Keys)) {
    if ($k -match '^(XDG_|GHOSTTY_|RUSTTY_)') { $psi.EnvironmentVariables.Remove($k) }
  }
  $psi.EnvironmentVariables["LOCALAPPDATA"] = (Join-Path $RunDir "appdata")
  # The home directory (`os/homedir.zig` homeWindows: HOMEDRIVE + HOMEPATH) is
  # the default `working-directory`, which `+show-config` prints.
  $homeDir = Join-Path $RunDir "home"
  $psi.EnvironmentVariables["HOMEDRIVE"] = [IO.Path]::GetPathRoot($homeDir).TrimEnd("\")
  $psi.EnvironmentVariables["HOMEPATH"] = $homeDir.Substring([IO.Path]::GetPathRoot($homeDir).Length - 1)
  $psi.EnvironmentVariables["USERPROFILE"] = $homeDir
  $prefix = if ([IO.Path]::GetFileName($ExePath) -like "ghostty*") { "GHOSTTY_" } else { "RUSTTY_" }
  foreach ($e in $envs) {
    $kv = $e.Substring(4) -split "=", 2
    $psi.EnvironmentVariables[$prefix + $kv[0]] = $kv[1]
  }
  $p = [Diagnostics.Process]::Start($psi)
  $p.StandardInput.Close()
  $so = New-Object IO.MemoryStream
  $se = New-Object IO.MemoryStream
  $t1 = $p.StandardOutput.BaseStream.CopyToAsync($so)
  $t2 = $p.StandardError.BaseStream.CopyToAsync($se)
  $exit = $null
  if ($p.WaitForExit($TimeoutSec * 1000)) {
    $p.WaitForExit()
    [void]$t1.Wait(10000); [void]$t2.Wait(10000)
    $exit = "$($p.ExitCode)"
  } else {
    try { $p.Kill() } catch { }
    [void]$t1.Wait(10000); [void]$t2.Wait(10000)
    $exit = "TIMEOUT"
  }
  $utf8 = New-Object Text.UTF8Encoding $false
  return [pscustomobject]@{
    Stdout = $utf8.GetString($so.ToArray())
    Stderr = $utf8.GetString($se.ToArray())
    Exit   = $exit
  }
}

function Get-ReportedVersion([string]$ExePath) {
  $d = New-RunDir "version-probe"
  $r = Invoke-Case $ExePath @("+version") $d
  $m = [regex]::Match($r.Stdout, '(?m)^  - version: (\S+)\r?$')
  if (-not $m.Success) { throw "cannot read the version from '$ExePath +version' (exit $($r.Exit)): $($r.Stdout)$($r.Stderr)" }
  return $m.Groups[1].Value
}

function Normalize([string]$Text, [string]$Version, [string]$RunDir, [bool]$IsReference) {
  if ($Version) { $Text = $Text.Replace($Version, "<VERSION>") }
  foreach ($d in @($RunDir, $RunDir.Replace("\", "/"))) {
    $Text = [regex]::Replace($Text, [regex]::Escape($d), "<WORK>", "IgnoreCase")
  }
  $Text = [regex]::Replace($Text, '(?m)^  - (Zig|Rust) version +: [^\r\n]*', '  - <TOOLCHAIN> version: <TOOLCHAIN_VERSION>')
  if ($IsReference) {
    $Text = $Text.Replace("GHOSTTY", "RUSTTY").Replace("Ghostty", "Rustty").Replace("ghostty", "rustty")
  }
  return $Text
}

function Run-Matrix([string]$ExePath, [string]$Tag, [bool]$IsReference) {
  $ver = Get-ReportedVersion $ExePath
  $results = [ordered]@{}
  foreach ($name in $Cases.Keys) {
    $d = New-RunDir "$Tag-$name"
    $r = Invoke-Case $ExePath $Cases[$name] $d
    $results[$name] = [pscustomobject]@{
      Stdout = Normalize $r.Stdout $ver $d $IsReference
      Stderr = Normalize $r.Stderr $ver $d $IsReference
      Exit   = $r.Exit
    }
  }
  return $results
}

# This host's double-click time and C-runtime locale (see NORMALISATION).
function Get-HostValues {
  Add-Type -AssemblyName System.Windows.Forms
  if (-not ("W4Crt.Locale" -as [type])) {
    Add-Type -Namespace W4Crt -Name Locale -MemberDefinition '[DllImport("ucrtbase.dll", CharSet = CharSet.Ansi)] public static extern IntPtr setlocale(int category, string locale);'
  }
  $loc = [Runtime.InteropServices.Marshal]::PtrToStringAnsi([W4Crt.Locale]::setlocale(0, ""))
  return @{ DoubleClick = "$([System.Windows.Forms.SystemInformation]::DoubleClickTime)"; Locale = $loc }
}

function To-HostPlaceholders([string]$Text, $HostValues) {
  $Text = [regex]::Replace($Text, '(?m)^click-repeat-interval = ' + [regex]::Escape($HostValues.DoubleClick) + '(?=\r?$)', 'click-repeat-interval = <HOST_DOUBLE_CLICK_MS>')
  return $Text.Replace("setlocale from env result=$($HostValues.Locale)`n", "setlocale from env result=<HOST_SETLOCALE>`n")
}

function From-HostPlaceholders([string]$Text, $HostValues) {
  return $Text.Replace("<HOST_DOUBLE_CLICK_MS>", $HostValues.DoubleClick).Replace("<HOST_SETLOCALE>", $HostValues.Locale)
}

function Read-Golden([string]$Dir) {
  $hostValues = Get-HostValues
  $utf8 = New-Object Text.UTF8Encoding $false
  $results = [ordered]@{}
  foreach ($name in $Cases.Keys) {
    $b = Join-Path $Dir $name
    if (-not (Test-Path "$b.exit")) { throw "golden '$b.exit' is missing; regenerate with -Mode capture" }
    $results[$name] = [pscustomobject]@{
      Stdout = From-HostPlaceholders ([IO.File]::ReadAllText("$b.stdout", $utf8)) $hostValues
      Stderr = From-HostPlaceholders ([IO.File]::ReadAllText("$b.stderr", $utf8)) $hostValues
      Exit   = [IO.File]::ReadAllText("$b.exit", $utf8).Trim()
    }
  }
  return $results
}

function Show-Diff([string]$Label, [string]$A, [string]$B) {
  $la = $A -split "`n"; $lb = $B -split "`n"
  $n = [Math]::Max($la.Count, $lb.Count)
  $shown = 0
  for ($i = 0; $i -lt $n -and $shown -lt 12; $i++) {
    $x = if ($i -lt $la.Count) { $la[$i] } else { "<EOF>" }
    $y = if ($i -lt $lb.Count) { $lb[$i] } else { "<EOF>" }
    if ($x -ne $y) {
      Write-Host "      $Label line $($i + 1):"
      Write-Host "        reference: $($x.TrimEnd("`r"))"
      Write-Host "        rustty   : $($y.TrimEnd("`r"))"
      $shown++
    }
  }
}

function Compare-Results($Ref, $Got, [bool]$OtherHost = $false) {
  $bad = 0
  Write-Host ("{0,-30} {1,-6} {2,-6} {3,-12} {4}" -f "case", "stdout", "stderr", "exit(ref/rt)", "verdict")
  foreach ($name in $Cases.Keys) {
    $r = $Ref[$name]; $g = $Got[$name]
    $crossHost = $OtherHost -and $HostDependentStdout -contains $name
    $o = ($r.Stdout -ceq $g.Stdout) -or $crossHost
    if ($crossHost) {
      $sane = $g.Stdout.Length -gt 0 -and -not $g.Stdout.Contains([string][char]0)
      Write-Host "  ($($name): stdout lists the host's fonts; not compared against another host's golden; Rustty's is $(if ($sane) { 'non-empty with no NUL byte' } else { 'EMPTY or holds a NUL byte (UD344)' }))"
      if (-not $sane) { $bad++ }
    }
    $e = $r.Stderr -ceq $g.Stderr
    $x = $r.Exit -eq $g.Exit
    # A justification names the streams it covers ("stdout", "stderr", "exit");
    # a difference in any other stream is still a DIFF.
    $differs = @(); if (-not $o) { $differs += "stdout" }; if (-not $e) { $differs += "stderr" }; if (-not $x) { $differs += "exit" }
    $covered = $Justified.ContainsKey($name) -and @($differs | Where-Object { $Justified[$name].Streams -notcontains $_ }).Count -eq 0
    # A justification whose difference is gone is stale: it fails too, so the
    # list stays exactly the recorded differences. A justified stream that was
    # not compared (host-dependent stdout in check mode) cannot be stale.
    $compared = if ($Justified.ContainsKey($name)) { @($Justified[$name].Streams | Where-Object { -not ($crossHost -and $_ -eq "stdout") }) } else { @() }
    $verdict = if ($differs.Count -eq 0 -and $compared.Count -gt 0) { $bad++; "MATCH, but a justification is listed (stale)" } elseif ($differs.Count -eq 0) { "MATCH" } elseif ($covered) { "JUSTIFIED ($($differs -join ',')): $($Justified[$name].Why)" } else { $bad++; "DIFF" }
    Write-Host ("{0,-30} {1,-6} {2,-6} {3,-12} {4}" -f $name, $(if ($o) { "same" } else { "DIFF" }), $(if ($e) { "same" } else { "DIFF" }), "$($r.Exit)/$($g.Exit)", $verdict)
    if ($verdict -eq "DIFF") {
      if (-not $o) { Show-Diff "stdout" $r.Stdout $g.Stdout }
      if (-not $e) { Show-Diff "stderr" $r.Stderr $g.Stderr }
    }
  }
  return $bad
}

# "Opening" the exe: a shell start (like a double-click: no console, no
# redirection), then a start under a console the GUI-subsystem child cannot
# write to (what `cmd /c rustty ...` in a console window does). Returns the
# observed exit codes; fails on a hang, a leftover process, or a WER record.
function Test-Launch([string]$ExePath) {
  $name = [IO.Path]::GetFileNameWithoutExtension($ExePath)
  $obs = [ordered]@{}
  $obs["subsystem"] = Get-PeSubsystem $ExePath
  $since = Get-Date
  foreach ($args1 in @(@(), @("+version"))) {
    $label = if ($args1.Count) { $args1 -join " " } else { "no-args" }
    $sw = [Diagnostics.Stopwatch]::StartNew()
    if ($args1.Count) { $p = Start-Process -FilePath $ExePath -ArgumentList $args1 -PassThru }
    else { $p = Start-Process -FilePath $ExePath -PassThru }
    $done = $p.WaitForExit(15000)
    $obs["open $label"] = if ($done) { "exit=$($p.ExitCode) in $($sw.ElapsedMilliseconds)ms" } else { try { $p.Kill() } catch { }; "HANG" }
    $cmdline = "`"$ExePath`" $($args1 -join ' ') & exit !errorlevel!"
    $c = Start-Process -FilePath cmd.exe -ArgumentList @("/v:on", "/c", "`"$cmdline`"") -WindowStyle Hidden -Wait -PassThru
    $obs["console $label"] = "exit=$($c.ExitCode)"
  }
  Start-Sleep -Milliseconds 500
  $left = @(Get-Process -ErrorAction SilentlyContinue | Where-Object { try { $_.Path -eq $ExePath } catch { $false } }).Count
  $obs["processes left"] = $left
  $wer = @()
  try {
    $wer = @(Get-WinEvent -FilterHashtable @{ LogName = "Application"; StartTime = $since; Id = 1000, 1001, 1002 } -ErrorAction Stop |
        Where-Object { $_.Message -match [regex]::Escape("$name.exe") })
  } catch [Exception] {
    if ($_.Exception.Message -notmatch "No events were found") { throw }
  }
  $obs["WER/Application Error events"] = $wer.Count
  return $obs
}

# ---------------------------------------------------------------- main
New-Item -ItemType Directory -Force $Work | Out-Null
$Exe = (Resolve-Path $Exe).Path
$failed = 0

"exe: $Exe"
$launch = Test-Launch $Exe
$launch.GetEnumerator() | ForEach-Object { "  {0,-32} {1}" -f $_.Key, $_.Value }
if ($launch["subsystem"] -ne 2) { "FAIL: PE subsystem is $($launch['subsystem']), expected 2 (GUI) as the reference"; $failed++ }
if ($launch["processes left"] -ne 0) { "FAIL: processes left behind"; $failed++ }
if ($launch["WER/Application Error events"] -ne 0) { "FAIL: Windows recorded a crash"; $failed++ }
foreach ($k in @($launch.Keys)) { if ("$($launch[$k])" -eq "HANG") { "FAIL: $k hung"; $failed++ } }

switch ($Mode) {
  "capture" {
    if (-not $Golden) { throw "-Golden is required with -Mode capture" }
    New-Item -ItemType Directory -Force $Golden | Out-Null
    $utf8 = New-Object Text.UTF8Encoding $false
    $res = Run-Matrix $Exe "cap" $true
    $hostValues = Get-HostValues
    foreach ($name in $Cases.Keys) {
      $b = Join-Path $Golden $name
      [IO.File]::WriteAllText("$b.stdout", (To-HostPlaceholders $res[$name].Stdout $hostValues), $utf8)
      [IO.File]::WriteAllText("$b.stderr", (To-HostPlaceholders $res[$name].Stderr $hostValues), $utf8)
      [IO.File]::WriteAllText("$b.exit", "$($res[$name].Exit)`n", $utf8)
    }
    $lines = @($launch.GetEnumerator() | Where-Object { $_.Key -match '^(open|console) ' } |
        ForEach-Object { "{0}: {1}" -f $_.Key, ($_.Value -replace ' in \d+ms', '') })
    [IO.File]::WriteAllText((Join-Path $Golden "_launch.txt"), (($lines -join "`n") + "`n"), $utf8)
    "captured $($Cases.Count) cases into $Golden"
  }
  "diff" {
    if (-not $RefExe) { throw "-RefExe is required with -Mode diff" }
    $RefExe = (Resolve-Path $RefExe).Path
    "reference: $RefExe"
    $refLaunch = Test-Launch $RefExe
    $refLaunch.GetEnumerator() | ForEach-Object { "  {0,-32} {1}" -f $_.Key, $_.Value }
    foreach ($k in @($refLaunch.Keys)) {
      if ($k -match '^(open|console) ') {
        $a = "$($refLaunch[$k])" -replace ' in \d+ms', ''; $b = "$($launch[$k])" -replace ' in \d+ms', ''
        if ($a -ne $b) { "FAIL: launch '$k': reference $a, rustty $b"; $failed++ }
      }
    }
    $failed += Compare-Results (Run-Matrix $RefExe "ref" $true) (Run-Matrix $Exe "rt" $false)
  }
  "check" {
    if (-not $Golden) { throw "-Golden is required with -Mode check" }
    $want = @{}
    Get-Content (Join-Path $Golden "_launch.txt") | ForEach-Object { $k, $v = $_ -split ': ', 2; $want[$k] = $v }
    foreach ($k in $want.Keys) {
      $b = "$($launch[$k])" -replace ' in \d+ms', ''
      if ($want[$k] -ne $b) { "FAIL: launch '$k': reference $($want[$k]), rustty $b"; $failed++ }
    }
    $failed += Compare-Results (Read-Golden $Golden) (Run-Matrix $Exe "rt" $false) $true
  }
}

if ($failed) { "RESULT: $failed difference(s) not justified"; exit 1 }
"RESULT: all cases match (or are justified)"
exit 0
