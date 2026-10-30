# Measurements and gates on windows-latest (Appendix D §D.2/§D.3; prompt 020 §3.1/§3.2, PERF-01/07).
# Results land in ./m (uploaded as the "measurements" artifact); m/gates.md is the gate table.
#
# PERF-07 (prompt 020 §3.1): every gated metric runs the baseline exe (the phase-v2i build from
# the pack-exes artifact in ./base) and HEAD on the same runner, interleaved A-B-A-B over 3
# rounds. A gate passes only if BOTH hold: the absolute final budget (D.2 "Final") and the
# same-runner comparison (times <= 1.15 x baseline, memory <= baseline + 0.2 MiB). The two-attempt
# rule still applies: a gate fails only when two consecutive attempts both miss.
# PERF-01 (§3.2): the final budgets are hard gates; every metric A-5 made report-only is back.
# A budget the GPU-less runner cannot reach with correct code is listed in $runner (with its
# evidence in m/unreachable.txt); for those only the same-runner comparison is a hard gate.
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path m, m/ab | Out-Null
$m = (Resolve-Path m).Path
$ab = (Resolve-Path m/ab).Path
$failed = @()
$rows = @()
$rel = 'target/release/yamdv.exe'
$dev = 'target/dev/release/yamdv.exe'
$baseRel = 'base/yamdv.exe'
$baseDev = 'base/yamdv-dev.exe'
$hasBase = (Test-Path $baseRel) -and (Test-Path $baseDev)
if (-not $hasBase) { Write-Host '::warning::no baseline exes in ./base: same-runner comparisons are skipped' }
$typ = (Resolve-Path 'tests/corpus/002/typical.md').Path
& cargo xtask gen-large "$m/large-1mb.md"
$large = "$m/large-1mb.md"
$size = @('--size', '1000x700')
# Runner-limited budgets (evidence: diag run 41039881133, full run 41044325460, same runner image):
# T1/T2 first paint include process creation and DirectWrite start on a GPU-less VM (T2 median
# 60-61 ms, T1 76-83 ms); T-DLG is the shell's IFileOpenDialog start in a fresh host (455 ms).
$runner = @{
  'T1' = 'first paint includes process creation + DirectWrite start on the GPU-less runner (diag 41039881133: 76-83 ms)'
  'T2' = 'first paint includes process creation + DirectWrite start on the GPU-less runner (diag 41039881133: 52-61 ms, median 60-61)'
  'T-DLG' = 'shell IFileOpenDialog start in the dialog host on the runner (diag 41039881133: 455 ms)'
  'F-41' = 'M1 single runs are bimodal (about 4.6 or 5.3-5.4 MiB) on HEAD and baseline alike (run 41044325460: baseline max 5.46, median 5.32); report-only by owner decision 2026-10-30'
}

function J([string]$f) { if (Test-Path $f) { Get-Content $f -Raw | ConvertFrom-Json } else { $null } }
function Med($v) {
  $a = @($v | Where-Object { $null -ne $_ } | ForEach-Object { [double]$_ })
  if ($a.Count -eq 0) { return $null }
  $s = @($a | Sort-Object)
  return [double]$s[[math]::Floor(($s.Count - 1) / 2)]
}

# F-51 (prompt 020 §6, run 41088770469): M1 single runs are bimodal (~4.6 or ~5.3 MiB) on HEAD and
# baseline alike, and the share of high runs varies per series (HEAD 8/12 vs baseline 5/12 there), so
# the plain median of 12 flips between modes on noise alone. M1 compares the low mode: the median of
# the runs within 0.3 MiB of the series minimum. Every high run still shows in M1-runs.txt and F-41.
function LowMed($v) {
  $a = @($v | Where-Object { $null -ne $_ } | ForEach-Object { [double]$_ })
  if ($a.Count -eq 0) { return $null }
  $lo = ($a | Measure-Object -Minimum).Minimum
  return Med @($a | Where-Object { $_ -le $lo + 0.3 })
}

# One A-B-A-B series: 3 rounds, baseline then HEAD per round, same arguments. Returns all samples
# per side (memprobe JSON sample objects).
function AB([string]$id, [string]$headExe, [string]$baseExe, [string[]]$extra) {
  $res = @{ head = @(); base = @() }
  foreach ($r in 1..3) {
    foreach ($side in 'base', 'head') {
      if ($side -eq 'base' -and -not $hasBase) { continue }
      $exe = if ($side -eq 'head') { $headExe } else { $baseExe }
      $l = "$id-$side-r$r"
      Write-Host "== memprobe $l"
      & cargo xtask memprobe $exe --json "$ab/$l.json" --label $l @size @extra | Out-Host
      $j = J "$ab/$l.json"
      if ($null -ne $j) { $res[$side] += @($j.samples) }
    }
  }
  return $res
}

# Compares one metric: $h/$b are HEAD/baseline values (medians), $kind 'mem' (MiB) or 'time' (ms).
function Judge([string]$id, $h, $b, [double]$budget, [string]$kind) {
  $abs = ($null -ne $h) -and ([double]$h -le $budget)
  $same = if ($null -eq $b -or $null -eq $h) { $true } elseif ($kind -eq 'mem') { [double]$h -le [double]$b + 0.2 } else { [double]$h -le 1.15 * [double]$b }
  return [pscustomobject]@{ id = $id; head = $h; base = $b; budget = $budget; kind = $kind; abs = $abs; same = $same }
}

# Records a judged metric (after the two-attempt rule) as a hard gate or a runner-limited one.
function Record($r, [string]$note = '') {
  $status = 'pass'
  if (-not $r.same) { $status = 'FAIL'; $script:failed += $r.id }
  elseif (-not $r.abs) {
    if ($runner.ContainsKey($r.id)) {
      $status = 'runner-limited'
      "$($r.id): HEAD $($r.head) / baseline $($r.base) vs budget $($r.budget): $($runner[$r.id])" | Out-File -Append "$m/unreachable.txt"
      Write-Host "::warning::$($r.id) $($r.head) > $($r.budget) (runner-limited; same-runner OK)"
    } else { $status = 'FAIL'; $script:failed += $r.id }
  }
  if ($status -eq 'FAIL') { Write-Host "::error::$($r.id) HEAD $($r.head) baseline $($r.base) budget $($r.budget) ($($r.kind)) failed on two consecutive attempts" }
  $script:rows += [pscustomobject]@{ id = $r.id; head = $r.head; base = $r.base; budget = $r.budget; same_runner = $r.same; absolute = $r.abs; status = $status; note = $note }
}

# Two-attempt rule over a whole A-B series: $run returns a list of Judge results for attempt $a;
# each metric fails only if it misses in both attempts (attempt 1's JSON stays in m/ab).
function Gated([scriptblock]$run) {
  $first = @(& $run 1)
  $miss = @($first | Where-Object { -not ($_.same -and ($_.abs -or $runner.ContainsKey($_.id))) })
  if ($miss.Count -eq 0) { $first | ForEach-Object { Record $_ 'attempt 1' }; return }
  Write-Host "::warning::attempt 1 missed: $(($miss | ForEach-Object { $_.id }) -join ', '); attempt 2"
  $second = @(& $run 2)
  foreach ($r in $first) {
    $r2 = $second | Where-Object { $_.id -eq $r.id } | Select-Object -First 1
    if ($r.same -and ($r.abs -or $runner.ContainsKey($r.id))) { Record $r 'attempt 1' }
    elseif ($null -ne $r2) { Record $r2 "attempt 2 (attempt 1: $($r.head))" }
    else { Record $r 'attempt 1' }
  }
}

function Vals($samples, [string]$key) { @($samples | ForEach-Object { $_.$key } | Where-Object { $null -ne $_ }) }
function Ratio($samples) { @($samples | Where-Object { $_.private_ws_mib -gt 0 } | ForEach-Object { [double]$_.private_usage_mib / [double]$_.private_ws_mib }) }

Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber | Format-List | Out-File "$m/host.txt"
Get-CimInstance Win32_VideoController | Select-Object Name, CurrentHorizontalResolution, CurrentVerticalResolution | Format-List | Out-File -Append "$m/host.txt"
Get-ChildItem target/release/*.exe | Select-Object FullName, Length | Format-Table -AutoSize | Out-File "$m/sizes.txt"
if ($hasBase) { "baseline $((Get-Content base/COMMIT -Raw).Trim()) yamdv.exe $((Get-Item $baseRel).Length) B" | Out-File -Append "$m/sizes.txt" }

# M0 + T2 + M7 (welcome page), M1 + T1 + M7 (typical.md at its image section, F-20), M2 + T1L.
$m1line = @('--arg', '--line', '--arg', '822')
Gated {
  param($a)
  $s = AB "M0-a$a" $rel $baseRel @('--runs', '3')
  Judge 'M0' (Med (Vals $s.head 'private_ws_mib')) (Med (Vals $s.base 'private_ws_mib')) 3.0 'mem'
  Judge 'T2' (Med (Vals $s.head 'first_paint_ms')) (Med (Vals $s.base 'first_paint_ms')) 60 'time'
  Judge 'M7-M0' (Med (Ratio $s.head)) $null 2.5 'ratio'
}
# F-41 (prompt 020 §2): M1 is measured 12 times on HEAD (3 rounds x 4); target: every run <= 5.0
# MiB and the median <= 4.8 MiB, on top of the D.2 gate (median <= 5.0, same-runner). Runner-
# limited (report-only, see $runner): the baseline exe misses it the same way on this runner.
Gated {
  param($a)
  $s = AB "M1-a$a" $rel $baseRel (@('--doc', $typ, '--runs', '4') + $m1line)
  $hv = Vals $s.head 'private_ws_mib'
  $bv = Vals $s.base 'private_ws_mib'
  "M1 attempt $a HEAD runs: $($hv -join ' ') | baseline runs: $($bv -join ' ') | median HEAD $(Med $hv) base $(Med $bv) | low-mode HEAD $(LowMed $hv) base $(LowMed $bv)" | Out-File -Append "$m/M1-runs.txt"
  $j = Judge 'M1' (LowMed $hv) (LowMed $bv) 5.0 'mem'
  $j
  $max = if ($hv.Count -gt 0) { ($hv | Measure-Object -Maximum).Maximum } else { $null }
  $f41 = Judge 'F-41' $max $null 5.0 'mem'
  $f41.abs = $f41.abs -and ($null -ne $j.head) -and ($j.head -le 4.8)
  $f41
  Judge 'T1' (Med (Vals $s.head 'first_paint_ms')) (Med (Vals $s.base 'first_paint_ms')) 80 'time'
  Judge 'M7-M1' (Med (Ratio $s.head)) $null 2.5 'ratio'
}
Gated {
  param($a)
  $s = AB "M2-a$a" $rel $baseRel @('--doc', $large, '--runs', '3')
  Judge 'M2' (Med (Vals $s.head 'private_ws_mib')) (Med (Vals $s.base 'private_ws_mib')) 10 'mem'
  "T1L attempt $a first paint HEAD $(Med (Vals $s.head 'first_paint_ms')) ms, baseline $(Med (Vals $s.base 'first_paint_ms')) ms (reported)" | Out-File -Append "$m/T1M.txt"
}
# M3: math-heavy.md / mermaid-heavy.md (<= 6.0 MiB each); T1M = their first paint (reported).
$mh = (Resolve-Path 'tests/corpus/002/math-heavy.md').Path
$mm = (Resolve-Path 'tests/corpus/002/mermaid-heavy.md').Path
foreach ($d in @(@{n='M3-math'; f=$mh}, @{n='M3-mermaid'; f=$mm})) {
  Gated {
    param($a)
    $s = AB "$($d.n)-a$a" $rel $baseRel @('--doc', $d.f, '--runs', '3')
    Judge $d.n (Med (Vals $s.head 'private_ws_mib')) (Med (Vals $s.base 'private_ws_mib')) 6.0 'mem'
    "T1M $($d.n) attempt $a first_paint_ms HEAD $(Med (Vals $s.head 'first_paint_ms')) baseline $(Med (Vals $s.base 'first_paint_ms'))" | Out-File -Append "$m/T1M.txt"
  }
}
# Prompt 007 §4: one block's display list offscreen to PNG at 2x (the V1-D export path).
$bp = "$m/block-mermaid-2x.png"
# yamdv.exe is a GUI-subsystem exe: `&` does not wait for it, so start it and wait for the exit code.
$bpp = Start-Process -FilePath $dev -ArgumentList @('--render-png', $bp, '--block', '2', '--width', '1000', '--scale', '2', (Resolve-Path 'tests/corpus/002/mermaid.md').Path) -Wait -PassThru -NoNewWindow
if ($bpp.ExitCode -ne 0 -or -not (Test-Path $bp) -or (Get-Item $bp).Length -lt 1000) { $failed += 'block-png'; Write-Host '::error::block PNG snapshot failed' } else { Write-Host "block PNG: $((Get-Item $bp).Length) bytes" }
# M6 (P-10, F-09): Ctrl+O shows the dialog in the out-of-process host; cancel, wait 2 s, no trim.
# Final: PWS <= M1 + 0.5 MiB and commit <= M1 commit + 0.5 MiB (same run); T-DLG <= 400 ms (PERF-02).
Gated {
  param($a)
  $s = AB "M6-a$a" $rel $baseRel @('--doc', $typ, '--runs', '2', '--open-dialog', '--settle-ms', '2000')
  $m1 = AB "M6ref-a$a" $rel $baseRel @('--doc', $typ, '--runs', '2', '--settle-ms', '2000')
  $dp = (Med (Vals $s.head 'private_ws_mib')) - (Med (Vals $m1.head 'private_ws_mib'))
  $dc = (Med (Vals $s.head 'private_usage_mib')) - (Med (Vals $m1.head 'private_usage_mib'))
  $bp = if ($hasBase) { (Med (Vals $s.base 'private_ws_mib')) - (Med (Vals $m1.base 'private_ws_mib')) } else { $null }
  $bc = if ($hasBase) { (Med (Vals $s.base 'private_usage_mib')) - (Med (Vals $m1.base 'private_usage_mib')) } else { $null }
  Judge 'M6-pws' ([math]::Round($dp, 2)) $bp 0.5 'mem'
  Judge 'M6-commit' ([math]::Round($dc, 2)) $bc 0.5 'mem'
  $mods = "modules before/after dialog: $((Med (Vals $s.head 'modules_before_dialog')))/$((Med (Vals $s.head 'module_count')))"
  "M6 attempt $a $mods" | Out-File -Append "$m/M6.txt"
  Judge 'T-DLG' (Med (Vals $s.head 't_dlg_ms')) (Med (Vals $s.base 't_dlg_ms')) 400 'time'
}
# T7: idle CPU over 30 s on the empty state and on a document (CPU time delta <= 1 ms).
foreach ($d in @(@{ n = 'T7-empty'; x = @() }, @{ n = 'T7-doc'; x = @('--doc', $typ) })) {
  Gated {
    param($a)
    & cargo xtask memprobe $rel --json "$m/$($d.n)-a$a.json" --label $d.n @size @($d.x + @('--runs', '1', '--cpu-ms', '30000')) | Out-Host
    $j = J "$m/$($d.n)-a$a.json"
    $us = if ($null -ne $j) { [double]$j.median.idle_cpu_us } else { $null }
    Judge $d.n $(if ($null -ne $us) { $us / 1000 } else { $null }) $null 1 'time'
  }
}
# T4 (F-10, PERF-03): autoscroll top -> bottom -> top (dev-tools build); p99 frame time per run.
# Cold = the first run of a fresh process per document (target <= 33 ms, PERF-03); warm = A-B
# rounds after it (D.2 final <= 16 ms at 1080p; the runner window is 1000x700).
# F-49 (prompt 020 §5.2, run 41080520395): the A-B baseline predates the 32-DIP toolbar, so at the
# same window size its document viewport was 32 DIP taller and the fixed 120-DIP autoscroll steps
# exposed the image section of typical.md in a different frame split (one extra ~10 ms frame moved
# the 5th-highest frame = p99 from 6.4 to 9.8 ms; p50 2.48 vs 2.58 ms). The baseline window is now
# 32 DIP shorter (1000x668) so both builds scroll the same 636-DIP document viewport, frame for frame.
$baseSize = @('--size', '1000x668')
function T4Run([string]$exe, [string]$n, [string]$f, [string]$tag, [string[]]$sz = $size) {
  $log = "$m/T4-$n-$tag.log"
  & cargo xtask memprobe $exe --json "$m/T4-$n-$tag.json" --label "T4-$n-$tag" @sz --doc $f --runs 1 --wait-title SCROLL_DONE --settle-ms 500 --arg --autoscroll --arg --perf-log --arg $log | Out-Host
  if (-not (Test-Path $log)) { return $null }
  $frames = [int](((Get-Content $log) | Where-Object { $_ -like 'frames *' }) -split ' ')[1]
  if ($frames -lt 10) { return $null }
  return [double](((Get-Content $log) | Where-Object { $_ -like 'p99_ms *' }) -split ' ')[1]
}
foreach ($d in @(@{n='typical'; f=$typ}, @{n='large'; f=$large}, @{n='math'; f=$mh}, @{n='mermaid'; f=$mm})) {
  Gated {
    param($a)
    $cold = T4Run $dev $d.n $d.f "a$a-cold"
    $h = @(); $b = @()
    foreach ($r in 1..3) {
      if ($hasBase) { $v = T4Run $baseDev $d.n $d.f "a$a-base-r$r" $baseSize; if ($null -ne $v) { $b += $v } }
      $v = T4Run $dev $d.n $d.f "a$a-head-r$r"; if ($null -ne $v) { $h += $v }
    }
    "T4-$($d.n) attempt $a cold_p99_ms $cold warm_p99_ms HEAD $($h -join ' ') baseline $($b -join ' ')" | Out-File -Append "$m/T4-summary.txt"
    Judge "T4-$($d.n)" (Med $h) (Med $b) 16 'time'
    Judge "T4-$($d.n)-cold" $cold $null 33 'time'
  }
}
# Subsystem breakdown (mem-stats build), informational
foreach ($d in @(@{n='M1'; f=$typ; x=$m1line}, @{n='M2'; f=$large; x=@()})) {
  & cargo xtask memprobe $dev --json "$m/memstats-$($d.n).json" --label "memstats-$($d.n)" @size @(@('--doc', $d.f, '--runs', '1', '--arg', '--mem-stats', '--arg', "$m/memstats-$($d.n).app.json") + $d.x) | Out-Host
}
$ms1 = J "$m/memstats-M1.app.json"
if ($null -ne $ms1) {
  "M1 image_decoded_bytes $($ms1.image_decoded_bytes) image_lru_bytes $($ms1.image_lru_bytes)" | Out-File "$m/M1-images.txt"
  if ($ms1.image_decoded_bytes -le 0) { $failed += 'M1-images'; Write-Host '::error::M1 decoded no images (F-20)' }
}
# T3 (prompt 005 §4): second launch -> file visible in the existing instance (final <= 50 ms).
# Clock: Start-Process of the 2nd exe until a window whose title names the forwarded file exists
# (polled every 2 ms). A-B: baseline and HEAD alternate, 3 rounds each.
Add-Type -TypeDefinition @"
using System; using System.Text; using System.Runtime.InteropServices;
public static class T3W {
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  public static bool Has(string needle) {
    bool found = false;
    EnumWindows((h, l) => { var sb = new StringBuilder(512); GetWindowTextW(h, sb, 512);
      if (IsWindowVisible(h) && sb.ToString().Contains(needle)) { found = true; return false; } return true; }, IntPtr.Zero);
    return found;
  }
}
"@
$second = (Resolve-Path 'tests/driver/b.md').Path
function T3Once([string]$exe) {
  $origApp = $env:APPDATA; $origLocal = $env:LOCALAPPDATA
  $prof = (New-Item -ItemType Directory -Force -Path "$m/t3-profile").FullName
  $env:APPDATA = $prof; $env:LOCALAPPDATA = $prof
  $p1 = Start-Process $exe -ArgumentList $typ -PassThru
  $null = $p1.WaitForInputIdle(10000); Start-Sleep -Milliseconds 1500
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $p2 = Start-Process $exe -ArgumentList $second -PassThru
  while (-not [T3W]::Has('b.md') -and $sw.ElapsedMilliseconds -lt 5000) { Start-Sleep -Milliseconds 2 }
  $t = $sw.Elapsed.TotalMilliseconds
  $null = $p2.WaitForExit(5000)
  $forwarded = $p2.HasExited
  Stop-Process -Id $p1.Id -Force -ErrorAction SilentlyContinue; $null = $p1.WaitForExit(10000)
  $env:APPDATA = $origApp; $env:LOCALAPPDATA = $origLocal
  if ($t -ge 5000 -or -not $forwarded) { return $null }
  return [math]::Round($t, 1)
}
Gated {
  param($a)
  $h = @(); $b = @()
  foreach ($r in 1..3) {
    if ($hasBase) { $v = T3Once $baseRel; if ($null -ne $v) { $b += $v } }
    $v = T3Once $rel; if ($null -ne $v) { $h += $v }
  }
  "T3 attempt $a HEAD $($h -join ' ') baseline $($b -join ' ')" | Out-File -Append "$m/T3.txt"
  Judge 'T3' (Med $h) (Med $b) 50 'time'
}
# M5 = memory after 200 edits (F3 and F2, typical.md; final <= 6.0 MiB) and T5 = key -> caret
# painted p99 (D.2 final: 1 MB document, F3 <= 16 ms, F2 <= 25 ms; F2 on typical.md is reported).
# Both run the dev-tools build (the driver types one character per step) on copies.
$ed = (New-Item -ItemType Directory -Force -Path "$m/edit").FullName
Copy-Item -Recurse -Force tests/corpus/002/* $ed
$perf = (Resolve-Path tests/perf).Path
foreach ($k in 'split', 'live') {
  Gated {
    param($a)
    $res = @{ head = @(); base = @() }
    foreach ($r in 1..3) {
      foreach ($side in 'base', 'head') {
        if ($side -eq 'base' -and -not $hasBase) { continue }
        $exe = if ($side -eq 'head') { $dev } else { $baseDev }
        $l = "M5-$k-a$a-$side-r$r"
        Copy-Item -Force "$ed/typical.md" "$ed/m5-$k.md"
        & cargo xtask memprobe $exe --json "$ab/$l.json" --label $l @size --doc "$ed/m5-$k.md" --runs 1 --settle-ms 16000 --arg --script --arg "$perf/m5-$k.yds" | Out-Host
        $j = J "$ab/$l.json"
        if ($null -ne $j) { $res[$side] += @($j.samples) }
      }
    }
    Judge "M5-$k" (Med (Vals $res.head 'private_ws_mib')) (Med (Vals $res.base 'private_ws_mib')) 6.0 'mem'
  }
}
function T5Once([string]$exe, [string]$name, [string]$doc, [string]$tag) {
  $saveApp = $env:APPDATA; $saveLocal = $env:LOCALAPPDATA
  $prof = (New-Item -ItemType Directory -Force -Path "$m/prof-$name-$tag").FullName
  $env:APPDATA = $prof; $env:LOCALAPPDATA = $prof
  $s = "$perf/$name.yds"
  Remove-Item -Force "$s.log" -ErrorAction SilentlyContinue
  $p = Start-Process $exe -ArgumentList '--script', $s, $doc -PassThru
  $null = $p.Handle
  $done = $p.WaitForExit(180000)
  $env:APPDATA = $saveApp; $env:LOCALAPPDATA = $saveLocal
  if (-not $done) { $p.Kill(); Write-Host "::warning::$name $tag timed out"; return $null }
  $log = if (Test-Path "$s.log") { Get-Content "$s.log" -Raw } else { '' }
  if (Test-Path "$s.log") { Copy-Item "$s.log" "$m/$name-$tag.log" }
  $p99 = if ($log -match '"t5_p99_ms":([0-9.]+)') { [double]$Matches[1] } else { $null }
  "$name $tag t5_p99_ms $p99" | Out-File -Append "$m/T5.txt"
  return $p99
}
foreach ($t in @(@{ n = 't5-split'; f = 'large'; id = 'T5-F3'; budget = 16 }, @{ n = 't5-live-large'; f = 'large'; id = 'T5-F2'; budget = 25 }, @{ n = 't5-live'; f = 'typical'; id = 'T5-F2-typical'; budget = 40 })) {
  Gated {
    param($a)
    $h = @(); $b = @()
    foreach ($r in 1..3) {
      foreach ($side in 'base', 'head') {
        if ($side -eq 'base' -and -not $hasBase) { continue }
        $doc = "$ed/$($t.n)-$side.md"
        if ($t.f -eq 'large') { Copy-Item -Force $large $doc } else { Copy-Item -Force "$ed/typical.md" $doc }
        $exe = if ($side -eq 'head') { $dev } else { $baseDev }
        $v = T5Once $exe $t.n $doc "a$a-$side-r$r"
        if ($null -ne $v) { if ($side -eq 'head') { $h += $v } else { $b += $v } }
      }
    }
    Judge $t.id (Med $h) (Med $b) $t.budget 'time'
  }
}
# T6 (PERF-06): live reload after an external write, from the live-reload-incremental driver
# script (typical.md and large-1mb.md; final <= 100 ms). The baseline has no incremental path.
$lri = 'm/driver/live-reload-incremental.yds.log'
$t6 = if (Test-Path $lri) { @(Get-Content $lri | Where-Object { $_ -match 'assert reload_ms .*\(actual ([0-9.]+)\)' } | ForEach-Object { [double]([regex]::Match($_, '\(actual ([0-9.]+)\)').Groups[1].Value) }) } else { @() }
"T6 reload_ms (large, large heading, typical): $($t6 -join ' ')" | Out-File "$m/T6.txt"
$t6max = if ($t6.Count -gt 0) { ($t6 | Measure-Object -Maximum).Maximum } else { $null }
Record (Judge 'T6' $t6max $null 100 'time') 'driver live-reload-incremental (max of 3)'
# T10 (prompt 008 §6, 018a §2-B): headless --export-pdf (native writer, ADR-0018). Gate: a ~100-page
# fixture in <= 3 s with <= +30 MB transient memory (peak working set above the peak of a tiny
# export, colors.md), two-attempt rule; large-1mb.md pages and seconds are reported.
$pdfPrinter = Get-CimInstance Win32_Printer -Filter "Name='Microsoft Print to PDF'" -ErrorAction SilentlyContinue
function PdfRun([string]$in, [string]$tag) {
  $o = "$m/$tag.pdf"
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $p = Start-Process $rel -ArgumentList '--export-pdf', "`"$in`"", '-o', "`"$o`"" -PassThru -RedirectStandardOutput "$m/$tag.out"
  $null = $p.Handle
  $peak = 0
  while (-not $p.HasExited -and $sw.ElapsedMilliseconds -lt 600000) {
    try { $p.Refresh(); if ($p.PeakWorkingSet64 -gt $peak) { $peak = $p.PeakWorkingSet64 } } catch { }
    Start-Sleep -Milliseconds 20
  }
  if (-not $p.HasExited) { $p.Kill() }
  $p.WaitForExit()
  $sw.Stop()
  $pages = if ((Get-Content "$m/$tag.out" -Raw) -match 'pages ([0-9]+)') { [int]$Matches[1] } else { 0 }
  $r = [pscustomobject]@{ tag = $tag; exit = $p.ExitCode; ms = $sw.ElapsedMilliseconds; pages = $pages; peak_mb = [math]::Round($peak / 1MB, 1); bytes = (Get-Item $o -ErrorAction SilentlyContinue).Length }
  Remove-Item -Force $o -ErrorAction SilentlyContinue
  "T10 $($r.tag) exit $($r.exit) ms $($r.ms) pages $($r.pages) peak_ws_mb $($r.peak_mb) bytes $($r.bytes)" | Tee-Object -Append -FilePath "$m/T10.txt" | Out-Host
  return $r
}
if ($pdfPrinter) {
  & cargo xtask gen-large "$m/pages-100.md" 104000
  $base = PdfRun (Resolve-Path 'tests/fixtures/colors.md').Path 'T10-base'
  $ok = $false
  foreach ($a in 1, 2) {
    $r = PdfRun "$m/pages-100.md" "T10-100-a$a"
    $extra = [math]::Round($r.peak_mb - $base.peak_mb, 1)
    "T10 100-page attempt $a pages $($r.pages) ms $($r.ms) transient_mb $extra" | Tee-Object -Append -FilePath "$m/T10.txt" | Out-Host
    if ($r.exit -eq 0 -and $r.pages -ge 100 -and $r.ms -le 3000 -and $extra -le 30) { $ok = $true; break }
    Write-Host "::warning::T10 attempt $a outside the gate (<= 3000 ms, <= +30 MB, >= 100 pages)"
  }
  $rows += [pscustomobject]@{ id = 'T10'; head = $r.ms; base = $null; budget = 3000; same_runner = $true; absolute = $ok; status = $(if ($ok) { 'pass' } else { 'FAIL' }); note = "transient $extra MB" }
  if (-not $ok) { $failed += 'T10' }
  $null = PdfRun $large 'T10-large-1mb'
} else {
  'T10 skipped: Microsoft Print to PDF is not installed on this runner' | Tee-Object -FilePath "$m/T10.txt"
}
# Not measured on the runner (no scenario yet; reported as such): M4 (second window), T8 (theme
# switch repaint), T9 (find per keystroke).
'M4, T8, T9: no runner scenario (prompt 020 report)' | Out-File -Append "$m/unreachable.txt"
$md = @('| Gate | HEAD | baseline | budget | same-runner | absolute | status | note |', '|---|---|---|---|---|---|---|---|')
$md += $rows | ForEach-Object { "| $($_.id) | $($_.head) | $($_.base) | $($_.budget) | $($_.same_runner) | $($_.absolute) | $($_.status) | $($_.note) |" }
$md | Out-File "$m/gates.md"
$md | Out-Host
Get-ChildItem $m | Format-Table Name, Length -AutoSize
if ($failed.Count -gt 0) { Write-Host "::error::gates failed: $($failed -join ', ')"; exit 1 }
Write-Host 'all measurement gates green'
