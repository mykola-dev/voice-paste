using System;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Threading.Tasks;

namespace VoicePaste.Transcription;

/// <summary>
/// Calls Python transcription worker (faster-whisper).
/// Handles CUDA/CPU fallback automatically.
/// </summary>
public class TranscriptionService
{
    private readonly string _pythonPath;
    private readonly string _transcribeScriptPath;
    private readonly string _modelSize;
    private readonly string _device;
    private readonly Settings.LanguageMode _languageMode;
    private readonly bool _cudaAutoFallback;
    
    /// <summary>
    /// Fires when CUDA fallback to CPU occurs.
    /// </summary>
    public event EventHandler<string>? CudaFallbackOccurred;

    /// <summary>
    /// Create transcription service.
    /// </summary>
    /// <param name="modelSize">Whisper model (tiny/base/small/medium/large-v3).</param>
    /// <param name="device">Device (cuda/cpu).</param>
    /// <param name="languageMode">Auto/en/ua/bilingual mode.</param>
    /// <param name="cudaAutoFallback">If true, fallback to CPU when CUDA fails.</param>
    public TranscriptionService(
        string modelSize = "medium",
        string device = "cuda",
        Settings.LanguageMode languageMode = Settings.LanguageMode.Auto,
        bool cudaAutoFallback = true)
    {
        _modelSize = modelSize;
        _device = device;
        _languageMode = languageMode;
        _cudaAutoFallback = cudaAutoFallback;

        // Find Python executable
        _pythonPath = PythonFinder.Find();

        // Find transcribe.py script
        _transcribeScriptPath = FindTranscribeScript();

        // Log model info
        Console.WriteLine($"[Transcribe] Model: {_modelSize}, Device: {_device}, LangMode: {_languageMode}");
    }
    
    /// <summary>
    /// Transcribe audio file to text.
    /// </summary>
    /// <param name="audioFilePath">Path to WAV file.</param>
    /// <returns>Transcribed text.</returns>
    /// <exception cref="TranscriptionException">Transcription failed.</exception>
    public async Task<string> TranscribeAsync(string audioFilePath)
    {
        if (!File.Exists(audioFilePath))
            throw new FileNotFoundException($"Audio file not found: {audioFilePath}");
        
        // Try with configured device first
        try
        {
            Console.WriteLine($"[Transcribe] Attempting transcription with device: {_device}");
            return await TranscribeWithDeviceAsync(audioFilePath, _device);
        }
        catch (TranscriptionException ex) when (_device == "cuda" && _cudaAutoFallback && IsCudaError(ex))
        {
            // CUDA failed - fallback to CPU
            var fallbackMessage = $"GPU acceleration unavailable ({GetCudaErrorReason(ex)}), using CPU. This will be slower.";
            Console.WriteLine($"[Transcribe] {fallbackMessage}");
            CudaFallbackOccurred?.Invoke(this, fallbackMessage);
            return await TranscribeWithDeviceAsync(audioFilePath, "cpu");
        }
    }
    
    private static bool IsCudaError(TranscriptionException ex)
    {
        var msg = ex.Message.ToLowerInvariant();
        return msg.Contains("cublas") || 
               msg.Contains("cuda") || 
               msg.Contains("gpu") ||
               msg.Contains("cudnn");
    }
    
    private static string GetCudaErrorReason(TranscriptionException ex)
    {
        var msg = ex.Message.ToLowerInvariant();
        if (msg.Contains("cublas")) return "CUDA libraries not found";
        if (msg.Contains("out of memory")) return "GPU out of memory";
        if (msg.Contains("cudnn")) return "cuDNN not found";
        return "GPU error";
    }
    
    private async Task<string> TranscribeWithDeviceAsync(string audioFilePath, string device)
    {
        var process = new Process
        {
            StartInfo = new ProcessStartInfo
            {
                FileName = _pythonPath,
                Arguments = $"\"{_transcribeScriptPath}\" --input \"{audioFilePath}\" --model {_modelSize} --device {device} --language-mode {ToLanguageModeArg(_languageMode)}",
                UseShellExecute = false,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                CreateNoWindow = true,
                StandardOutputEncoding = Encoding.UTF8,
                StandardErrorEncoding = Encoding.UTF8
            }
        };

        var exeDir = AppDomain.CurrentDomain.BaseDirectory;
        var pythonDir = Path.GetFullPath(Path.Combine(exeDir, "python"));
        var modelsDir = Path.GetFullPath(Path.Combine(exeDir, "models"));

        // Force Python to use UTF-8 for stdout/stderr
        process.StartInfo.Environment["PYTHONIOENCODING"] = "utf-8";
        process.StartInfo.Environment["PYTHONUTF8"] = "1";

        // Portable runtime support without a launcher .bat
        if (Directory.Exists(pythonDir))
        {
            var existingPath = process.StartInfo.Environment["PATH"] ?? Environment.GetEnvironmentVariable("PATH") ?? string.Empty;
            process.StartInfo.Environment["PATH"] = pythonDir + ";" + existingPath;
            process.StartInfo.Environment["PYTHONHOME"] = pythonDir;
        }

        if (Directory.Exists(modelsDir))
        {
            process.StartInfo.Environment["HF_HOME"] = modelsDir;
        }
        
        var outputBuilder = new StringBuilder();
        var errorBuilder = new StringBuilder();
        
        process.OutputDataReceived += (s, e) =>
        {
            if (e.Data != null)
            {
                outputBuilder.AppendLine(e.Data);
                Console.WriteLine($"[Transcribe stdout] {e.Data}");  // Debug output
            }
        };
        
        process.ErrorDataReceived += (s, e) =>
        {
            if (e.Data != null)
            {
                errorBuilder.AppendLine(e.Data);
                Console.WriteLine($"[Transcribe stderr] {e.Data}");  // Debug output
            }
        };
        
        try
        {
            process.Start();
            process.BeginOutputReadLine();
            process.BeginErrorReadLine();
            
            // Wait for completion with timeout (60 seconds)
            bool completed = await Task.Run(() => process.WaitForExit(60000));
            
            if (!completed)
            {
                try
                {
                    process.Kill();
                }
                catch { /* Ignore kill errors */ }
                
                throw new TranscriptionException(
                    "Transcription timed out after 60 seconds. The audio file may be too long or the model is taking too long to process. Try using a smaller model (e.g., 'small' or 'medium') or shorter recordings."
                );
            }
            
            //IMPORTANT: Wait for async output/error reading to complete
            await Task.Run(() => process.WaitForExit());
            
            Console.WriteLine($"[Transcribe] Process exited with code: {process.ExitCode}");
            Console.WriteLine($"[Transcribe] Stdout length: {outputBuilder.Length}");
            Console.WriteLine($"[Transcribe] Stderr length: {errorBuilder.Length}");
            
            if (process.ExitCode != 0)
            {
                var error = errorBuilder.ToString();
                throw new TranscriptionException($"Transcription failed (exit code {process.ExitCode}): {error}");
            }
            
            string transcript = outputBuilder.ToString().Trim();
            Console.WriteLine($"[Transcribe] Transcript length after trim: {transcript.Length}");
            Console.WriteLine($"[Transcribe] Transcript content: '{transcript}'");
            
            if (string.IsNullOrWhiteSpace(transcript))
            {
                // Empty transcript - either silence or error
                var stderr = errorBuilder.ToString();
                Console.WriteLine($"[Transcribe] WARNING: Empty transcript.");
                Console.WriteLine($"[Transcribe] Full stdout: '{outputBuilder.ToString()}'");
                Console.WriteLine($"[Transcribe] Full stderr: '{stderr}'");
                return string.Empty;
            }
            
            return transcript;
        }
        finally
        {
            process.Dispose();
        }
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
        // Look for transcribe.py relative to executable
        var exeDir = AppDomain.CurrentDomain.BaseDirectory;
        var candidates = new[]
        {
            Path.Combine(exeDir, "transcribe", "transcribe.py"),
            // Dev fallback: from bin/Debug/net8.0-windows
            Path.Combine(exeDir, "..", "..", "..", "..", "src", "transcribe", "transcribe.py"),
            Path.Combine(exeDir, "..", "..", "..", "..", "..", "src", "transcribe", "transcribe.py")
        };
        
        foreach (var candidate in candidates)
        {
            var fullPath = Path.GetFullPath(candidate);
            if (File.Exists(fullPath))
                return fullPath;
        }
        
        throw new TranscriptionException(
            "transcribe.py not found. Please ensure the transcription worker is installed."
        );
    }
}

/// <summary>
/// Exception thrown when transcription fails.
/// </summary>
public class TranscriptionException : Exception
{
    public TranscriptionException(string message) : base(message) { }
    public TranscriptionException(string message, Exception inner) : base(message, inner) { }
}
