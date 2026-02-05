using System;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Threading.Tasks;

namespace VoicePaste.Transcription;

/// <summary>
/// Calls Python transcription worker (faster-whisper).
/// Handles CUDA/CPU fallback automatically.
/// Support asynchronous pre-loading while recording.
/// </summary>
public class TranscriptionService : IDisposable
{
    private readonly string _pythonPath;
    private readonly string _transcribeScriptPath;
    private readonly string _modelSize;
    private readonly string _device;
    private readonly Settings.LanguageMode _languageMode;
    private readonly int _beamSize;
    private readonly bool _cudaAutoFallback;
    private readonly string _initialPrompt;

    private Process? _activeProcess;
    private TaskCompletionSource<bool>? _readyTcs;
    private TaskCompletionSource<string>? _resultTcs;
    private StringBuilder _errorBuffer = new();

    /// <summary>
    /// Fires when CUDA fallback to CPU occurs.
    /// </summary>
    public event EventHandler<string>? CudaFallbackOccurred;

    public TranscriptionService(
        string modelSize = "medium",
        string device = "cuda",
        Settings.LanguageMode languageMode = Settings.LanguageMode.Auto,
        int beamSize = 5,
        bool cudaAutoFallback = true,
        string initialPrompt = "")
    {
        _modelSize = modelSize;
        _device = device;
        _languageMode = languageMode;
        _beamSize = beamSize;
        _cudaAutoFallback = cudaAutoFallback;
        _initialPrompt = initialPrompt;

        _pythonPath = PythonFinder.Find();
        _transcribeScriptPath = FindTranscribeScript();

        Console.WriteLine($"[Transcribe] Python: {_pythonPath}");
        Console.WriteLine($"[Transcribe] Model: {_modelSize}, Device: {_device}, LangMode: {_languageMode}, BeamSize: {_beamSize}");
    }

    /// <summary>
    /// Starts the Python process and loads the model in the background.
    /// Call this when recording starts to hide the model loading time.
    /// </summary>
    public void StartPreloading()
    {
        // Cancel any existing process
        CleanupProcess();

        _readyTcs = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        _resultTcs = new TaskCompletionSource<string>(TaskCreationOptions.RunContinuationsAsynchronously);
        _errorBuffer.Clear();

        _activeProcess = CreateProcess(_device, waitMode: true);
        
        _activeProcess.OutputDataReceived += (s, e) =>
        {
            if (e.Data == null) return;
            Console.WriteLine($"[Transcribe stdout] {e.Data}");

            if (e.Data == "READY")
            {
                _readyTcs.TrySetResult(true);
            }
            else if (!string.IsNullOrWhiteSpace(e.Data))
            {
                _resultTcs.TrySetResult(e.Data);
            }
        };

        _activeProcess.ErrorDataReceived += (s, e) =>
        {
            if (e.Data == null) return;
            Console.WriteLine($"[Transcribe stderr] {e.Data}");
            _errorBuffer.AppendLine(e.Data);

            if (e.Data.Contains("CUDA failed") && _device == "cuda" && _cudaAutoFallback)
            {
                // We'll handle fallback by killing and restarting with CPU if needed,
                // but usually the process will print an error or READY eventually.
            }
        };

        try
        {
            _activeProcess.Start();
            _activeProcess.BeginOutputReadLine();
            _activeProcess.BeginErrorReadLine();
        }
        catch (Exception ex)
        {
            _readyTcs.TrySetException(ex);
            _resultTcs.TrySetException(ex);
        }
    }

    /// <summary>
    /// Transcribe audio file to text. 
    /// If preloading was started, it waits for the model to be ready.
    /// </summary>
    public async Task<string> TranscribeAsync(string audioFilePath)
    {
        if (!File.Exists(audioFilePath))
            throw new FileNotFoundException($"Audio file not found: {audioFilePath}");

        // If no process is running or it exited, start a one-off or a fresh one
        if (_activeProcess == null || _activeProcess.HasExited)
        {
            Console.WriteLine("[Transcribe] No preloaded process found. Starting one-off transcription.");
            return await TranscribeOneOffAsync(audioFilePath, _device);
        }

        try
        {
            // Wait for model to be READY (up to 30s)
            var readyTask = _readyTcs?.Task ?? Task.FromResult(false);
            var completedTask = await Task.WhenAny(readyTask, Task.Delay(30000));

            if (completedTask != readyTask || !await readyTask)
            {
                Console.WriteLine("[Transcribe] Preloading timed out or failed. Falling back to one-off.");
                CleanupProcess();
                return await TranscribeOneOffAsync(audioFilePath, _device);
            }

            // Send filename to stdin
            Console.WriteLine($"[Transcribe] Model ready. Sending file for transcription: {audioFilePath}");
            var startTime = DateTime.Now;
            await _activeProcess.StandardInput.WriteLineAsync(audioFilePath);
            await _activeProcess.StandardInput.FlushAsync();

            // Wait for result
            var resultTask = _resultTcs!.Task;
            var timeoutTask = Task.Delay(60000);
            var finishedTask = await Task.WhenAny(resultTask, timeoutTask);

            if (finishedTask == timeoutTask)
            {
                throw new TranscriptionException("Transcription timed out after 60 seconds.");
            }

            var transcript = await resultTask;
            var duration = DateTime.Now - startTime;
            Console.WriteLine($"[Transcribe] Transcription total time (including IPC): {duration.TotalMilliseconds:F0}ms");
            
            if (string.IsNullOrWhiteSpace(transcript) && _errorBuffer.Length > 0)
            {
                var error = _errorBuffer.ToString();
                if (IsCudaError(error) && _device == "cuda" && _cudaAutoFallback)
                {
                    var fallbackMessage = "GPU acceleration failed, falling back to CPU.";
                    CudaFallbackOccurred?.Invoke(this, fallbackMessage);
                    CleanupProcess();
                    return await TranscribeOneOffAsync(audioFilePath, "cpu");
                }
                throw new TranscriptionException($"Transcription failed: {error}");
            }

            return transcript.Trim();
        }
        finally
        {
            CleanupProcess();
        }
    }

    private async Task<string> TranscribeOneOffAsync(string audioFilePath, string device)
    {
        var startTime = DateTime.Now;
        using var process = CreateProcess(device, waitMode: false, audioFilePath: audioFilePath);
        var output = new StringBuilder();
        var error = new StringBuilder();

        process.OutputDataReceived += (s, e) => { if (e.Data != null) output.AppendLine(e.Data); };
        process.ErrorDataReceived += (s, e) => { if (e.Data != null) error.AppendLine(e.Data); };

        process.Start();
        process.BeginOutputReadLine();
        process.BeginErrorReadLine();

        bool completed = await Task.Run(() => process.WaitForExit(60000));
        if (!completed)
        {
            try { process.Kill(); } catch { }
            throw new TranscriptionException("Transcription timed out.");
        }

        await Task.Run(() => process.WaitForExit());

        if (process.ExitCode != 0)
        {
            var errMsg = error.ToString();
            if (IsCudaError(errMsg) && device == "cuda" && _cudaAutoFallback)
            {
                CudaFallbackOccurred?.Invoke(this, "GPU acceleration failed, falling back to CPU.");
                return await TranscribeOneOffAsync(audioFilePath, "cpu");
            }
            throw new TranscriptionException($"Transcription failed (exit code {process.ExitCode}): {errMsg}");
        }

        var duration = DateTime.Now - startTime;
        Console.WriteLine($"[Transcribe] One-off transcription total time: {duration.TotalMilliseconds:F0}ms");

        return output.ToString().Trim();
    }

    private Process CreateProcess(string device, bool waitMode, string? audioFilePath = null)
    {
        var args = new StringBuilder();
        args.Append($"\"{_transcribeScriptPath}\" ");
        args.Append($"--model {_modelSize} ");
        args.Append($"--device {device} ");
        args.Append($"--language-mode {ToLanguageModeArg(_languageMode)} ");
        args.Append($"--beam-size {_beamSize} ");

        if (!string.IsNullOrWhiteSpace(_initialPrompt))
        {
            args.Append($"--initial-prompt \"{_initialPrompt.Replace("\"", "\\\"")}\" ");
        }

        if (waitMode)
        {
            args.Append("--wait ");
        }
        else if (audioFilePath != null)
        {
            args.Append($"--input \"{audioFilePath}\" ");
        }

        var process = new Process
        {
            StartInfo = new ProcessStartInfo
            {
                FileName = _pythonPath,
                Arguments = args.ToString(),
                UseShellExecute = false,
                RedirectStandardInput = true,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                CreateNoWindow = true,
                StandardOutputEncoding = Encoding.UTF8,
                StandardErrorEncoding = Encoding.UTF8
            }
        };

        var exeDir = AppDomain.CurrentDomain.BaseDirectory;
        var pythonDir = Path.GetFullPath(Path.Combine(exeDir, "python"));

        process.StartInfo.Environment["PYTHONIOENCODING"] = "utf-8";
        process.StartInfo.Environment["PYTHONUTF8"] = "1";
        if (Environment.GetEnvironmentVariable("VOICEPASTE_DEBUG") == "1")
        {
            process.StartInfo.Environment["VOICEPASTE_DEBUG"] = "1";
        }

        if (Directory.Exists(pythonDir))
        {
            var existingPath = process.StartInfo.Environment["PATH"] ?? Environment.GetEnvironmentVariable("PATH") ?? string.Empty;
            process.StartInfo.Environment["PATH"] = pythonDir + ";" + existingPath;
            process.StartInfo.Environment["PYTHONHOME"] = pythonDir;
        }

        // Use standard HF user cache (~/.cache/huggingface), not bundled models
        // HF_HOME is NOT set, allowing faster-whisper to use its default location

        return process;
    }

    private void CleanupProcess()
    {
        if (_activeProcess != null)
        {
            try
            {
                if (!_activeProcess.HasExited)
                {
                    _activeProcess.StandardInput.WriteLine("QUIT");
                    if (!_activeProcess.WaitForExit(500))
                        _activeProcess.Kill();
                }
            }
            catch { }
            finally
            {
                _activeProcess.Dispose();
                _activeProcess = null;
            }
        }
    }

    private static bool IsCudaError(string msg)
    {
        msg = msg.ToLowerInvariant();
        return msg.Contains("cublas") || msg.Contains("cuda") || msg.Contains("gpu") || msg.Contains("cudnn");
    }

    private static string ToLanguageModeArg(Settings.LanguageMode mode)
    {
        return mode switch
        {
            Settings.LanguageMode.Auto => "auto",
            Settings.LanguageMode.En => "en",
            Settings.LanguageMode.Ua => "ua",
            Settings.LanguageMode.Bilingual => "bilingual",
            _ => "auto"
        };
    }

    private static string FindTranscribeScript()
    {
        var exeDir = AppDomain.CurrentDomain.BaseDirectory;
        var candidates = new[]
        {
            Path.Combine(exeDir, "transcribe", "transcribe.py"),
            Path.Combine(exeDir, "..", "..", "..", "..", "src", "transcribe", "transcribe.py"),
            Path.Combine(exeDir, "..", "..", "..", "..", "..", "src", "transcribe", "transcribe.py")
        };
        
        foreach (var candidate in candidates)
        {
            var fullPath = Path.GetFullPath(candidate);
            if (File.Exists(fullPath)) return fullPath;
        }
        
        throw new TranscriptionException("transcribe.py not found.");
    }

    public void Dispose()
    {
        CleanupProcess();
    }
}

public class TranscriptionException : Exception
{
    public TranscriptionException(string message) : base(message) { }
    public TranscriptionException(string message, Exception inner) : base(message, inner) { }
}
