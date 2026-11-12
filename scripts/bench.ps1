<#
  Bridge performance bench: the same measurements for any build (Electron baseline or Tauri).
  Safe: temp BRIDGE_HOME seeded with the demo profile, BRIDGE_DRY_LAUNCH=1, BRIDGE_WINDOW=1600x900 (isolated: no tray,
  no global hotkey), and it only measures the process tree it started itself.

  Electron baseline:
    powershell -File scripts/bench.ps1 -Label electron -Exe node_modules\electron\dist\electron.exe -ArgList '.' `
      -SizePaths out,node_modules\electron\dist
  Tauri:
    powershell -File scripts/bench.ps1 -Label tauri -Exe src-tauri\target\release\bridge.exe -SizePaths src-tauri\target\release\bridge.exe
#>
param(
  [Parameter(Mandatory)] [string] $Label,
  [Parameter(Mandatory)] [string] $Exe,
  [string] $ArgList = '',
  [string[]] $SizePaths = @(),
  [int] $ColdRuns = 4,
  [int] $SettleSec = 12,
  [int] $CpuSec = 20,
  [string] $Out = 'docs/perf-results.jsonl'
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo
$Exe = (Resolve-Path $Exe).Path
Remove-Item Env:ELECTRON_RUN_AS_NODE -ErrorAction SilentlyContinue

Add-Type -Namespace BridgeBench -Name W -MemberDefinition @'
  public delegate bool EnumProc(System.IntPtr h, System.IntPtr l);
  [System.Runtime.InteropServices.DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, System.IntPtr l);
  [System.Runtime.InteropServices.DllImport("user32.dll")] public static extern bool IsWindowVisible(System.IntPtr h);
  [System.Runtime.InteropServices.DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(System.IntPtr h, out uint pid);
  [System.Runtime.InteropServices.DllImport("user32.dll", CharSet=System.Runtime.InteropServices.CharSet.Unicode)]
  public static extern int GetWindowText(System.IntPtr h, System.Text.StringBuilder s, int n);
  [System.Runtime.InteropServices.DllImport("user32.dll")] public static extern bool ShowWindow(System.IntPtr h, int cmd);
'@

function Get-Tree([int] $rootPid) {
  $all = Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId
  $ids = [System.Collections.Generic.HashSet[int]]::new(); [void]$ids.Add($rootPid)
  do {
    $added = 0
    foreach ($p in $all) { if ($ids.Contains([int]$p.ParentProcessId) -and $ids.Add([int]$p.ProcessId)) { $added++ } }
  } while ($added)
  @($ids | ForEach-Object { Get-Process -Id $_ -ErrorAction SilentlyContinue })
}

function Find-BridgeWindow([int[]] $pids) {
  $script:found = [IntPtr]::Zero
  $set = [System.Collections.Generic.HashSet[uint32]]::new(); foreach ($p in $pids) { [void]$set.Add([uint32]$p) }
  [BridgeBench.W]::EnumWindows({ param($h, $l)
      [uint32]$wp = 0; [void][BridgeBench.W]::GetWindowThreadProcessId($h, [ref]$wp)
      if ($set.Contains($wp) -and [BridgeBench.W]::IsWindowVisible($h)) {
        $sb = [System.Text.StringBuilder]::new(256); [void][BridgeBench.W]::GetWindowText($h, $sb, 256)
        if ($sb.ToString() -eq 'Bridge') { $script:found = $h; return $false }
      }
      return $true }, [IntPtr]::Zero) | Out-Null
  $script:found
}

# One data folder for the whole run, like a real install (the first launch creates the browser profile).
$script:benchHome = Join-Path ([IO.Path]::GetTempPath()) ("bridge-bench-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory $script:benchHome | Out-Null
Copy-Item tests/fixtures/demo-profile.json (Join-Path $script:benchHome 'profile.json')

function Start-Bridge {
  $home_ = $script:benchHome
  $env:BRIDGE_HOME = $home_; $env:BRIDGE_DRY_LAUNCH = '1'; $env:BRIDGE_WINDOW = '1600x900'
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $p = if ($ArgList) { Start-Process -FilePath $Exe -ArgumentList $ArgList -WorkingDirectory $repo -PassThru }
       else { Start-Process -FilePath $Exe -WorkingDirectory $repo -PassThru }
  $hwnd = [IntPtr]::Zero
  while ($sw.ElapsedMilliseconds -lt 20000) {
    # Top-level windows belong to the root process (Electron browser process / Tauri exe), so no tree walk here:
    # walking the tree via CIM takes ~0.5 s per call and would distort the timing.
    $hwnd = Find-BridgeWindow @($p.Id)
    if ($hwnd -ne [IntPtr]::Zero) { break }
    Start-Sleep -Milliseconds 10
  }
  $ms = $sw.ElapsedMilliseconds
  if ($hwnd -eq [IntPtr]::Zero) { throw "Bridge window never appeared" }
  [pscustomobject]@{ Proc = $p; Home = $home_; Hwnd = $hwnd; ColdMs = $ms }
}

function Stop-Bridge($b) {
  Get-Tree $b.Proc.Id | Sort-Object Id -Descending | ForEach-Object { Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue }
  Start-Sleep -Milliseconds 800
}

function Measure-Cpu([int] $rootPid, [int] $sec) {
  $a = (Get-Tree $rootPid | ForEach-Object { $_.TotalProcessorTime.TotalMilliseconds } | Measure-Object -Sum).Sum
  Start-Sleep -Seconds $sec
  $b = (Get-Tree $rootPid | ForEach-Object { $_.TotalProcessorTime.TotalMilliseconds } | Measure-Object -Sum).Sum
  # % of ONE logical core, averaged over the window.
  [math]::Round(100 * ($b - $a) / ($sec * 1000), 2)
}

function Measure-Pipe($b) {
  $fn = Get-Content (Join-Path $b.Home 'bridge-functions.ps1') -Raw
  if ($fn -notmatch "\`$pipeName\s*=\s*'([^']+)'") { return $null }
  $name = $Matches[1]
  $times = foreach ($i in 1..10) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $c = [IO.Pipes.NamedPipeClientStream]::new('.', $name, [IO.Pipes.PipeDirection]::InOut)
    $c.Connect(2000)
    $w = [IO.StreamWriter]::new($c, [Text.UTF8Encoding]::new($false)); $w.AutoFlush = $true
    $r = [IO.StreamReader]::new($c)
    $w.Write('{"argv":["help"]}' + "`n")
    [void]$r.ReadLine()
    $c.Dispose()
    $sw.Elapsed.TotalMilliseconds
  }
  [math]::Round((($times | Sort-Object)[4] + ($times | Sort-Object)[5]) / 2, 1)
}

# 1) Cold starts (first one warms the disk cache; we report all + median).
$cold = @()
foreach ($i in 1..$ColdRuns) { $b = Start-Bridge; $cold += $b.ColdMs; Stop-Bridge $b }

# 2) Steady state on one more launch.
$b = Start-Bridge
Start-Sleep -Seconds $SettleSec
$tree = Get-Tree $b.Proc.Id
$ws = [math]::Round(($tree | Measure-Object WorkingSet64 -Sum).Sum / 1MB, 1)
$priv = [math]::Round(($tree | Measure-Object PrivateMemorySize64 -Sum).Sum / 1MB, 1)
$cpuVisible = Measure-Cpu $b.Proc.Id $CpuSec
[void][BridgeBench.W]::ShowWindow($b.Hwnd, 0)   # SW_HIDE
Start-Sleep -Seconds 2
$cpuHidden = Measure-Cpu $b.Proc.Id $CpuSec
[void][BridgeBench.W]::ShowWindow($b.Hwnd, 5)   # SW_SHOW
$pipeMs = Measure-Pipe $b
Stop-Bridge $b

Remove-Item $script:benchHome -Recurse -Force -ErrorAction SilentlyContinue
$size = 0
foreach ($p in ($SizePaths -split ',')) { $size += (Get-ChildItem $p -Recurse -File -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum }

$res = [ordered]@{
  label = $Label; at = (Get-Date).ToString('s'); exe = Split-Path $Exe -Leaf
  coldStartMs = $cold; coldStartMedianMs = ($cold | Sort-Object)[[int][math]::Floor($cold.Count / 2)]
  processes = $tree.Count; workingSetMB = $ws; privateMB = $priv
  cpuVisiblePctOfCore = $cpuVisible; cpuHiddenPctOfCore = $cpuHidden
  pipeRoundTripMs = $pipeMs; sizeMB = [math]::Round($size / 1MB, 1)
}
$json = $res | ConvertTo-Json -Compress
Add-Content -Path $Out -Value $json -Encoding UTF8
$res
