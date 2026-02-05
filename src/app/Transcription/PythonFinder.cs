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

        throw new InvalidOperationException("Python 3.10-3.12 not found. Please install a supported Python version.");
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
                RedirectStandardError = true,
                CreateNoWindow = true
            };

            using var process = Process.Start(psi);
            if (process != null)
            {
                var stdout = process.StandardOutput.ReadToEnd();
                var stderr = process.StandardError.ReadToEnd();
                process.WaitForExit(1000);
                var versionText = string.IsNullOrWhiteSpace(stdout) ? stderr : stdout;
                return process.ExitCode == 0 && IsSupportedPython(versionText);
            }
        }
        catch
        {
            // Ignore errors for this candidate
        }
        return false;
    }

    private static bool IsSupportedPython(string versionText)
    {
        // Expected: "Python 3.12.4"
        var parts = versionText.Trim().Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length < 2)
            return false;

        var versionParts = parts[1].Split('.');
        if (versionParts.Length < 2)
            return false;

        if (!int.TryParse(versionParts[0], out var major) ||
            !int.TryParse(versionParts[1], out var minor))
        {
            return false;
        }

        return major == 3 && minor >= 10 && minor <= 12;
    }
}
