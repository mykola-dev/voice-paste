using System.ComponentModel;
using System.Runtime.ExceptionServices;
using System.Runtime.InteropServices;
using System.Windows.Threading;
using VoicePaste.Hotkey;

namespace VoicePaste.Tests;

public class RecordingCancelHotkeyTests
{
    private const int Escape = 0x1B;
    private const int KeyDown = 0x0100;
    private const int KeyUp = 0x0101;
    private const int SysKeyDown = 0x0104;
    private const int SysKeyUp = 0x0105;

    [Theory]
    [InlineData(KeyDown, KeyUp)]
    [InlineData(SysKeyDown, SysKeyUp)]
    public void Escape_CancelsAsynchronouslyAndConsumesRepeatsAndRelease(int down, int up) => RunSta(() =>
    {
        var native = new FakeKeyboardHook();
        using var hotkey = new RecordingCancelHotkey(Dispatcher.CurrentDispatcher, native);
        var cancellations = 0;
        hotkey.CancelRequested += (_, _) => { cancellations++; hotkey.SetRecording(false); };
        Assert.Equal(new IntPtr(42), native.Send(Escape, down)); // Idle: no hook at all.
        Assert.Equal(0, native.InstallCount);
        hotkey.SetRecording(true);
        hotkey.SetRecording(true);
        Assert.Equal(1, native.InstallCount);
        Assert.Equal(new IntPtr(42), native.Send(0x41, KeyDown));
        Assert.Equal(new IntPtr(42), native.Send(Escape, up)); // Unmatched release passes through.
        Assert.Equal(new IntPtr(1), native.Send(Escape, down));
        Assert.Equal(0, cancellations); // Domain work must never run inside the keyboard callback.
        Assert.Equal(new IntPtr(1), native.Send(Escape, down));
        PumpDispatcher();
        Assert.Equal(1, cancellations);
        Assert.Equal(0, native.RemoveCount); // Still consuming this physical press after cancellation.
        Assert.Equal(new IntPtr(1), native.Send(Escape, down));
        Assert.Equal(new IntPtr(42), native.Send(0x41, KeyDown)); // Other keys are never swallowed.
        PumpDispatcher();
        Assert.Equal(1, cancellations);
        Assert.Equal(new IntPtr(1), native.Send(Escape, up));
        Assert.Equal(1, native.RemoveCount);
        Assert.Equal(new IntPtr(42), native.Send(Escape, down)); // Next press belongs to the active app.
    });

    [Fact]
    public void LeavingRecordingNormally_RemovesHookImmediately() => RunSta(() =>
    {
        var native = new FakeKeyboardHook();
        using var hotkey = new RecordingCancelHotkey(Dispatcher.CurrentDispatcher, native);
        hotkey.SetRecording(true);
        hotkey.SetRecording(false); // Idle or transcription.
        Assert.Equal(1, native.RemoveCount);
        Assert.Equal(new IntPtr(42), native.Send(Escape, KeyDown));
    });

    [Fact]
    public void QueuedCancellation_DoesNotCancelTranscriptionOrANewerSession() => RunSta(() =>
    {
        var native = new FakeKeyboardHook();
        using var hotkey = new RecordingCancelHotkey(Dispatcher.CurrentDispatcher, native);
        var cancellations = 0;
        hotkey.CancelRequested += (_, _) => cancellations++;
        hotkey.SetRecording(true);
        native.Send(Escape, KeyDown);
        hotkey.SetRecording(false);
        native.Send(Escape, KeyUp);
        PumpDispatcher();
        Assert.Equal(0, cancellations);

        hotkey.SetRecording(true);
        native.Send(Escape, KeyDown);
        hotkey.SetRecording(false);
        hotkey.SetRecording(true); // New recording starts while the old Esc is still held.
        Assert.Equal(new IntPtr(1), native.Send(Escape, KeyDown));
        PumpDispatcher();
        Assert.Equal(0, cancellations);
        native.Send(Escape, KeyUp);
        native.Send(Escape, KeyDown);
        PumpDispatcher();
        Assert.Equal(1, cancellations);
    });

    [Fact]
    public void EscapeAlreadyHeldBeforeRecording_PassesThroughUntilReleased() => RunSta(() =>
    {
        var native = new FakeKeyboardHook { EscapePressed = true };
        using var hotkey = new RecordingCancelHotkey(Dispatcher.CurrentDispatcher, native);
        var cancellations = 0;
        hotkey.CancelRequested += (_, _) => cancellations++;
        hotkey.SetRecording(true);
        Assert.Equal(new IntPtr(42), native.Send(Escape, KeyDown));
        Assert.Equal(new IntPtr(42), native.Send(Escape, KeyUp));
        PumpDispatcher();
        Assert.Equal(0, cancellations);
        Assert.Equal(new IntPtr(1), native.Send(Escape, KeyDown));
        PumpDispatcher();
        Assert.Equal(1, cancellations);
    });

    [Fact]
    public void Dispose_RemovesHookAndInvalidatesQueuedWork() => RunSta(() =>
    {
        var native = new FakeKeyboardHook();
        var hotkey = new RecordingCancelHotkey(Dispatcher.CurrentDispatcher, native);
        var cancellations = 0;
        hotkey.CancelRequested += (_, _) => cancellations++;
        hotkey.SetRecording(true);
        native.Send(Escape, KeyDown);
        hotkey.Dispose();
        hotkey.Dispose();
        PumpDispatcher();
        Assert.Equal(0, cancellations);
        Assert.Equal(1, native.RemoveCount);
        Assert.Equal(new IntPtr(42), native.Send(Escape, KeyUp));
        Assert.Throws<ObjectDisposedException>(() => hotkey.SetRecording(true));
    });

    [Fact]
    public void FailedInstallation_LeavesCancellationInactiveAndCanBeRetried() => RunSta(() =>
    {
        var native = new FakeKeyboardHook { FailInstall = true };
        using var hotkey = new RecordingCancelHotkey(Dispatcher.CurrentDispatcher, native);
        Assert.Throws<Win32Exception>(() => hotkey.SetRecording(true));
        hotkey.SetRecording(false);
        Assert.Equal(0, native.RemoveCount);
        native.FailInstall = false;
        hotkey.SetRecording(true);
        Assert.Equal(2, native.InstallCount);
        hotkey.SetRecording(false);
        Assert.Equal(1, native.RemoveCount);
    });

    [Fact]
    public void NegativeHookCode_IsForwardedWithoutReadingKeyboardData() => RunSta(() =>
    {
        var native = new FakeKeyboardHook();
        using var hotkey = new RecordingCancelHotkey(Dispatcher.CurrentDispatcher, native);
        hotkey.SetRecording(true);
        Assert.Equal(new IntPtr(42), native.Send(Escape, KeyDown, code: -1));
        PumpDispatcher();
    });

    [Fact]
    public void DedicatedInputThread_DoesNotWaitForUIToConsumeEscape() => RunSta(() =>
    {
        var dispatcher = Dispatcher.CurrentDispatcher;
        var uiThreadId = Environment.CurrentManagedThreadId;
        var native = new FakeKeyboardHook();
        using var hotkey = new RecordingCancelHotkey(dispatcher, native, useDedicatedThread: true);
        var cancellations = 0;
        hotkey.CancelRequested += (_, _) =>
        {
            Assert.Equal(uiThreadId, Environment.CurrentManagedThreadId);
            cancellations++;
            hotkey.SetRecording(false);
        };
        hotkey.SetRecording(true);
        Assert.NotEqual(uiThreadId, native.InstallThreadId);
        IntPtr result = IntPtr.Zero;
        Exception? inputError = null;
        var press = new Thread(() =>
        {
            try { result = native.Send(Escape, KeyDown); }
            catch (Exception ex) { inputError = ex; }
        }) { IsBackground = true };
        press.Start();
        // Deliberately block the UI. The hook thread must still consume the key immediately.
        Assert.True(press.Join(TimeSpan.FromSeconds(3)));
        if (inputError != null) ExceptionDispatchInfo.Capture(inputError).Throw();
        Assert.Equal(new IntPtr(1), result);
        Assert.Equal(0, cancellations);
        PumpDispatcher();
        Assert.Equal(1, cancellations);
        Assert.Equal(new IntPtr(1), native.Send(Escape, KeyUp));
        Assert.Equal(1, native.RemoveCount);
    });

    private static void PumpDispatcher()
    {
        var frame = new DispatcherFrame();
        Dispatcher.CurrentDispatcher.BeginInvoke(DispatcherPriority.Background, new Action(() => frame.Continue = false));
        Dispatcher.PushFrame(frame);
    }

    private static void RunSta(Action test)
    {
        Exception? error = null;
        var thread = new Thread(() =>
        {
            try { test(); }
            catch (Exception ex) { error = ex; }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        Assert.True(thread.Join(TimeSpan.FromSeconds(10)), "Escape cancellation tests timed out");
        if (error != null) ExceptionDispatchInfo.Capture(error).Throw();
    }

    private sealed class FakeKeyboardHook : IKeyboardHookNative
    {
        private RecordingCancelHotkey.HookCallback? _callback;
        private Dispatcher? _dispatcher;
        public int InstallCount { get; private set; }
        public int RemoveCount { get; private set; }
        public int InstallThreadId { get; private set; }
        public bool FailInstall { get; set; }
        public bool EscapePressed { get; set; }

        public IntPtr Install(RecordingCancelHotkey.HookCallback callback)
        {
            InstallCount++;
            if (FailInstall) throw new Win32Exception(5);
            _dispatcher = Dispatcher.CurrentDispatcher;
            InstallThreadId = Environment.CurrentManagedThreadId;
            _callback = callback;
            return new IntPtr(123);
        }

        public void Remove(IntPtr hook) { RemoveCount++; _callback = null; }
        public IntPtr Forward(int code, IntPtr message, IntPtr data) => new(42);
        public bool IsEscapePressed() => EscapePressed;

        public IntPtr Send(int key, int message, int code = 0)
        {
            if (_dispatcher == null) return new IntPtr(42);
            return _dispatcher.Invoke(() =>
            {
                var callback = _callback;
                if (callback == null) return new IntPtr(42);
                if (code < 0) return callback(code, new IntPtr(message), IntPtr.Zero);
                var data = Marshal.AllocHGlobal(sizeof(int));
                try
                {
                    Marshal.WriteInt32(data, key);
                    return callback(code, new IntPtr(message), data);
                }
                finally { Marshal.FreeHGlobal(data); }
            });
        }
    }
}
