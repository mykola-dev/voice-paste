using System;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;

namespace VoicePaste.Transcription;

public class ModelDownloadProgress
{
    public string ModelName { get; init; } = string.Empty;
    public int Percentage { get; init; }
    public string Status { get; init; } = string.Empty;
}

public class ModelDownloadService
{
    private readonly string _pythonPath;
    private static readonly string[] RequiredModels = { "large-v2", "large-v3-turbo" };

    public event EventHandler<ModelDownloadProgress>? ProgressChanged;

    public ModelDownloadService()
    {
        _pythonPath = PythonFinder.Find();
        Console.WriteLine($"[ModelDownload] Python: {_pythonPath}");
    }

    /// <summary>
    /// Check if all required models are cached in HF user cache.
    /// </summary>
    public bool AllModelsPresent()
    {
        var userProfile = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var hfCacheDir = Path.Combine(userProfile, ".cache", "huggingface", "hub");

        if (!Directory.Exists(hfCacheDir))
            return false;

        foreach (var model in RequiredModels)
        {
            if (!IsModelCached(hfCacheDir, model))
                return false;
        }

        return true;
    }

    /// <summary>
    /// Get list of missing models.
    /// </summary>
    public string[] GetMissingModels()
    {
        var userProfile = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var hfCacheDir = Path.Combine(userProfile, ".cache", "huggingface", "hub");

        var missing = new System.Collections.Generic.List<string>();

        foreach (var model in RequiredModels)
        {
            if (!Directory.Exists(hfCacheDir) || !IsModelCached(hfCacheDir, model))
                missing.Add(model);
        }

        return missing.ToArray();
    }

    /// <summary>
    /// Download all missing models.
    /// </summary>
    public async Task DownloadMissingModelsAsync(CancellationToken cancellationToken = default)
    {
        var missing = GetMissingModels();

        for (int i = 0; i < missing.Length; i++)
        {
            var model = missing[i];
            ProgressChanged?.Invoke(this, new ModelDownloadProgress
            {
                ModelName = model,
                Percentage = (i * 100) / missing.Length,
                Status = $"Downloading {model}..."
            });

            await DownloadModelAsync(model, cancellationToken);

            ProgressChanged?.Invoke(this, new ModelDownloadProgress
            {
                ModelName = model,
                Percentage = ((i + 1) * 100) / missing.Length,
                Status = $"Downloaded {model}"
            });
        }
    }

    private async Task DownloadModelAsync(string modelName, CancellationToken cancellationToken)
    {
        var scriptPath = Path.Combine(Path.GetTempPath(), $"voicepaste-download-{Guid.NewGuid():N}.py");

        try
        {
            var script = @"
import sys
model = sys.argv[1]

print(f'Downloading model: {model}', flush=True)

try:
    from faster_whisper import WhisperModel
    # Download to default HF cache by loading the model
    WhisperModel(model, device='cpu', compute_type='int8')
    print('DONE', flush=True)
except Exception as e:
    print(f'ERROR: {e}', file=sys.stderr, flush=True)
    sys.exit(1)
";

            await File.WriteAllTextAsync(scriptPath, script, Encoding.UTF8, cancellationToken);

            var psi = new ProcessStartInfo
            {
                FileName = _pythonPath,
                ArgumentList = { scriptPath, modelName },
                UseShellExecute = false,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                CreateNoWindow = true,
                StandardOutputEncoding = Encoding.UTF8,
                StandardErrorEncoding = Encoding.UTF8
            };

            psi.Environment["PYTHONUTF8"] = "1";
            psi.Environment["PYTHONIOENCODING"] = "utf-8";
            psi.Environment["HF_HUB_DISABLE_SYMLINKS_WARNING"] = "1";
            if (Environment.GetEnvironmentVariable("VOICEPASTE_DEBUG") == "1")
            {
                psi.Environment["VOICEPASTE_DEBUG"] = "1";
            }

            // Set PYTHONHOME if bundled Python exists
            var exeDir = AppDomain.CurrentDomain.BaseDirectory;
            var pythonDir = Path.GetFullPath(Path.Combine(exeDir, "python"));
            if (Directory.Exists(pythonDir))
            {
                var existingPath = psi.Environment["PATH"] ?? Environment.GetEnvironmentVariable("PATH") ?? string.Empty;
                psi.Environment["PATH"] = pythonDir + ";" + existingPath;
                psi.Environment["PYTHONHOME"] = pythonDir;
            }

            using var process = new Process { StartInfo = psi };

            var output = new StringBuilder();
            var error = new StringBuilder();

            process.OutputDataReceived += (_, args) =>
            {
                if (args.Data == null) return;
                output.AppendLine(args.Data);
                Console.WriteLine($"[ModelDownload] {args.Data}");
            };

            process.ErrorDataReceived += (_, args) =>
            {
                if (args.Data == null) return;
                if (IsBenignHubWarning(args.Data))
                {
                    Console.WriteLine($"[ModelDownload] {args.Data}");
                    return;
                }

                error.AppendLine(args.Data);
                Console.WriteLine($"[ModelDownload ERROR] {args.Data}");
            };

            process.Start();
            process.BeginOutputReadLine();
            process.BeginErrorReadLine();

            // Wait for completion with cancellation support
            await process.WaitForExitAsync(cancellationToken);

            if (process.ExitCode != 0)
            {
                throw new ModelDownloadException($"Failed to download model '{modelName}': {error}");
            }

            if (!output.ToString().Contains("DONE"))
            {
                throw new ModelDownloadException($"Model download incomplete for '{modelName}'");
            }
        }
        finally
        {
            try
            {
                if (File.Exists(scriptPath))
                    File.Delete(scriptPath);
            }
            catch
            {
                // Ignore cleanup errors
            }
        }
    }

    private static bool IsModelCached(string hfCacheDir, string modelName)
    {
        try
        {
            // Check for directory pattern: models--Systran--faster-whisper-<modelName>
            var dirs = Directory.GetDirectories(hfCacheDir, "*faster-whisper*", SearchOption.TopDirectoryOnly);
            foreach (var dir in dirs)
            {
                var dirName = Path.GetFileName(dir);
                if (dirName.Contains(modelName, StringComparison.OrdinalIgnoreCase))
                {
                    // Verify snapshots folder exists (indicates successful download)
                    var snapshotsDir = Path.Combine(dir, "snapshots");
                    if (Directory.Exists(snapshotsDir) && Directory.GetDirectories(snapshotsDir).Length > 0)
                    {
                        return true;
                    }
                }
            }
            return false;
        }
        catch
        {
            return false;
        }
    }

    private static bool IsBenignHubWarning(string line)
    {
        return line.Contains("unauthenticated requests to the HF Hub", StringComparison.OrdinalIgnoreCase)
            || line.Contains("Please set a HF_TOKEN", StringComparison.OrdinalIgnoreCase);
    }
}

public class ModelDownloadException : Exception
{
    public ModelDownloadException(string message) : base(message) { }
    public ModelDownloadException(string message, Exception inner) : base(message, inner) { }
}
