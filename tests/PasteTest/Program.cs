using System;
using System.Runtime.InteropServices;
using System.Threading;

/// <summary>
/// Simple test to verify SendInput works for pasting.
/// Run this, then click on Notepad within 3 seconds.
/// </summary>
class TestPaste
{
    [DllImport("user32.dll", SetLastError = true)]
    static extern uint SendInput(uint nInputs, INPUT[] pInputs, int cbSize);

    [DllImport("user32.dll")]
    static extern IntPtr GetMessageExtraInfo();

    [DllImport("user32.dll")]
    static extern bool SetClipboardData(uint uFormat, IntPtr hMem);

    [DllImport("user32.dll")]
    static extern bool OpenClipboard(IntPtr hWndNewOwner);

    [DllImport("user32.dll")]
    static extern bool CloseClipboard();

    [DllImport("user32.dll")]
    static extern bool EmptyClipboard();

    [DllImport("kernel32.dll")]
    static extern IntPtr GlobalAlloc(uint uFlags, UIntPtr dwBytes);

    [DllImport("kernel32.dll")]
    static extern IntPtr GlobalLock(IntPtr hMem);

    [DllImport("kernel32.dll")]
    static extern bool GlobalUnlock(IntPtr hMem);

    const uint CF_UNICODETEXT = 13;
    const uint GMEM_MOVEABLE = 0x0002;

    const int INPUT_KEYBOARD = 1;
    const uint KEYEVENTF_KEYUP = 0x0002;
    const ushort VK_CONTROL = 0x11;
    const ushort VK_V = 0x56;

    [StructLayout(LayoutKind.Sequential)]
    struct INPUT
    {
        public int type;
        public InputUnion U;
    }

    [StructLayout(LayoutKind.Explicit)]
    struct InputUnion
    {
        [FieldOffset(0)]
        public KEYBDINPUT ki;
    }

    [StructLayout(LayoutKind.Sequential)]
    struct KEYBDINPUT
    {
        public ushort wVk;
        public ushort wScan;
        public uint dwFlags;
        public uint time;
        public IntPtr dwExtraInfo;
    }

    static void Main()
    {
        string testText = "Hello from VoicePaste test!";
        
        Console.WriteLine("=== SendInput Paste Test ===");
        Console.WriteLine($"Will paste: '{testText}'");
        Console.WriteLine("Click on Notepad within 3 seconds...");
        
        Thread.Sleep(3000);
        
        // Set clipboard using Win32 API directly
        Console.WriteLine("Setting clipboard...");
        if (OpenClipboard(IntPtr.Zero))
        {
            EmptyClipboard();
            
            // Allocate global memory for the string
            var bytes = (testText.Length + 1) * 2; // Unicode = 2 bytes per char
            var hGlobal = GlobalAlloc(GMEM_MOVEABLE, (UIntPtr)bytes);
            var lpGlobal = GlobalLock(hGlobal);
            
            // Copy string to global memory
            Marshal.Copy(testText.ToCharArray(), 0, lpGlobal, testText.Length);
            
            GlobalUnlock(hGlobal);
            SetClipboardData(CF_UNICODETEXT, hGlobal);
            CloseClipboard();
            
            Console.WriteLine("Clipboard set successfully");
        }
        else
        {
            Console.WriteLine("ERROR: Could not open clipboard");
            return;
        }
        
        Thread.Sleep(100);
        
        // Send Ctrl+V
        Console.WriteLine("Sending Ctrl+V...");
        
        var inputs = new INPUT[]
        {
            // Ctrl down
            new INPUT
            {
                type = INPUT_KEYBOARD,
                U = new InputUnion
                {
                    ki = new KEYBDINPUT
                    {
                        wVk = VK_CONTROL,
                        wScan = 0,
                        dwFlags = 0,
                        time = 0,
                        dwExtraInfo = GetMessageExtraInfo()
                    }
                }
            },
            // V down
            new INPUT
            {
                type = INPUT_KEYBOARD,
                U = new InputUnion
                {
                    ki = new KEYBDINPUT
                    {
                        wVk = VK_V,
                        wScan = 0,
                        dwFlags = 0,
                        time = 0,
                        dwExtraInfo = GetMessageExtraInfo()
                    }
                }
            },
            // V up
            new INPUT
            {
                type = INPUT_KEYBOARD,
                U = new InputUnion
                {
                    ki = new KEYBDINPUT
                    {
                        wVk = VK_V,
                        wScan = 0,
                        dwFlags = KEYEVENTF_KEYUP,
                        time = 0,
                        dwExtraInfo = GetMessageExtraInfo()
                    }
                }
            },
            // Ctrl up
            new INPUT
            {
                type = INPUT_KEYBOARD,
                U = new InputUnion
                {
                    ki = new KEYBDINPUT
                    {
                        wVk = VK_CONTROL,
                        wScan = 0,
                        dwFlags = KEYEVENTF_KEYUP,
                        time = 0,
                        dwExtraInfo = GetMessageExtraInfo()
                    }
                }
            }
        };

        uint result = SendInput((uint)inputs.Length, inputs, Marshal.SizeOf<INPUT>());
        int error = Marshal.GetLastWin32Error();
        
        Console.WriteLine($"SendInput returned: {result} (expected: 4)");
        Console.WriteLine($"Last Win32 error: {error}");
        
        if (result == 4)
        {
            Console.WriteLine("SUCCESS! Text should have been pasted.");
        }
        else
        {
            Console.WriteLine("FAILED! SendInput did not send all inputs.");
        }
    }
}
