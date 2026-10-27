using System;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using VoicePaste.Logging;

namespace VoicePaste.Transcription;

/// <summary>Owns a single faster-whisper worker, optionally retained between dictations.</summary>
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
    private readonly bool _enableVad;
    private readonly Func<string, Process>? _processFactory;
    private readonly TimeSpan _readyTimeout;
    private readonly TimeSpan _resultTimeout;
    private readonly object _sync = new();
    private readonly SemaphoreSlim _operationGate = new(1, 1);
    private Worker? _worker;
    private string _workerDevice;
    private bool _keepInMemory;
    private bool _requestInProgress;
    private bool _disposed;
    private long _preloadVersion;
    private long _preloadTaskVersion;
    private Task? _preloadTask;

    private sealed class Worker
    {
        public Process Process { get; }
        public TaskCompletionSource<string> Ready { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource<string>? Result;
        public string Device { get; set; }
        private readonly StringBuilder _errors = new();

        public Worker(Process process, string device)
        {
            Process = process;
            Device = device;
            // Disposal/exit may fault readiness before anyone awaits it.
            _ = Ready.Task.ContinueWith(task => _ = task.Exception, TaskContinuationOptions.OnlyOnFaulted);
        }

        public void AddError(string error)
        {
            lock (_errors)
            {
                // A resident process must not accumulate unbounded diagnostic output.
                if (_errors.Length > 16000) _errors.Clear();
                _errors.AppendLine(error);
            }
        }

        public string Errors { get { lock (_errors) return _errors.ToString(); } }

        public void Fail(Exception error)
        {
            Ready.TrySetException(error);
            Volatile.Read(ref Result)?.TrySetException(error);
        }
    }

    public event EventHandler<string>? CudaFallbackOccurred;
    public event EventHandler<string>? PreloadingFailed;

    public TranscriptionService(
        string modelSize = "medium",
        string device = "cuda",
        Settings.LanguageMode languageMode = Settings.LanguageMode.Auto,
        int beamSize = 5,
        bool cudaAutoFallback = true,
        string initialPrompt = "",
        bool enableVad = false,
        bool keepInMemory = false)
        : this(modelSize, device, languageMode, beamSize, cudaAutoFallback, initialPrompt,
            enableVad, keepInMemory, null, TimeSpan.FromSeconds(30), TimeSpan.FromSeconds(60))
    {
    }

    internal TranscriptionService(
        string modelSize, string device, Settings.LanguageMode languageMode, int beamSize,
        bool cudaAutoFallback, string initialPrompt, bool enableVad, bool keepInMemory,
        Func<string, Process>? processFactory, TimeSpan readyTimeout, TimeSpan resultTimeout)
    {
        _modelSize = modelSize;
        _device = device;
        _workerDevice = device;
        _languageMode = languageMode;
        _beamSize = beamSize;
        _cudaAutoFallback = cudaAutoFallback;
        _initialPrompt = initialPrompt;
        _enableVad = enableVad;
        _keepInMemory = keepInMemory;
        _processFactory = processFactory;
        _readyTimeout = readyTimeout;
        _resultTimeout = resultTimeout;
        _pythonPath = processFactory == null ? PythonFinder.Find() : string.Empty;
        _transcribeScriptPath = processFactory == null ? FindTranscribeScript() : string.Empty;
    }

    /// <summary>Idempotent: recording never replaces a loading or ready worker.</summary>
    public void StartPreloading()
    {
        lock (_sync)
        {
            ObjectDisposedException.ThrowIf(_disposed, this);
            if (_preloadTask is { IsCompleted: false } && _preloadTaskVersion == _preloadVersion) return;
            _preloadTaskVersion = _preloadVersion;
            _preloadTask = PreloadAsync(_preloadVersion);
        }
    }

    private async Task PreloadAsync(long version)
    {
        await _operationGate.WaitAsync().ConfigureAwait(false);
        try
        {
            await GetReadyWorkerAsync(version).ConfigureAwait(false);
        }
        catch (ObjectDisposedException) { }
        catch (Exception ex)
        {
            Logger.Warning("Transcribe", $"Preloading failed: {ex.Message}");
            PreloadingFailed?.Invoke(this, ex.Message);
        }
        finally
        {
            _operationGate.Release();
        }
    }

    public void SetKeepInMemory(bool keepInMemory)
    {
        Worker? retired = null;
        lock (_sync)
        {
            ObjectDisposedException.ThrowIf(_disposed, this);
            _keepInMemory = keepInMemory;
            if (!keepInMemory) _preloadVersion++;
            if (!keepInMemory && !_requestInProgress)
            {
                retired = _worker;
                _worker = null;
                _workerDevice = _device;
            }
        }
        StopWorker(retired);
    }

    public async Task<string> TranscribeAsync(string audioFilePath)
    {
        if (!File.Exists(audioFilePath))
            throw new FileNotFoundException($"Audio file not found: {audioFilePath}");
        if (audioFilePath.Contains('\n') || audioFilePath.Contains('\r'))
            throw new ArgumentException("Audio path cannot contain newlines.", nameof(audioFilePath));

        await _operationGate.WaitAsync().ConfigureAwait(false);
        try
        {
            lock (_sync)
            {
                ObjectDisposedException.ThrowIf(_disposed, this);
                _requestInProgress = true;
            }

            var worker = await GetReadyWorkerAsync().ConfigureAwait(false);
            try
            {
                return await SendRequestAsync(worker, audioFilePath).ConfigureAwait(false);
            }
            catch (Exception ex) when (ex is not ObjectDisposedException && CanFallback(worker.Device, ex.Message))
            {
                StopCurrentWorker();
                UseCpuFallback();
                worker = await GetReadyWorkerAsync().ConfigureAwait(false);
                return await SendRequestAsync(worker, audioFilePath).ConfigureAwait(false);
            }
        }
        catch
        {
            // Never reuse a failed/timed-out protocol stream: a late result could reach the next request.
            await Task.Run(StopCurrentWorker).ConfigureAwait(false);
            throw;
        }
        finally
        {
            Worker? retired = null;
            lock (_sync)
            {
                _requestInProgress = false;
                if (!_keepInMemory)
                {
                    retired = _worker;
                    _worker = null;
                    _workerDevice = _device;
                }
            }
            if (retired != null) await Task.Run(() => StopWorker(retired)).ConfigureAwait(false);
            _operationGate.Release();
        }
    }

    private async Task<string> SendRequestAsync(Worker worker, string audioFilePath)
    {
        var result = new TaskCompletionSource<string>(TaskCreationOptions.RunContinuationsAsynchronously);
        Volatile.Write(ref worker.Result, result);
        try
        {
            if (worker.Process.HasExited)
                throw new TranscriptionException($"Worker exited: {worker.Errors}");
            await worker.Process.StandardInput.WriteLineAsync(audioFilePath).ConfigureAwait(false);
            await worker.Process.StandardInput.FlushAsync().ConfigureAwait(false);
            return (await result.Task.WaitAsync(_resultTimeout).ConfigureAwait(false)).Trim();
        }
        catch (TimeoutException ex)
        {
            throw new TranscriptionException("Transcription timed out.", ex);
        }
        finally
        {
            Volatile.Write(ref worker.Result, null);
            // Writing stdin can fail concurrently with an exit notification.
            if (result.Task.IsFaulted) _ = result.Task.Exception;
        }
    }

    private async Task<Worker> GetReadyWorkerAsync(long? preloadVersion = null)
    {
        var worker = EnsureWorker(preloadVersion);
        try
        {
            await worker.Ready.Task.WaitAsync(_readyTimeout).ConfigureAwait(false);
            return worker;
        }
        catch (Exception ex)
        {
            var fallback = ex is not (ObjectDisposedException or TimeoutException) && CanFallback(worker.Device, ex.Message);
            StopCurrentWorker();
            if (!fallback) throw;
            UseCpuFallback(preloadVersion);
            worker = EnsureWorker(preloadVersion);
            try
            {
                await worker.Ready.Task.WaitAsync(_readyTimeout).ConfigureAwait(false);
                return worker;
            }
            catch
            {
                StopCurrentWorker();
                throw;
            }
        }
    }

    private Worker EnsureWorker(long? preloadVersion = null)
    {
        lock (_sync)
        {
            ObjectDisposedException.ThrowIf(_disposed, this);
            if (preloadVersion.HasValue && preloadVersion != _preloadVersion)
                throw new ObjectDisposedException("Cancelled model preload");
            if (_worker != null && !_worker.Process.HasExited) return _worker;
            StopWorker(_worker);
            _worker = null;
            var process = _processFactory?.Invoke(_workerDevice) ?? CreateProcess(_workerDevice);
            var worker = new Worker(process, _workerDevice);
            process.OutputDataReceived += (_, e) => HandleOutput(worker, e.Data);
            process.ErrorDataReceived += (_, e) =>
            {
                if (e.Data == null) return;
                Logger.Debug("Transcribe", e.Data);
                worker.AddError(e.Data);
            };
            process.EnableRaisingEvents = true;
            process.Exited += (_, _) => _ = CompleteExitAsync(worker);
            _worker = worker;
            try
            {
                process.Start();
                process.BeginOutputReadLine();
                process.BeginErrorReadLine();
            }
            catch
            {
                _worker = null;
                StopWorker(worker);
                throw;
            }
            return worker;
        }
    }

    private void HandleOutput(Worker worker, string? line)
    {
        if (line == null) return;
        try
        {
            using var message = JsonDocument.Parse(line);
            var root = message.RootElement;
            switch (root.GetProperty("type").GetString())
            {
                case "ready":
                    var device = root.GetProperty("device").GetString();
                    if (device != "cpu" && device != "cuda") throw new JsonException("Invalid worker device.");
                    var fellBack = worker.Device == "cuda" && device == "cpu";
                    worker.Device = device;
                    if (fellBack && worker.Ready.TrySetResult(device))
                        CudaFallbackOccurred?.Invoke(this, "GPU acceleration failed, falling back to CPU.");
                    else
                        worker.Ready.TrySetResult(device);
                    break;
                case "result":
                    Volatile.Read(ref worker.Result)?.TrySetResult(root.GetProperty("text").GetString() ?? string.Empty);
                    break;
                case "error":
                    var error = root.GetProperty("message").GetString() ?? "Unknown worker error";
                    worker.Fail(new TranscriptionException(error));
                    break;
                default:
                    throw new JsonException("Unknown worker message.");
            }
        }
        catch (Exception ex) when (ex is JsonException or InvalidOperationException or KeyNotFoundException)
        {
            worker.Fail(new TranscriptionException("Invalid transcription worker response.", ex));
        }
    }

    private static async Task CompleteExitAsync(Worker worker)
    {
        // Drain buffered stdout/stderr before reporting exit, so a final response/error is not lost.
        try { await Task.Run(() => worker.Process.WaitForExit()).ConfigureAwait(false); }
        catch (InvalidOperationException) { }
        worker.Fail(new TranscriptionException($"Transcription worker exited. {worker.Errors}"));
    }

    private bool CanFallback(string device, string error) => device == "cuda" && _cudaAutoFallback && IsCudaError(error);

    private void UseCpuFallback(long? preloadVersion = null)
    {
        lock (_sync)
        {
            ObjectDisposedException.ThrowIf(_disposed, this);
            if (preloadVersion.HasValue && preloadVersion != _preloadVersion)
                throw new ObjectDisposedException("Cancelled model preload");
            _workerDevice = "cpu";
        }
        CudaFallbackOccurred?.Invoke(this, "GPU acceleration failed, falling back to CPU.");
    }

    public async Task<WorkerMemoryUsage> GetMemoryUsageAsync(CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        Worker worker;
        int pid;
        long? ram;
        bool ready;
        lock (_sync)
        {
            if (_worker == null || _worker.Process.HasExited) return WorkerMemoryUsage.NotLoaded;
            worker = _worker;
            pid = worker.Process.Id;
            try
            {
                worker.Process.Refresh();
                ram = worker.Process.WorkingSet64;
            }
            catch (System.ComponentModel.Win32Exception) { ram = null; }
            catch (InvalidOperationException) { return WorkerMemoryUsage.NotLoaded; }
            ready = worker.Ready.Task.IsCompletedSuccessfully;
        }
        var vram = worker.Device == "cuda"
            ? await GpuMemoryReader.ReadAsync(pid, cancellationToken).ConfigureAwait(false)
            : null;
        lock (_sync)
        {
            if (_worker != worker || worker.Process.HasExited) return WorkerMemoryUsage.NotLoaded;
            return new WorkerMemoryUsage(_modelSize, worker.Device, pid, ready, ram, vram);
        }
    }

    private Process CreateProcess(string device)
    {
        var info = new ProcessStartInfo
        {
            FileName = _pythonPath,
            UseShellExecute = false,
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            CreateNoWindow = true,
            StandardInputEncoding = new UTF8Encoding(false),
            StandardOutputEncoding = Encoding.UTF8,
            StandardErrorEncoding = Encoding.UTF8
        };
        foreach (var arg in new[] { _transcribeScriptPath, "--model", _modelSize, "--device", device,
            "--language-mode", ToLanguageModeArg(_languageMode), "--beam-size", _beamSize.ToString(), "--wait" })
            info.ArgumentList.Add(arg);
        if (!_cudaAutoFallback && device == "cuda") info.ArgumentList.Add("--no-fallback");
        if (_enableVad) info.ArgumentList.Add("--vad");
        if (!string.IsNullOrWhiteSpace(_initialPrompt))
        {
            info.ArgumentList.Add("--initial-prompt");
            info.ArgumentList.Add(_initialPrompt);
        }
        info.Environment["PYTHONIOENCODING"] = "utf-8";
        info.Environment["PYTHONUTF8"] = "1";
        info.Environment["VOICEPASTE_DEBUG"] = Logger.IsDebugMode ? "1" : "0";
        var pythonDir = Path.GetFullPath(Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "python"));
        if (Directory.Exists(pythonDir))
        {
            info.Environment["PATH"] = pythonDir + ";" + (info.Environment["PATH"] ?? string.Empty);
            info.Environment["PYTHONHOME"] = pythonDir;
        }
        return new Process { StartInfo = info };
    }

    private void StopCurrentWorker()
    {
        Worker? worker;
        lock (_sync)
        {
            worker = _worker;
            _worker = null;
        }
        StopWorker(worker);
    }

    private static void StopWorker(Worker? worker)
    {
        if (worker == null) return;
        worker.Fail(new ObjectDisposedException("Transcription worker"));
        try
        {
            if (!worker.Process.HasExited)
            {
                worker.Process.Kill(entireProcessTree: true);
                worker.Process.WaitForExit(2000);
            }
        }
        catch (InvalidOperationException) { }
        catch (System.ComponentModel.Win32Exception) { }
        finally { worker.Process.Dispose(); }
    }

    private static bool IsCudaError(string message)
    {
        var error = message.ToLowerInvariant();
        return error.Contains("cublas") || error.Contains("cuda") || error.Contains("gpu") || error.Contains("cudnn");
    }

    private static string ToLanguageModeArg(Settings.LanguageMode mode) => mode switch
    {
        Settings.LanguageMode.En => "en",
        Settings.LanguageMode.Ua => "ua",
        Settings.LanguageMode.Bilingual => "bilingual",
        _ => "auto"
    };

    private static string FindTranscribeScript()
    {
        var exeDir = AppDomain.CurrentDomain.BaseDirectory;
        foreach (var candidate in new[]
        {
            Path.Combine(exeDir, "transcribe", "transcribe.py"),
            Path.Combine(exeDir, "..", "..", "..", "..", "src", "transcribe", "transcribe.py"),
            Path.Combine(exeDir, "..", "..", "..", "..", "..", "src", "transcribe", "transcribe.py")
        })
        {
            var path = Path.GetFullPath(candidate);
            if (File.Exists(path)) return path;
        }
        throw new TranscriptionException("transcribe.py not found.");
    }

    public void Dispose()
    {
        lock (_sync)
        {
            _disposed = true;
            _preloadVersion++;
        }
        StopCurrentWorker();
    }
}

public class TranscriptionException : Exception
{
    public TranscriptionException(string message) : base(message) { }
    public TranscriptionException(string message, Exception inner) : base(message, inner) { }
}
