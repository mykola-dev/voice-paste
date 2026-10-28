# Bounded process regression: no model/worker load, network, microphone or paste.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$Executable)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$work = Join-Path $root ('build/quit-test-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $work | Out-Null
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class QuitProcessTest {
    public delegate bool Callback(IntPtr h, IntPtr p);
    [DllImport("user32.dll")] public static extern bool EnumWindows(Callback callback, IntPtr p);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
    public static IntPtr FindHost(uint pid) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((h, p) => {
            uint owner; GetWindowThreadProcessId(h, out owner);
            var text = new StringBuilder(128);
            GetClassName(h, text, text.Capacity);
            if(owner == pid && text.ToString() == "VoicePasteRustHost") { found = h; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
}
'@
$app = $null
try {
    Copy-Item -LiteralPath $Executable -Destination (Join-Path $work 'VoicePaste.exe')
    New-Item -ItemType Directory (Join-Path $work 'data') | Out-Null
    $config = Get-Content (Join-Path $root 'crates/core/assets/default-settings.json') -Raw | ConvertFrom-Json
    $config.hotkey = 'Ctrl+Alt+Shift+F11'
    $config | ConvertTo-Json | Set-Content (Join-Path $work 'data/config.json')
    $info = [Diagnostics.ProcessStartInfo]::new((Join-Path $work 'VoicePaste.exe'), '--capture-preview')
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.WorkingDirectory = $work
    $app = [Diagnostics.Process]::Start($info)
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $hwnd = [IntPtr]::Zero
    while ($timer.Elapsed.TotalSeconds -lt 10 -and -not $app.HasExited) {
        $hwnd = [QuitProcessTest]::FindHost($app.Id)
        if ($hwnd -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 50
    }
    if ($hwnd -eq [IntPtr]::Zero) { throw 'Host did not start within 10 seconds.' }
    if (-not [QuitProcessTest]::PostMessage($hwnd, 0x10, [IntPtr]::Zero, [IntPtr]::Zero)) { throw 'WM_CLOSE delivery failed.' }
    if (-not $app.WaitForExit(5000)) { throw 'FAIL: Quit left the host alive after 5 seconds.' }
    if ($app.ExitCode -ne 0) { throw "Host exited with error $($app.ExitCode)." }
    Write-Output "PASS: real host exited cleanly in $([math]::Round($timer.Elapsed.TotalSeconds, 2))s; no model/network/microphone."
} finally {
    if ($app -and -not $app.HasExited) { $app.Kill(); $app.WaitForExit(1000) | Out-Null }
    if ($app) { $app.Dispose() }
    Remove-Item -LiteralPath $work -Recurse -Force
}
