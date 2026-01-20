using System;
using System.Diagnostics;
using System.IO;

namespace VoicePaste.Transcription;

/// <summary>
/// Robustly finds a Python executable in the environment.
/// </summary>
public static class PythonFinder
{
    public static string Find()
    {
        // 1. Portable build: expect python/python.exe next to VoicePaste.exe
        var exeDir = AppDomain.CurrentDomain.BaseDirectory;
        var portablePython = Path.GetFullPath(Path.Combine(exeDir, "python", "python.exe"));
        if (File.Exists(portablePython))
            return portablePython;

        // 2. Search for installed Python in common locations and PATH
        var candidates = new[]
        {
            "py",               // Python Launcher
            "python",           // PATH
            "python3",          // PATH (Linux/Mac style)
            Environment.ExpandEnvironmentVariables(@"%LOCALAPPDATA%\Programs\Python\Python312\python.exe"),
            Environment.ExpandEnvironmentVariables(@"%LOCALAPPDATA%\Programs\Python\Python311\python.exe"),
            Environment.ExpandEnvironmentVariables(@"%LOCALAPPDATA%\Programs\Python\Python310\python.exe"),
            @"C:\Python312\python.exe",
            @"C:\Python311\python.exe",
            @"C:\Python310\python.exe"
        };

        foreach (var candidate in candidates)
        {
            if (CheckPython(candidate))
                return candidate;
        }

        throw new InvalidOperationException("Python not found. Please install Python 3.10 or later.");
    }

    private static bool CheckPython(string candidate)
    {
        try
        {
            var psi = new ProcessStartInfo
            {
                FileName = candidate,
                Arguments = "--version",
                UseShellExecute = false,
                RedirectStandardOutput = true,
                CreateNoWindow = true
            };

            using var process = Process.Start(psi);
            if (process != null)
            {
                process.WaitForExit(1000);
                return process.ExitCode == 0;
            }
        }
        catch
        {
            // Ignore errors for this candidate
        }
        return false;
    }
}
