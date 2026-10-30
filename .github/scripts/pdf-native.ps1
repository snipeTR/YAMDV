# 018a §2-B pdf-native checks: inspects the PDFs written by tests/driver/pdf-native.yds (UI path)
# and by headless --export-pdf with `cargo xtask pdf-inspect`. Results land in ./m/pdf-native.
$ErrorActionPreference = 'Stop'
$out = (New-Item -ItemType Directory -Force -Path m/pdf-native).FullName
$rel = (Resolve-Path 'target/release/yamdv.exe').Path
$typ = (Resolve-Path 'tests/corpus/002/typical.md').Path
$col = (Resolve-Path 'tests/fixtures/colors.md').Path
$fail = @()
function Headings([string]$md) {
  $n = 0; $fence = $false
  foreach ($l in Get-Content $md) {
    if ($l -match '^\s{0,3}(```|~~~)') { $fence = -not $fence; continue }
    if (-not $fence -and $l -match '^#{1,6} ') { $n++ }
  }
  return $n
}
$cases = @(
  @{ tag = 'ui-typical'; pdf = 'tests/driver/pn-typical.tmp.pdf'; md = $typ; find = 'The reader is built around a two-level'; white = $true },
  @{ tag = 'ui-colors-dark'; pdf = 'tests/driver/pn-colors.tmp.pdf'; md = $col; find = 'Plain text, bold, italic'; white = $true }
)
foreach ($c in @(@{ tag = 'cli-typical'; md = $typ; mode = 'light'; find = 'The reader is built around a two-level' },
                 @{ tag = 'cli-colors-dark'; md = $col; mode = 'dark'; find = 'Plain text, bold, italic' })) {
  $pdf = "$out/$($c.tag).pdf"
  $p = Start-Process $rel -ArgumentList '--export-pdf', "`"$($c.md)`"", '-o', "`"$pdf`"", '--mode', $c.mode -PassThru -RedirectStandardOutput "$out/$($c.tag).out"
  $null = $p.Handle
  if (-not $p.WaitForExit(120000)) { $p.Kill(); $fail += "$($c.tag) timeout"; continue }
  if ($p.ExitCode -ne 0) { $fail += "$($c.tag) exit $($p.ExitCode)" }
  $cases += @{ tag = $c.tag; pdf = $pdf; md = $c.md; find = $c.find; white = $true }
}
foreach ($c in $cases) {
  if (-not (Test-Path $c.pdf)) { $fail += "$($c.tag) missing file"; continue }
  $j = & cargo xtask pdf-inspect $c.pdf --find $c.find | Select-Object -Last 1
  $j | Out-File "$out/$($c.tag).json"
  $r = $j | ConvertFrom-Json
  $want = Headings $c.md
  $bad = @()
  if ($r.pages -lt 1) { $bad += 'pages' }
  if ($r.outline_count -ne $want) { $bad += "outline $($r.outline_count) != headings $want" }
  if ($c.tag -like '*typical' -and $r.goto_links -lt 1) { $bad += 'no GoTo links' }
  if ($r.bad_uris.Count -gt 0) { $bad += "bad URIs: $($r.bad_uris -join ' ')" }
  if ($r.forbidden.Count -gt 0) { $bad += "forbidden: $($r.forbidden -join ' ')" }
  if (-not $r.fonts_ok) { $bad += 'fonts not all subset+embedded+ToUnicode' }
  if (-not $r.found) { $bad += "text not found: $($c.find)" }
  if ($c.white -and -not $r.page1_white) { $bad += "page 1 background $($r.page1_bg -join ',')" }
  Write-Host "$($c.tag): pages $($r.pages), outline $($r.outline_count)/$want, goto $($r.goto_links), uri $($r.uri_links), fonts $($r.fonts.Count), found $($r.found), white $($r.page1_white)"
  if ($bad.Count -gt 0) { $fail += "$($c.tag): $($bad -join '; ')" }
}
if (Test-Path 'tests/driver/pn-cancel.tmp.pdf') { $fail += 'cancelled UI export left a file' }
Remove-Item -Force tests/driver/pn-*.tmp.pdf -ErrorAction SilentlyContinue
if ($fail.Count -gt 0) { $fail | ForEach-Object { Write-Host "::error::pdf-native $_" }; exit 1 }
Write-Host 'pdf-native: all checks green'
