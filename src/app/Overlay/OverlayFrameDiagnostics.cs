namespace VoicePaste.Overlay;

/// <summary>Recording-only counters. No per-frame allocations, file I/O, or dispatcher work items.</summary>
internal sealed class OverlayFrameDiagnostics(Action<OverlayFrameReport> publish,
    Func<OverlayFrameContext> getContext, double? targetFps = null)
{
    private const int MaxIntervals = 2048;
    private readonly double[] _intervals = new double[MaxIntervals];
    private readonly double[] _calibration = new double[120];
    private readonly OverlaySlowFrame[] _slowFrames = new OverlaySlowFrame[8];
    private int _intervalCount, _calibrationCount, _slowCount, _frames, _duplicates, _intervalOverflow;
    private int _uploads, _busy, _blockingLocks, _surfaceRebuilds, _onRenderCalls;
    private DurationCounter _handler, _envelope, _raster, _lock, _upload, _onRender;
    private double _currentGap, _currentPresentationGap, _currentEnvelope, _currentRaster, _currentLock, _currentUpload;
    private double _presentationGapMax, _heartbeatGapMax;
    private TimeSpan _start, _windowStart, _lastHeartbeat;
    private TimeSpan? _lastFrame;
    private double? _frameIntervalMs = targetFps is > 0 and <= 1000 ? 1000 / targetFps : null;
    private long _audioCount, _lastAudioTicks, _audioMaxGapTicks, _audioMaxWorkTicks;
    private bool _active;
    private string _session = string.Empty;

    public bool IsActive => Volatile.Read(ref _active);

    public void Start(TimeSpan now)
    {
        if (IsActive) Stop(now);
        _session = Guid.NewGuid().ToString("N");
        _start = _windowStart = _lastHeartbeat = now;
        _lastFrame = null;
        _calibrationCount = 0;
        _frameIntervalMs = targetFps is > 0 and <= 1000 ? 1000 / targetFps : null;
        Interlocked.Exchange(ref _audioCount, 0);
        Interlocked.Exchange(ref _lastAudioTicks, 0);
        Interlocked.Exchange(ref _audioMaxGapTicks, 0);
        Interlocked.Exchange(ref _audioMaxWorkTicks, 0);
        ResetWindow();
        Volatile.Write(ref _active, true);
        Emit("start", now);
    }

    public void Stop(TimeSpan now)
    {
        if (!IsActive) return;
        Volatile.Write(ref _active, false);
        Emit("stop", now);
        _lastFrame = null;
    }

    public void RecordAudio(TimeSpan timestamp, TimeSpan work)
    {
        if (!IsActive) return;
        Interlocked.Increment(ref _audioCount);
        var previous = Interlocked.Exchange(ref _lastAudioTicks, timestamp.Ticks);
        if (previous > 0) AtomicMax(ref _audioMaxGapTicks, timestamp.Ticks - previous);
        AtomicMax(ref _audioMaxWorkTicks, work.Ticks);
    }

    public void DuplicateFrame() { if (IsActive) _duplicates++; }

    public void BeginFrame(TimeSpan now, TimeSpan presentationElapsed)
    {
        if (!IsActive) return;
        _currentGap = _lastFrame.HasValue ? Math.Max(0, (now - _lastFrame.Value).TotalMilliseconds) : 0;
        _currentPresentationGap = presentationElapsed.TotalMilliseconds;
        _presentationGapMax = Math.Max(_presentationGapMax, _currentPresentationGap);
        if (_lastFrame.HasValue)
        {
            if (_intervalCount < _intervals.Length) _intervals[_intervalCount++] = _currentGap;
            else _intervalOverflow++;
            if (_calibrationCount < _calibration.Length && _currentPresentationGap is >= 2 and <= 100)
                _calibration[_calibrationCount++] = _currentPresentationGap;
        }
        _lastFrame = now;
        _currentEnvelope = _currentRaster = _currentLock = _currentUpload = 0;
    }

    public void EndFrame(TimeSpan now, double handlerMs)
    {
        if (!IsActive) return;
        _frames++;
        _handler.Add(handlerMs);
        if (_currentGap > (_frameIntervalMs ?? 16.667) * 1.5 || handlerMs > 16.667)
        {
            if (_slowCount < _slowFrames.Length)
                _slowFrames[_slowCount++] = new OverlaySlowFrame((now - _start).TotalSeconds, _currentGap,
                    _currentPresentationGap, handlerMs, _currentEnvelope, _currentRaster, _currentLock, _currentUpload);
        }
        FlushIfDue(now);
    }

    public void Heartbeat(TimeSpan now)
    {
        if (!IsActive) return;
        _heartbeatGapMax = Math.Max(_heartbeatGapMax, (now - _lastHeartbeat).TotalMilliseconds);
        _lastHeartbeat = now;
        FlushIfDue(now);
    }

    public void RecordEnvelope(double ms) { if (IsActive) { _envelope.Add(ms); _currentEnvelope += ms; } }
    public void RecordRaster(double ms) { if (IsActive) { _raster.Add(ms); _currentRaster += ms; } }
    public void RecordLock(double ms, bool busy, bool blocking)
    {
        if (!IsActive) return;
        _lock.Add(ms);
        _currentLock += ms;
        if (busy) _busy++;
        if (blocking) _blockingLocks++;
    }
    public void RecordUpload(double ms) { if (IsActive) { _uploads++; _upload.Add(ms); _currentUpload += ms; } }
    public void RecordOnRender(double ms) { if (IsActive) { _onRenderCalls++; _onRender.Add(ms); } }
    public void SurfaceRebuilt() { if (IsActive) _surfaceRebuilds++; }

    private void FlushIfDue(TimeSpan now)
    {
        if (now - _windowStart >= TimeSpan.FromSeconds(2)) Emit("window", now);
    }

    private void Emit(string kind, TimeSpan now)
    {
        // Bounded snapshots only once per reporting window. Percentiles, drop estimation, JSON,
        // process sampling and disk writes happen in the background consumer.
        var report = new OverlayFrameReport(kind, _session, DateTimeOffset.UtcNow,
            (now - _start).TotalSeconds, (now - _windowStart).TotalSeconds, getContext(),
            _frames, _duplicates, _intervals.AsSpan(0, _intervalCount).ToArray(), _intervalOverflow,
            _calibration.AsSpan(0, _calibrationCount).ToArray(), _frameIntervalMs,
            targetFps.HasValue ? "configured" : "startup-WPF-presentation-clock", _presentationGapMax, _heartbeatGapMax,
            _lastFrame.HasValue ? (now - _lastFrame.Value).TotalMilliseconds : null,
            _handler.Summary, _envelope.Summary, _raster.Summary, _lock.Summary, _upload.Summary, _onRender.Summary,
            _uploads, _busy, _blockingLocks, _surfaceRebuilds, _onRenderCalls,
            Interlocked.Exchange(ref _audioCount, 0),
            Interlocked.Read(ref _lastAudioTicks) is var last && last > 0 ? (now.Ticks - last) / 10000d : null,
            Interlocked.Exchange(ref _audioMaxGapTicks, 0) / 10000d,
            Interlocked.Exchange(ref _audioMaxWorkTicks, 0) / 10000d,
            _slowFrames.AsSpan(0, _slowCount).ToArray());
        // Calibration is fixed once 120 valid startup gaps are available, independent of later lag.
        if (!targetFps.HasValue && _calibrationCount >= _calibration.Length && !_frameIntervalMs.HasValue)
        {
            var sorted = (double[])report.CalibrationIntervalsMs.Clone();
            Array.Sort(sorted);
            _frameIntervalMs = sorted[sorted.Length / 5];
        }
        publish(report);
        _windowStart = now;
        ResetWindow();
    }

    private void ResetWindow()
    {
        _frames = _duplicates = _intervalCount = _slowCount = _intervalOverflow = 0;
        _uploads = _busy = _blockingLocks = _surfaceRebuilds = _onRenderCalls = 0;
        _handler = _envelope = _raster = _lock = _upload = _onRender = default;
        _presentationGapMax = _heartbeatGapMax = 0;
    }

    private static void AtomicMax(ref long target, long value)
    {
        var previous = Interlocked.Read(ref target);
        while (value > previous)
        {
            var actual = Interlocked.CompareExchange(ref target, value, previous);
            if (actual == previous) break;
            previous = actual;
        }
    }

    private struct DurationCounter
    {
        private int _count;
        private double _sum, _max;
        public void Add(double ms) { _count++; _sum += ms; _max = Math.Max(_max, ms); }
        public readonly OverlayDuration Summary => new(_count, _count == 0 ? 0 : _sum / _count, _max);
    }
}

internal readonly record struct OverlayDuration(int Count, double MeanMs, double MaxMs);
internal readonly record struct OverlaySlowFrame(double Seconds, double GapMs, double PresentationGapMs,
    double HandlerMs, double EnvelopeMs, double RasterMs, double BitmapLockMs, double UploadMs);

internal sealed record OverlayFrameContext(string Animation, double WidthDips, double HeightDips,
    double LeftDips, double TopDips, double DpiX, double DpiY, int RenderTier, string ProcessRenderMode,
    string WindowRenderMode, string Hwnd, int BitmapWidth, int BitmapHeight, int PixelBufferBytes,
    long BitmapGeneration, int HistoryCount, WaveformQueueDiagnostics AudioQueue,
    DispatcherFrameSnapshot? Dispatcher = null);

internal sealed record OverlayFrameReport(string Event, string Session, DateTimeOffset Utc,
    double SessionSeconds, double WindowSeconds, OverlayFrameContext Context, int Frames, int DuplicateCallbacks,
    double[] IntervalsMs, int IntervalSamplesOmitted, double[] CalibrationIntervalsMs, double? TargetIntervalMs,
    string CadenceSource, double PresentationGapMaxMs, double HeartbeatGapMaxMs, double? SinceLastFrameMs,
    OverlayDuration Handler, OverlayDuration Envelope, OverlayDuration Raster, OverlayDuration BitmapLock,
    OverlayDuration Upload, OverlayDuration OnRender, int BitmapUploads, int BitmapBusySkips, int BlockingLocks,
    int BitmapRebuilds, int OnRenderCalls, long AudioCallbacks, double? LastAudioAgeMs,
    double AudioGapMaxMs, double AudioCallbackWorkMaxMs, OverlaySlowFrame[] SlowFrames)
{
    public double? EstimateTargetInterval()
    {
        if (TargetIntervalMs.HasValue) return TargetIntervalMs;
        if (CalibrationIntervalsMs.Length < 20) return null;
        var sorted = (double[])CalibrationIntervalsMs.Clone();
        Array.Sort(sorted);
        return sorted[sorted.Length / 5];
    }

    public static long EstimateMissedCallbacks(ReadOnlySpan<double> intervals, double? targetIntervalMs)
    {
        if (targetIntervalMs is not > 0) return 0;
        long missed = 0;
        foreach (var gap in intervals)
            missed += Math.Max(0, (long)Math.Floor(gap / targetIntervalMs.Value + 0.5) - 1);
        return missed;
    }
}
