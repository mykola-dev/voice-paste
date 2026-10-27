using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Windows.Threading;

namespace VoicePaste.Hotkey;

/// <summary>Consumes Escape only while recording, including repeats and the matching key release.</summary>
internal sealed class RecordingCancelHotkey : IDisposable
{
    private const int VkEscape = 0x1B;
    private const int WmKeyDown = 0x0100;
    private const int WmKeyUp = 0x0101;
    private const int WmSysKeyDown = 0x0104;
    private const int WmSysKeyUp = 0x0105;
    private readonly Dispatcher _dispatcher;
    private readonly Dispatcher _hookDispatcher;
    private readonly HookThread? _hookThread;
    private readonly IKeyboardHookNative _native;
    private readonly HookCallback _callback; // Root the delegate for the entire native hook lifetime.
    private IntPtr _hook;
    private bool _recording;
    private bool _escapeDown;
    private bool _ignoreHeldEscape;
    private bool _disposed;
    private bool _disposeRequested;
    private long _session;

    internal delegate IntPtr HookCallback(int code, IntPtr message, IntPtr data);

    public event EventHandler? CancelRequested;

    public RecordingCancelHotkey(Dispatcher dispatcher) : this(dispatcher, new NativeHook(), useDedicatedThread: true) { }

    internal RecordingCancelHotkey(Dispatcher dispatcher, IKeyboardHookNative native, bool useDedicatedThread = false)
    {
        _dispatcher = dispatcher;
        _native = native;
        _callback = HandleKeyboard;
        // Windows drops low-level hooks that time out. Keep input processing independent of
        // UI stalls, microphone shutdown and model unloading on the application dispatcher.
        if (useDedicatedThread) _hookThread = new HookThread();
        _hookDispatcher = _hookThread?.Dispatcher ?? dispatcher;
    }

    public void SetRecording(bool recording)
    {
        _dispatcher.VerifyAccess();
        ObjectDisposedException.ThrowIf(_disposeRequested, this);
        _hookDispatcher.Invoke(() => SetRecordingCore(recording), DispatcherPriority.Send);
    }

    private void SetRecordingCore(bool recording)
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
        if (_recording == recording) return;
        if (recording && _hook == IntPtr.Zero)
        {
            _hook = _native.Install(_callback);
            _ignoreHeldEscape = _native.IsEscapePressed();
        }
        _recording = recording;
        _session++;
        // Cancellation changes state before the user releases Esc. Keep consuming that one
        // physical press until key-up, otherwise repeats/releases could affect the focused app.
        if (!recording && !_escapeDown) RemoveHook();
    }

    private IntPtr HandleKeyboard(int code, IntPtr message, IntPtr data)
    {
        if (code < 0 || _disposed || Marshal.ReadInt32(data) != VkEscape)
            return _native.Forward(code, message, data);

        var keyMessage = message.ToInt32();
        if (_ignoreHeldEscape)
        {
            // A press that began in another app before recording belongs to that app.
            if (keyMessage is WmKeyUp or WmSysKeyUp) _ignoreHeldEscape = false;
            return _native.Forward(code, message, data);
        }
        if ((keyMessage is WmKeyDown or WmSysKeyDown) && (_recording || _escapeDown))
        {
            if (!_escapeDown && _recording)
            {
                var session = _session;
                // Never stop audio, unload a model or perform file I/O inside a keyboard hook.
                try
                {
                    _dispatcher.BeginInvoke(DispatcherPriority.Send, new Action(() =>
                    {
                        // An old queued request must not cancel a later recording or transcription.
                        if (!Volatile.Read(ref _disposed) && Volatile.Read(ref _recording) &&
                            session == Volatile.Read(ref _session))
                            CancelRequested?.Invoke(this, EventArgs.Empty);
                    }));
                }
                catch (InvalidOperationException) when (_dispatcher.HasShutdownStarted || _dispatcher.HasShutdownFinished)
                {
                    // Do not let a dispatcher-shutdown exception cross the unmanaged callback.
                    return _native.Forward(code, message, data);
                }
            }
            _escapeDown = true;
            return new IntPtr(1);
        }
        if ((keyMessage is WmKeyUp or WmSysKeyUp) && _escapeDown)
        {
            _escapeDown = false;
            if (!_recording) RemoveHook();
            return new IntPtr(1);
        }
        return _native.Forward(code, message, data);
    }

    private void RemoveHook()
    {
        if (_hook == IntPtr.Zero) return;
        _native.Remove(_hook);
        _hook = IntPtr.Zero;
    }

    public void Dispose()
    {
        _dispatcher.VerifyAccess();
        if (_disposeRequested) return;
        _disposeRequested = true;
        _hookDispatcher.Invoke(() =>
        {
            _disposed = true;
            _recording = false;
            _session++;
            RemoveHook();
        }, DispatcherPriority.Send);
        _hookThread?.Dispose();
    }

    private sealed class HookThread : IDisposable
    {
        private readonly Thread _thread;
        public Dispatcher Dispatcher { get; }

        public HookThread()
        {
            Dispatcher? dispatcher = null;
            using var ready = new ManualResetEventSlim();
            _thread = new Thread(() =>
            {
                dispatcher = Dispatcher.CurrentDispatcher;
                ready.Set();
                Dispatcher.Run();
            }) { IsBackground = true, Name = "VoicePaste Escape hook" };
            _thread.SetApartmentState(ApartmentState.STA);
            _thread.Start();
            ready.Wait();
            Dispatcher = dispatcher!;
        }

        public void Dispose()
        {
            Dispatcher.BeginInvokeShutdown(DispatcherPriority.Send);
            _thread.Join(1000);
        }
    }

    private sealed class NativeHook : IKeyboardHookNative
    {
        public IntPtr Install(HookCallback callback)
        {
            var hook = SetWindowsHookEx(13 /* WH_KEYBOARD_LL */, callback, GetModuleHandle(null), 0);
            if (hook == IntPtr.Zero)
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Could not enable Escape recording cancellation.");
            return hook;
        }

        public void Remove(IntPtr hook) => UnhookWindowsHookEx(hook);
        public IntPtr Forward(int code, IntPtr message, IntPtr data) => CallNextHookEx(IntPtr.Zero, code, message, data);
        public bool IsEscapePressed() => (GetAsyncKeyState(VkEscape) & 0x8000) != 0;

        [DllImport("user32.dll", EntryPoint = "SetWindowsHookExW", ExactSpelling = true, SetLastError = true)]
        private static extern IntPtr SetWindowsHookEx(int hookId, HookCallback callback, IntPtr module, uint threadId);

        [DllImport("user32.dll", ExactSpelling = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool UnhookWindowsHookEx(IntPtr hook);

        [DllImport("user32.dll", ExactSpelling = true)]
        private static extern IntPtr CallNextHookEx(IntPtr hook, int code, IntPtr message, IntPtr data);

        [DllImport("user32.dll", ExactSpelling = true)]
        private static extern short GetAsyncKeyState(int key);

        [DllImport("kernel32.dll", EntryPoint = "GetModuleHandleW", ExactSpelling = true, CharSet = CharSet.Unicode)]
        private static extern IntPtr GetModuleHandle(string? module);
    }
}

internal interface IKeyboardHookNative
{
    IntPtr Install(RecordingCancelHotkey.HookCallback callback);
    void Remove(IntPtr hook);
    IntPtr Forward(int code, IntPtr message, IntPtr data);
    bool IsEscapePressed();
}
