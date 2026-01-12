using System;
using System.Collections.Generic;

namespace VoicePaste.Settings;

public readonly record struct ParsedHotkey(uint Key, uint Modifiers);

public static class HotkeyParser
{
    private static readonly Dictionary<string, uint> KeyMap = new(StringComparer.OrdinalIgnoreCase)
    {
        ["ScrollLock"] = Hotkey.VirtualKeys.VK_SCROLL,
        ["Pause"] = Hotkey.VirtualKeys.VK_PAUSE,
        ["F13"] = Hotkey.VirtualKeys.VK_F13,
        ["F14"] = Hotkey.VirtualKeys.VK_F14,
        ["F15"] = Hotkey.VirtualKeys.VK_F15,
        ["F8"] = 0x77,
        ["F9"] = 0x78,
        ["F10"] = 0x79,
        ["F11"] = 0x7A,
        ["F12"] = 0x7B,
        ["Space"] = 0x20
    };

    public static bool TryParse(string? hotkey, out ParsedHotkey parsed)
    {
        parsed = default;

        if (string.IsNullOrWhiteSpace(hotkey))
        {
            return false;
        }

        uint modifiers = 0;
        string? keyPart = null;

        var parts = hotkey.Split('+', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
        foreach (var part in parts)
        {
            if (part.Equals("Ctrl", StringComparison.OrdinalIgnoreCase) || part.Equals("Control", StringComparison.OrdinalIgnoreCase))
            {
                modifiers |= (uint)Hotkey.ModifierKeys.Control;
                continue;
            }

            if (part.Equals("Alt", StringComparison.OrdinalIgnoreCase))
            {
                modifiers |= (uint)Hotkey.ModifierKeys.Alt;
                continue;
            }

            if (part.Equals("Shift", StringComparison.OrdinalIgnoreCase))
            {
                modifiers |= (uint)Hotkey.ModifierKeys.Shift;
                continue;
            }

            if (part.Equals("Win", StringComparison.OrdinalIgnoreCase) || part.Equals("Windows", StringComparison.OrdinalIgnoreCase))
            {
                modifiers |= (uint)Hotkey.ModifierKeys.Win;
                continue;
            }

            keyPart = part;
        }

        if (keyPart is null)
        {
            // Single-part key.
            keyPart = parts.Length == 1 ? parts[0] : null;
        }

        if (keyPart is null)
        {
            return false;
        }

        if (!KeyMap.TryGetValue(keyPart, out var key))
        {
            return false;
        }

        parsed = new ParsedHotkey(key, modifiers);
        return true;
    }
}
