using System;
using System.Runtime.InteropServices;
using System.Windows;
using System.Windows.Interop;

namespace VoicePaste.Hotkey;

/// <summary>
/// Manages global hotkey registration (e.g., ScrollLock).
/// Uses Windows RegisterHotKey API.
/// </summary>
public class GlobalHotkeyManager : IDisposable
{
    private const int WM_HOTKEY = 0x0312;
    
    private readonly Window _window;
    private HwndSource? _source;
    private int _hotkeyId;
    private bool _isRegistered;
    
    /// <summary>
    /// Fires when the registered hotkey is pressed.
    /// </summary>
    public event EventHandler? HotkeyPressed;
    
    [DllImport("user32.dll")]
    private static extern bool RegisterHotKey(IntPtr hWnd, int id, uint fsModifiers, uint vk);
    
    [DllImport("user32.dll")]
    private static extern bool UnregisterHotKey(IntPtr hWnd, int id);
    
    public GlobalHotkeyManager(Window window)
    {
        _window = window ?? throw new ArgumentNullException(nameof(window));
        _hotkeyId = GetHashCode();
    }
    
    /// <summary>
    /// Register a global hotkey.
    /// </summary>
    /// <param name="key">Virtual key code (e.g., VK_SCROLL for ScrollLock = 0x91).</param>
    /// <param name="modifiers">Modifier keys (0 = none).</param>
    /// <exception cref="InvalidOperationException">Hotkey already registered or registration failed.</exception>
    public void RegisterHotkey(uint key, uint modifiers = 0)
    {
        if (_isRegistered)
            throw new InvalidOperationException("Hotkey already registered");
        
        // Get window handle
        var helper = new WindowInteropHelper(_window);
        var handle = helper.Handle;
        
        if (handle == IntPtr.Zero)
        {
            // Window not loaded yet, wait for SourceInitialized
            _window.SourceInitialized += (s, e) => RegisterHotkeyInternal(key, modifiers);
        }
        else
        {
            RegisterHotkeyInternal(key, modifiers);
        }
    }
    
    private void RegisterHotkeyInternal(uint key, uint modifiers)
    {
        var helper = new WindowInteropHelper(_window);
        var handle = helper.Handle;
        
        Console.WriteLine($"[Hotkey] Registering hotkey - Key: 0x{key:X}, Modifiers: {modifiers}, Handle: {handle}");
        
        // Add hook for WM_HOTKEY messages
        _source = HwndSource.FromHwnd(handle);
        _source?.AddHook(WndProc);
        
        // Register hotkey
        if (!RegisterHotKey(handle, _hotkeyId, modifiers, key))
        {
            Console.WriteLine($"[Hotkey] ERROR: Failed to register hotkey!");
            throw new InvalidOperationException(
                $"Failed to register hotkey. Key may be in use by another application."
            );
        }
        
        Console.WriteLine($"[Hotkey] Hotkey registered successfully with ID: {_hotkeyId}");
        _isRegistered = true;
    }
    
    /// <summary>
    /// Unregister the hotkey.
    /// </summary>
    public void UnregisterHotkey()
    {
        if (!_isRegistered) return;
        
        var helper = new WindowInteropHelper(_window);
        UnregisterHotKey(helper.Handle, _hotkeyId);
        
        _source?.RemoveHook(WndProc);
        _isRegistered = false;
    }
    
    private IntPtr WndProc(IntPtr hwnd, int msg, IntPtr wParam, IntPtr lParam, ref bool handled)
    {
        if (msg == WM_HOTKEY && wParam.ToInt32() == _hotkeyId)
        {
            Console.WriteLine($"[Hotkey] WM_HOTKEY received! ID: {_hotkeyId}");
            HotkeyPressed?.Invoke(this, EventArgs.Empty);
            handled = true;
        }
        
        return IntPtr.Zero;
    }
    
    public void Dispose()
    {
        UnregisterHotkey();
    }
}

/// <summary>
/// Common virtual key codes for hotkeys.
/// </summary>
public static class VirtualKeys
{
    public const uint VK_SCROLL = 0x91;      // ScrollLock
    public const uint VK_F13 = 0x7C;         // F13
    public const uint VK_F14 = 0x7D;         // F14
    public const uint VK_F15 = 0x7E;         // F15
    public const uint VK_PAUSE = 0x13;       // Pause/Break
}

/// <summary>
/// Modifier key flags for hotkeys.
/// </summary>
[Flags]
public enum ModifierKeys : uint
{
    None = 0,
    Alt = 1,
    Control = 2,
    Shift = 4,
    Win = 8
}
