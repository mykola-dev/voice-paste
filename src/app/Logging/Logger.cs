using System;
using System.IO;

namespace VoicePaste.Logging;

/// <summary>
/// Simple logger that writes to console and optional file.
/// </summary>
public static class Logger
{
    private static readonly string? _logFilePath;
    private static volatile bool _isDebugMode;
    private static readonly object _lock = new();

    static Logger()
    {
        Configure(false);

        // Setup log file in temp directory
        var tempDir = Path.Combine(Path.GetTempPath(), "VoicePaste");
        try
        {
            Directory.CreateDirectory(tempDir);
            _logFilePath = Path.Combine(tempDir, "voicepaste.log");
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[Logger] WARNING: Could not initialize log file: {ex.Message}");
            _logFilePath = null;
        }
    }

    public static bool IsDebugMode => _isDebugMode;

    public static void Configure(bool debugLogging)
    {
        var debugEnv = Environment.GetEnvironmentVariable("VOICEPASTE_DEBUG");
        _isDebugMode = debugLogging || debugEnv == "1" ||
            string.Equals(debugEnv, "true", StringComparison.OrdinalIgnoreCase);
    }

    public static void Info(string component, string message)
    {
        Log("INFO", component, message);
    }

    public static void Debug(string component, string message)
    {
        if (!_isDebugMode) return;
        Log("DEBUG", component, message);
    }

    public static void Warning(string component, string message)
    {
        Log("WARN", component, message);
    }

    public static void Error(string component, string message, Exception? ex = null)
    {
        var fullMessage = ex != null ? $"{message} - {ex.Message}\n{ex.StackTrace}" : message;
        Log("ERROR", component, fullMessage);
    }

    private static void Log(string level, string component, string message)
    {
        var timestamp = DateTime.Now.ToString("HH:mm:ss.fff");
        var logLine = $"[{timestamp}] [{level}] [{component}] {message}";
        
        // Always write to console
        Console.WriteLine(logLine);
        
        // Write to file if available
        WriteToFile(logLine);
    }

    private static void WriteToFile(string message)
    {
        if (_logFilePath == null) return;

        try
        {
            lock (_lock)
            {
                File.AppendAllText(_logFilePath, message + Environment.NewLine);
            }
        }
        catch
        {
            // Ignore file writing errors to avoid cascading failures
        }
    }

    public static string? GetLogFilePath() => _logFilePath;
}
