using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Windows.Threading;

namespace VoicePaste.Overlay;

/// <summary>Passive recording-only dispatcher observation; no polling, posted work, or timer-resolution changes.</summary>
internal sealed class DispatcherFrameObserver(Dispatcher dispatcher) : IDisposable
{
    private readonly Operation[] _operations = new Operation[64];
    private readonly DispatcherSlowOperation[] _slow = new DispatcherSlowOperation[8];
    private bool _active;
    private int _depth, _count, _slowCount, _untracked;
    private double _busyMs, _maxOperationMs, _maxUnobservedGapMs;
    private DispatcherPriority _maxPriority;
    private long _lastCompleted;
    private long? _previousCpuTicks;

    public void Start()
    {
        dispatcher.VerifyAccess();
        Dispose();
        _depth = 0;
        ResetWindow();
        _lastCompleted = Stopwatch.GetTimestamp();
        _previousCpuTicks = ReadCpuTicks();
        dispatcher.Hooks.OperationStarted += OperationStarted;
        dispatcher.Hooks.OperationCompleted += OperationCompleted;
        _active = true;
    }

    public DispatcherFrameSnapshot Snapshot()
    {
        dispatcher.VerifyAccess();
        var cpu = ReadCpuTicks();
        var cpuMs = cpu.HasValue && _previousCpuTicks.HasValue
            ? Math.Max(0, cpu.Value - _previousCpuTicks.Value) / 10000d : (double?)null;
        _previousCpuTicks = cpu;
        var report = new DispatcherFrameSnapshot(GetCurrentThreadId(), cpuMs, _count, _busyMs,
            _maxOperationMs, _maxPriority.ToString(), _maxUnobservedGapMs, _depth,
            _depth > 0 ? Stopwatch.GetElapsedTime(_operations[0].Started).TotalMilliseconds : 0,
            _depth > 0 ? _operations[0].Priority.ToString() : "None", _untracked,
            _slow.AsSpan(0, _slowCount).ToArray());
        ResetWindow();
        return report;
    }

    public void Dispose()
    {
        dispatcher.VerifyAccess();
        if (!_active) return;
        dispatcher.Hooks.OperationStarted -= OperationStarted;
        dispatcher.Hooks.OperationCompleted -= OperationCompleted;
        _active = false;
        // Do not retain callback closures/operations after recording stops, including a parent
        // operation in whose nested message loop Start/Stop may have been called.
        Array.Clear(_operations);
        _depth = 0;
    }

    private void OperationStarted(object? sender, DispatcherHookEventArgs e)
    {
        var now = Stopwatch.GetTimestamp();
        if (_depth == 0)
            _maxUnobservedGapMs = Math.Max(_maxUnobservedGapMs, Stopwatch.GetElapsedTime(_lastCompleted, now).TotalMilliseconds);
        if (_depth == _operations.Length) { _untracked++; return; }
        _operations[_depth++] = new Operation(e.Operation, now, e.Operation.Priority);
    }

    private void OperationCompleted(object? sender, DispatcherHookEventArgs e)
    {
        var now = Stopwatch.GetTimestamp();
        var index = _depth - 1;
        while (index >= 0 && !ReferenceEquals(_operations[index].Value, e.Operation)) index--;
        if (index < 0) return; // Operation began before observation, or exceeded the bounded stack.
        var operation = _operations[index];
        var elapsed = Stopwatch.GetElapsedTime(operation.Started, now).TotalMilliseconds;
        _count++;
        if (index == 0) _busyMs += elapsed; // Nested dispatcher frames must not double-count busy time.
        if (elapsed > _maxOperationMs) { _maxOperationMs = elapsed; _maxPriority = operation.Priority; }
        if (elapsed >= 2 && _slowCount < _slow.Length)
            _slow[_slowCount++] = new DispatcherSlowOperation(elapsed, operation.Priority.ToString());
        Array.Clear(_operations, index, _depth - index);
        _depth = index;
        if (_depth == 0) _lastCompleted = now;
    }

    private void ResetWindow()
    {
        _count = _slowCount = _untracked = 0;
        _busyMs = _maxOperationMs = _maxUnobservedGapMs = 0;
        _maxPriority = DispatcherPriority.Invalid;
    }

    private static long? ReadCpuTicks()
        => GetThreadTimes(new IntPtr(-2), out _, out _, out var kernel, out var user) ? kernel + user : null;

    private readonly record struct Operation(DispatcherOperation Value, long Started, DispatcherPriority Priority);

    [DllImport("kernel32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetThreadTimes(IntPtr thread, out long creation, out long exit, out long kernel, out long user);

    [DllImport("kernel32.dll")]
    private static extern uint GetCurrentThreadId();
}

internal sealed record DispatcherFrameSnapshot(uint NativeThreadId, double? ThreadCpuMs, int CompletedOperations,
    double OutermostOperationMs, double MaxOperationMs, string MaxOperationPriority,
    double MaxGapOutsideObservedOperationsMs, int OpenOperations, double OpenOperationElapsedMs,
    string OpenOperationPriority, int UntrackedOperations,
    DispatcherSlowOperation[] SlowOperations);

internal readonly record struct DispatcherSlowOperation(double Milliseconds, string Priority);
