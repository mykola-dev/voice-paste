using System.Diagnostics;
using System.Globalization;
using System.IO;

namespace VoicePaste.Transcription;

internal static class GpuMemoryReader
{
    public static async Task<long?> ReadAsync(int processId, CancellationToken cancellationToken)
    {
        using var process = new Process
        {
            StartInfo = new ProcessStartInfo
            {
                FileName = "nvidia-smi",
                ArgumentList = { "--query-compute-apps=pid,used_gpu_memory", "--format=csv,noheader,nounits" },
                UseShellExecute = false,
                CreateNoWindow = true,
                RedirectStandardOutput = true,
                RedirectStandardError = true
            }
        };
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(TimeSpan.FromSeconds(2));
        try
        {
            process.Start();
            var output = process.StandardOutput.ReadToEndAsync(timeout.Token);
            var error = process.StandardError.ReadToEndAsync(timeout.Token);
            await process.WaitForExitAsync(timeout.Token).ConfigureAwait(false);
            await error.ConfigureAwait(false);
            var csv = await output.ConfigureAwait(false);
            return process.ExitCode == 0 ? Parse(csv, processId) : null;
        }
        catch (Exception ex) when (ex is System.ComponentModel.Win32Exception or InvalidOperationException or OperationCanceledException or IOException)
        {
            try { if (!process.HasExited) process.Kill(entireProcessTree: true); }
            catch (InvalidOperationException) { }
            catch (System.ComponentModel.Win32Exception) { }
            cancellationToken.ThrowIfCancellationRequested();
            return null;
        }
    }

    internal static long? Parse(string csv, int processId)
    {
        long total = 0;
        var found = false;
        foreach (var line in csv.Split('\n', StringSplitOptions.RemoveEmptyEntries))
        {
            var cells = line.Split(',');
            if (cells.Length != 2 || !int.TryParse(cells[0].Trim(), out var pid) || pid != processId) continue;
            // WDDM often reports N/A. Never substitute whole-card usage or treat unknown as zero.
            if (!long.TryParse(cells[1].Trim(), NumberStyles.None, CultureInfo.InvariantCulture, out var mib) ||
                mib > long.MaxValue / (1024 * 1024)) return null;
            var bytes = mib * 1024 * 1024;
            if (total > long.MaxValue - bytes) return null;
            total += bytes;
            found = true;
        }
        return found ? total : null;
    }
}
