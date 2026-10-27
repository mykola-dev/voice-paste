using System;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading.Tasks;
using VoicePaste.Logging;

namespace VoicePaste.Paste;

/// <summary>
/// Pastes text into focused window using clipboard + SendInput.
/// </summary>
public class ClipboardPaster
{
    private readonly Settings.PasteShortcut _pasteShortcut;

    public ClipboardPaster(
        Settings.PasteShortcut pasteShortcut = Settings.PasteShortcut.CtrlShiftV)
    {
        _pasteShortcut = pasteShortcut;
    }
    
    /// <summary>
    /// Paste text into currently focused window.
    /// </summary>
    public Task<bool> PasteTextAsync(string text)
    {
        if (string.IsNullOrWhiteSpace(text))
        {
            return Task.FromResult(true);
        }

        return Task.Run(() => PasteText(text));
    }

    private bool PasteText(string text)
    {
        
        // Set new clipboard content
        if (!TrySetClipboardTextWithRetry(text))
        {
            return false;
        }

        
        // Keep the paste delay off the UI thread so the overlay animation stays smooth.
        System.Threading.Thread.Sleep(100);
        
        // Send configured paste shortcut
        Logger.Debug("Paste", $"Sending paste shortcut: {_pasteShortcut}");

        if (!SendPasteShortcut(_pasteShortcut))
        {
            return false;
        }

        return true;
    }
    
    /// <summary>
    /// Try to set clipboard text with short retry logic.
    /// </summary>
    private static bool TrySetClipboardTextWithRetry(string text)
    {
        const int maxAttempts = 6;
        for (int attempt = 1; attempt <= maxAttempts; attempt++)
        {
            if (TrySetClipboardText(text, out var error))
            {
                if (attempt > 1)
                {
                    Logger.Warning("Paste", $"Clipboard set succeeded after retry {attempt}/{maxAttempts}");
                }

                return true;
            }

            if (attempt < maxAttempts)
            {
                int delayMs = 10 * (1 << (attempt - 1)); // 10, 20, 40, 80, 160ms
                Logger.Warning("Paste", $"Clipboard set failed on attempt {attempt}/{maxAttempts}. Retrying in {delayMs}ms. LastError: {error}");
                System.Threading.Thread.Sleep(delayMs);
                continue;
            }

            Logger.Warning("Paste", $"Could not set clipboard after {maxAttempts} attempts. LastError: {error}");
            return false;
        }

        return false;
    }
    
    private static bool TrySetClipboardText(string text, out int error)
    {
        error = 0;
        if (!OpenClipboard(IntPtr.Zero))
        {
            error = Marshal.GetLastWin32Error();
            return false;
        }

        IntPtr hGlobal = IntPtr.Zero;
        try
        {
            if (!EmptyClipboard())
            {
                error = Marshal.GetLastWin32Error();
                return false;
            }

            byte[] bytes = Encoding.Unicode.GetBytes(text + "\0");
            hGlobal = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, (UIntPtr)bytes.Length);
            if (hGlobal == IntPtr.Zero)
            {
                error = Marshal.GetLastWin32Error();
                return false;
            }

            IntPtr target = GlobalLock(hGlobal);
            if (target == IntPtr.Zero)
            {
                error = Marshal.GetLastWin32Error();
                return false;
            }

            try
            {
                Marshal.Copy(bytes, 0, target, bytes.Length);
            }
            finally
            {
                GlobalUnlock(hGlobal);
            }

            if (SetClipboardData(CF_UNICODETEXT, hGlobal) == IntPtr.Zero)
            {
                error = Marshal.GetLastWin32Error();
                return false;
            }

            // Clipboard owns the memory after SetClipboardData succeeds.
            hGlobal = IntPtr.Zero;
            return true;
        }
        finally
        {
            if (hGlobal != IntPtr.Zero)
            {
                GlobalFree(hGlobal);
            }

            CloseClipboard();
        }
    }

    private static bool SendPasteShortcut(Settings.PasteShortcut shortcut)
    {
        switch (shortcut)
        {
            case Settings.PasteShortcut.CtrlV:
                return SendCtrlV();
            case Settings.PasteShortcut.ShiftInsert:
                return SendShiftInsert();
            case Settings.PasteShortcut.CtrlShiftV:
            default:
                return SendCtrlShiftV();
        }
    }

    /// <summary>
    /// Send Ctrl+Shift+V using SendInput with correct 64-bit struct layout.
    /// </summary>
    private static bool SendCtrlShiftV()
    {
        var extraInfo = GetMessageExtraInfo();
        
        INPUT[] inputs = new INPUT[]
        {
            // Ctrl down
            new INPUT
            {
                type = INPUT_KEYBOARD,
                ki = new KEYBDINPUT
                {
                    wVk = VK_CONTROL,
                    wScan = 0,
                    dwFlags = 0,
                    time = 0,
                    dwExtraInfo = extraInfo
                }
            },
            // Shift down
            new INPUT
            {
                type = INPUT_KEYBOARD,
                ki = new KEYBDINPUT
                {
                    wVk = VK_SHIFT,
                    wScan = 0,
                    dwFlags = 0,
                    time = 0,
                    dwExtraInfo = extraInfo
                }
            },
            // V down
            new INPUT
            {
                type = INPUT_KEYBOARD,
                ki = new KEYBDINPUT
                {
                    wVk = VK_V,
                    wScan = 0,
                    dwFlags = 0,
                    time = 0,
                    dwExtraInfo = extraInfo
                }
            },
            // V up
            new INPUT
            {
                type = INPUT_KEYBOARD,
                ki = new KEYBDINPUT
                {
                    wVk = VK_V,
                    wScan = 0,
                    dwFlags = KEYEVENTF_KEYUP,
                    time = 0,
                    dwExtraInfo = extraInfo
                }
            },
            // Shift up
            new INPUT
            {
                type = INPUT_KEYBOARD,
                ki = new KEYBDINPUT
                {
                    wVk = VK_SHIFT,
                    wScan = 0,
                    dwFlags = KEYEVENTF_KEYUP,
                    time = 0,
                    dwExtraInfo = extraInfo
                }
            },
            // Ctrl up
            new INPUT
            {
                type = INPUT_KEYBOARD,
                ki = new KEYBDINPUT
                {
                    wVk = VK_CONTROL,
                    wScan = 0,
                    dwFlags = KEYEVENTF_KEYUP,
                    time = 0,
                    dwExtraInfo = extraInfo
                }
            }
        };
        
        return SendKeyboardInputs(inputs, expectedCount: 6);
    }
    
    private static bool SendCtrlV()
    {
        var extraInfo = GetMessageExtraInfo();

        INPUT[] inputs =
        {
            KeyDown(VK_CONTROL, extraInfo),
            KeyDown(VK_V, extraInfo),
            KeyUp(VK_V, extraInfo),
            KeyUp(VK_CONTROL, extraInfo)
        };

        return SendKeyboardInputs(inputs, expectedCount: 4);
    }

    private static bool SendShiftInsert()
    {
        var extraInfo = GetMessageExtraInfo();

        INPUT[] inputs =
        {
            KeyDown(VK_SHIFT, extraInfo),
            KeyDown(VK_INSERT, extraInfo, extended: true),
            KeyUp(VK_INSERT, extraInfo, extended: true),
            KeyUp(VK_SHIFT, extraInfo)
        };

        return SendKeyboardInputs(inputs, expectedCount: 4);
    }

    private static INPUT KeyDown(ushort vk, IntPtr extraInfo, bool extended = false)
    {
        return new INPUT
        {
            type = INPUT_KEYBOARD,
            ki = new KEYBDINPUT
            {
                wVk = vk,
                wScan = 0,
                dwFlags = extended ? KEYEVENTF_EXTENDEDKEY : 0,
                time = 0,
                dwExtraInfo = extraInfo
            }
        };
    }

    private static INPUT KeyUp(ushort vk, IntPtr extraInfo, bool extended = false)
    {
        return new INPUT
        {
            type = INPUT_KEYBOARD,
            ki = new KEYBDINPUT
            {
                wVk = vk,
                wScan = 0,
                dwFlags = (extended ? KEYEVENTF_EXTENDEDKEY : 0) | KEYEVENTF_KEYUP,
                time = 0,
                dwExtraInfo = extraInfo
            }
        };
    }

    private static bool SendKeyboardInputs(INPUT[] inputs, int expectedCount)
    {
        int size = Marshal.SizeOf<INPUT>();

        uint sent = SendInput((uint)inputs.Length, inputs, size);
        int error = Marshal.GetLastWin32Error();


        if (sent != expectedCount)
        {
            var errorMessage = $"SendInput failed! Only {sent}/{expectedCount} inputs sent. Error code: {error}";
            Logger.Warning("Paste", errorMessage);
            return false;
        }

        return true;
    }

    // P/Invoke
    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool OpenClipboard(IntPtr hWndNewOwner);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool CloseClipboard();

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool EmptyClipboard();

    [DllImport("user32.dll", SetLastError = true)]
    private static extern IntPtr SetClipboardData(uint uFormat, IntPtr hMem);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr GlobalAlloc(uint uFlags, UIntPtr dwBytes);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr GlobalLock(IntPtr hMem);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GlobalUnlock(IntPtr hMem);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern IntPtr GlobalFree(IntPtr hMem);

    [DllImport("user32.dll")]
    private static extern IntPtr GetMessageExtraInfo();

    private const uint CF_UNICODETEXT = 13;
    private const uint GMEM_MOVEABLE = 0x0002;
    private const uint GMEM_ZEROINIT = 0x0040;
    private const int INPUT_KEYBOARD = 1;
    private const uint KEYEVENTF_KEYUP = 0x0002;
    private const uint KEYEVENTF_EXTENDEDKEY = 0x0001;
    private const ushort VK_CONTROL = 0x11;
    private const ushort VK_SHIFT = 0x10;
    private const ushort VK_V = 0x56;
    private const ushort VK_INSERT = 0x2D;

    // Correct INPUT structure for 64-bit Windows
    // Total size must be 40 bytes: type(4) + padding(4) + union(32)
    // See: https://docs.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-input
    [StructLayout(LayoutKind.Explicit, Size = 40)]
    private struct INPUT
    {
        [FieldOffset(0)]
        public int type;
        
        [FieldOffset(8)]  // 4 bytes padding after type for 8-byte alignment
        public KEYBDINPUT ki;
    }
    
    // KEYBDINPUT: 24 bytes on x64
    // wVk(2) + wScan(2) + dwFlags(4) + time(4) + padding(4) + dwExtraInfo(8) = 24
    [StructLayout(LayoutKind.Sequential)]
    private struct KEYBDINPUT
    {
        public ushort wVk;
        public ushort wScan;
        public uint dwFlags;
        public uint time;
        public IntPtr dwExtraInfo;
    }
}
