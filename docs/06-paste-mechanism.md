# Paste Mechanism Specification

## Overview

After transcription, paste text into the currently focused window using:
1. Clipboard manipulation
2. Simulated keystrokes (SendInput)

## Paste Flow

```
┌─────────────────┐
│ Save clipboard  │
└────────┬────────┘
         │
┌────────┴────────┐
│ Set clipboard   │
│ to transcript   │
└────────┬────────┘
         │
┌────────┴────────┐
│ Send paste keys │
│ (Ctrl+Shift+V)  │
└────────┬────────┘
         │
┌────────┴────────┐
│ Wait 400ms      │
└────────┬────────┘
         │
┌────────┴────────┐
│ Restore         │
│ clipboard       │
└─────────────────┘
```

## Implementation

### ClipboardPaster Class

```csharp
public class ClipboardPaster
{
    private readonly int _restoreDelayMs;
    private readonly bool _shouldRestore;
    
    public async Task PasteTextAsync(string text, string pasteShortcut)
    {
        // Save current clipboard
        string? previousContent = null;
        if (_shouldRestore)
        {
            previousContent = Clipboard.GetText();
        }
        
        // Set new content
        Clipboard.SetText(text);
        
        // Small delay to ensure clipboard is set
        await Task.Delay(50);
        
        // Send paste shortcut
        SendPasteKeys(pasteShortcut);
        
        // Restore previous content
        if (_shouldRestore && previousContent != null)
        {
            await Task.Delay(_restoreDelayMs);
            Clipboard.SetText(previousContent);
        }
    }
}
```

### SendInput for Paste Keys

```csharp
private void SendPasteKeys(string shortcut)
{
    // Parse shortcut: "Ctrl+Shift+V" → modifiers + key
    var (modifiers, key) = ParseShortcut(shortcut);
    
    var inputs = new List<INPUT>();
    
    // Press modifiers
    if (modifiers.HasFlag(Modifiers.Ctrl))
        inputs.Add(KeyDown(VK_CONTROL));
    if (modifiers.HasFlag(Modifiers.Shift))
        inputs.Add(KeyDown(VK_SHIFT));
    if (modifiers.HasFlag(Modifiers.Alt))
        inputs.Add(KeyDown(VK_MENU));
    
    // Press and release main key
    inputs.Add(KeyDown(key));
    inputs.Add(KeyUp(key));
    
    // Release modifiers (reverse order)
    if (modifiers.HasFlag(Modifiers.Alt))
        inputs.Add(KeyUp(VK_MENU));
    if (modifiers.HasFlag(Modifiers.Shift))
        inputs.Add(KeyUp(VK_SHIFT));
    if (modifiers.HasFlag(Modifiers.Ctrl))
        inputs.Add(KeyUp(VK_CONTROL));
    
    SendInput(inputs.Count, inputs.ToArray(), Marshal.SizeOf<INPUT>());
}
```

## Paste Shortcuts

### Supported Shortcuts

| Shortcut | Use Case |
|----------|----------|
| Ctrl+Shift+V | Windows Terminal (default) |
| Ctrl+V | Most Windows apps |
| Shift+Insert | Universal fallback |

### Why Ctrl+Shift+V Default?

Windows Terminal uses Ctrl+Shift+V by default. Since VoicePaste is primarily for coding/terminal use, this is the best default.

## Clipboard Restoration

### Why Restore?

User may have important data in clipboard. Pasting should be transparent.

### Timing

```
Paste → Wait 400ms → Restore
```

400ms delay ensures:
- Paste operation completes
- Target app processes clipboard
- Safe to overwrite

### Configuration

- `restoreClipboard`: true (default) / false
- `clipboardRestoreDelayMs`: 400 (default)

## Focus Handling

### Target Window

Paste into whatever window is focused at paste time (not recording start).

**Rationale:** User may alt-tab during transcription. Paste should go where they are now.

### No Window Locking

- Don't capture/save target window at recording start
- Just paste to current foreground window
- Simpler and matches user expectation

## Edge Cases

### Empty Transcript

```csharp
if (string.IsNullOrWhiteSpace(text))
{
    // Don't paste empty text
    // Don't touch clipboard
    return;
}
```

### Clipboard Contains Non-Text

```csharp
// Only save/restore text content
// Images, files, etc. are lost after paste
// Document this limitation
```

### Paste Fails

- No reliable way to detect paste failure
- User will notice and can re-try
- Log for debugging

## Thread Safety

- Clipboard operations must be on STA thread
- Use Dispatcher if called from background thread

```csharp
Application.Current.Dispatcher.Invoke(() =>
{
    Clipboard.SetText(text);
});
```
