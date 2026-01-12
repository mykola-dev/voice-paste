using System;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using System.Windows;

namespace VoicePaste.Paste;

/// <summary>
/// Pastes text into focused window using clipboard + SendInput.
/// </summary>
public class ClipboardPaster
{
    private readonly int _restoreDelayMs;
    private readonly bool _shouldRestoreClipboard;
    private readonly Settings.PasteShortcut _pasteShortcut;

    public ClipboardPaster(
        Settings.PasteShortcut pasteShortcut = Settings.PasteShortcut.CtrlShiftV,
        bool restoreClipboard = true,
        int restoreDelayMs = 400)
    {
        _pasteShortcut = pasteShortcut;
        _shouldRestoreClipboard = restoreClipboard;
        _restoreDelayMs = restoreDelayMs;
    }
    
    /// <summary>
    /// Paste text into currently focused window.
    /// </summary>
    public async Task PasteTextAsync(string text)
    {
        if (string.IsNullOrWhiteSpace(text))
        {
            Console.WriteLine("[Paste] WARNING: Empty text, nothing to paste");
            return;
        }
        
        Console.WriteLine($"[Paste] Starting paste operation. Text length: {text.Length}");
        Console.WriteLine($"[Paste] Text to paste: '{text}'");
        
        // Save current clipboard (must be on STA thread)
        IDataObject? previousClipboardData = null;
        if (_shouldRestoreClipboard)
        {
            await Application.Current.Dispatcher.InvokeAsync(() =>
            {
                try
                {
                    previousClipboardData = Clipboard.GetDataObject();
                    
                    if (previousClipboardData != null)
                    {
                        var formats = previousClipboardData.GetFormats();
                        Console.WriteLine($"[Paste] Saved previous clipboard content ({formats.Length} format(s): {string.Join(", ", formats)})");
                    }
                }
                catch (Exception ex)
                {
                    Console.WriteLine($"[Paste] WARNING: Could not save clipboard: {ex.Message}");
                }
            });
        }
        
        // Set new clipboard content
        await Application.Current.Dispatcher.InvokeAsync(() =>
        {
            try
            {
                Clipboard.SetText(text);
                Console.WriteLine("[Paste] Text copied to clipboard");
            }
            catch (Exception ex)
            {
                Console.WriteLine($"[Paste] ERROR: Could not set clipboard: {ex.Message}");
                throw;
            }
        });
        
        // Small delay to ensure clipboard is set
        await Task.Delay(100);
        
        // Send configured paste shortcut
        Console.WriteLine($"[Paste] Sending paste shortcut: {_pasteShortcut}");
        SendPasteShortcut(_pasteShortcut);
        
        // Restore previous clipboard after delay
        if (_shouldRestoreClipboard && previousClipboardData != null)
        {
            await Task.Delay(_restoreDelayMs);
            
            await Application.Current.Dispatcher.InvokeAsync(() =>
            {
                try
                {
                    Clipboard.SetDataObject(previousClipboardData, false);
                    Console.WriteLine("[Paste] Clipboard restored");
                }
                catch (Exception ex)
                {
                    Console.WriteLine($"[Paste] WARNING: Could not restore clipboard: {ex.Message}");
                }
            });
        }
    }
    
    private static void SendPasteShortcut(Settings.PasteShortcut shortcut)
    {
        switch (shortcut)
        {
            case Settings.PasteShortcut.CtrlV:
                SendCtrlV();
                return;
            case Settings.PasteShortcut.ShiftInsert:
                SendShiftInsert();
                return;
            case Settings.PasteShortcut.CtrlShiftV:
            default:
                SendCtrlShiftV();
                return;
        }
    }

    /// <summary>
    /// Send Ctrl+Shift+V using SendInput with correct 64-bit struct layout.
    /// </summary>
    private static void SendCtrlShiftV()
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
        
        SendKeyboardInputs(inputs, expectedCount: 6);
    }
    
    private static void SendCtrlV()
    {
        var extraInfo = GetMessageExtraInfo();

        INPUT[] inputs =
        {
            KeyDown(VK_CONTROL, extraInfo),
            KeyDown(VK_V, extraInfo),
            KeyUp(VK_V, extraInfo),
            KeyUp(VK_CONTROL, extraInfo)
        };

        SendKeyboardInputs(inputs, expectedCount: 4);
    }

    private static void SendShiftInsert()
    {
        var extraInfo = GetMessageExtraInfo();

        INPUT[] inputs =
        {
            KeyDown(VK_SHIFT, extraInfo),
            KeyDown(VK_INSERT, extraInfo),
            KeyUp(VK_INSERT, extraInfo),
            KeyUp(VK_SHIFT, extraInfo)
        };

        SendKeyboardInputs(inputs, expectedCount: 4);
    }

    private static INPUT KeyDown(ushort vk, IntPtr extraInfo)
    {
        return new INPUT
        {
            type = INPUT_KEYBOARD,
            ki = new KEYBDINPUT
            {
                wVk = vk,
                wScan = 0,
                dwFlags = 0,
                time = 0,
                dwExtraInfo = extraInfo
            }
        };
    }

    private static INPUT KeyUp(ushort vk, IntPtr extraInfo)
    {
        return new INPUT
        {
            type = INPUT_KEYBOARD,
            ki = new KEYBDINPUT
            {
                wVk = vk,
                wScan = 0,
                dwFlags = KEYEVENTF_KEYUP,
                time = 0,
                dwExtraInfo = extraInfo
            }
        };
    }

    private static void SendKeyboardInputs(INPUT[] inputs, int expectedCount)
    {
        int size = Marshal.SizeOf<INPUT>();
        Console.WriteLine($"[Paste] INPUT struct size: {size} (expected 40 on x64)");

        uint sent = SendInput((uint)inputs.Length, inputs, size);
        int error = Marshal.GetLastWin32Error();

        Console.WriteLine($"[Paste] SendInput returned: {sent} (expected {expectedCount}), LastError: {error}");

        if (sent != expectedCount)
        {
            Console.WriteLine($"[Paste] ERROR: SendInput failed! Only {sent}/{expectedCount} inputs sent. Error code: {error}");
        }
    }

    // P/Invoke
    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);

    [DllImport("user32.dll")]
    private static extern IntPtr GetMessageExtraInfo();

    private const int INPUT_KEYBOARD = 1;
    private const uint KEYEVENTF_KEYUP = 0x0002;
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
