"""
Test paste mechanism directly
"""
import sys
import time
from pathlib import Path

# Add parent directory to path for imports
sys.path.insert(0, str(Path(__file__).parent.parent / "src" / "app"))

print("Testing paste mechanism...")
print("1. Focus a text editor (Notepad, VS Code, etc.)")
print("2. Place cursor where you want text to appear")
print("3. Wait 5 seconds...")

for i in range(5, 0, -1):
    print(f"   {i}...")
    time.sleep(1)

print("\nPasting text now!")

# Simulate what the C# app does
import win32clipboard
import win32con
import ctypes

# SendInput structures
PUL = ctypes.POINTER(ctypes.c_ulong)

class KeyBdInput(ctypes.Structure):
    _fields_ = [("wVk", ctypes.c_ushort),
                ("wScan", ctypes.c_ushort),
                ("dwFlags", ctypes.c_ulong),
                ("time", ctypes.c_ulong),
                ("dwExtraInfo", PUL)]

class HardwareInput(ctypes.Structure):
    _fields_ = [("uMsg", ctypes.c_ulong),
                ("wParamL", ctypes.c_short),
                ("wParamH", ctypes.c_ushort)]

class MouseInput(ctypes.Structure):
    _fields_ = [("dx", ctypes.c_long),
                ("dy", ctypes.c_long),
                ("mouseData", ctypes.c_ulong),
                ("dwFlags", ctypes.c_ulong),
                ("time",ctypes.c_ulong),
                ("dwExtraInfo", PUL)]

class Input_I(ctypes.Union):
    _fields_ = [("ki", KeyBdInput),
                 ("mi", MouseInput),
                 ("hi", HardwareInput)]

class Input(ctypes.Structure):
    _fields_ = [("type", ctypes.c_ulong),
                ("ii", Input_I)]

# Test text
test_text = "Hello from VoicePaste test! This is a streaming transcription test."

# Set clipboard
win32clipboard.OpenClipboard()
win32clipboard.EmptyClipboard()
win32clipboard.SetClipboardText(test_text, win32con.CF_UNICODETEXT)
win32clipboard.CloseClipboard()

print(f"Text copied to clipboard: '{test_text}'")

# Wait a bit
time.sleep(0.05)

# Send Ctrl+Shift+V
VK_CONTROL = 0x11
VK_SHIFT = 0x10
VK_V = 0x56

extra = ctypes.c_ulong(0)
ii_ = Input_I()

# Press Ctrl
ii_.ki = KeyBdInput(VK_CONTROL, 0, 0, 0, ctypes.pointer(extra))
x = Input(ctypes.c_ulong(1), ii_)
ctypes.windll.user32.SendInput(1, ctypes.pointer(x), ctypes.sizeof(x))

# Press Shift
ii_.ki = KeyBdInput(VK_SHIFT, 0, 0, 0, ctypes.pointer(extra))
x = Input(ctypes.c_ulong(1), ii_)
ctypes.windll.user32.SendInput(1, ctypes.pointer(x), ctypes.sizeof(x))

# Press V
ii_.ki = KeyBdInput(VK_V, 0, 0, 0, ctypes.pointer(extra))
x = Input(ctypes.c_ulong(1), ii_)
ctypes.windll.user32.SendInput(1, ctypes.pointer(x), ctypes.sizeof(x))

# Release V
ii_.ki = KeyBdInput(VK_V, 0, 2, 0, ctypes.pointer(extra))  # 2 = KEYEVENTF_KEYUP
x = Input(ctypes.c_ulong(1), ii_)
ctypes.windll.user32.SendInput(1, ctypes.pointer(x), ctypes.sizeof(x))

# Release Shift
ii_.ki = KeyBdInput(VK_SHIFT, 0, 2, 0, ctypes.pointer(extra))
x = Input(ctypes.c_ulong(1), ii_)
ctypes.windll.user32.SendInput(1, ctypes.pointer(x), ctypes.sizeof(x))

# Release Ctrl
ii_.ki = KeyBdInput(VK_CONTROL, 0, 2, 0, ctypes.pointer(extra))
x = Input(ctypes.c_ulong(1), ii_)
ctypes.windll.user32.SendInput(1, ctypes.pointer(x), ctypes.sizeof(x))

print("Sent Ctrl+Shift+V")
print("\nDid text appear in your editor? (Check if Ctrl+Shift+V works)")
print("If not, try manually pressing Ctrl+V - the text is in clipboard")
