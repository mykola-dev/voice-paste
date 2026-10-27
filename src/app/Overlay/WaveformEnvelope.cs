namespace VoicePaste.Overlay;

/// <summary>Bounded audio/UI handoff with measurement-time ballistics, independent of render rate.</summary>
internal sealed class WaveformEnvelope
{
    private readonly object _sync = new();
    private readonly Measurement[] _pending = new Measurement[WaveformHistory.Capacity];
    private readonly LevelMeterSmoother _smoother = new();
    private int _first;
    private int _count;
    private TimeSpan? _lastMeasurementTime;
    private TimeSpan? _captureOrigin;
    private TimeSpan _presentationTime;
    private float _lastInput;

    public WaveformHistory History { get; } = new();

    public void AddMeasurement(float level, TimeSpan timestamp)
    {
        lock (_sync)
        {
            if (_count == _pending.Length)
            {
                _first = (_first + 1) % _pending.Length;
                _count--;
            }
            _pending[(_first + _count++) % _pending.Length] = new Measurement(level, timestamp);
        }
    }

    public void Advance(TimeSpan elapsed, TimeSpan captureTime)
    {
        Span<Measurement> measurements = stackalloc Measurement[WaveformHistory.Capacity];
        int count;
        lock (_sync)
        {
            count = _count;
            for (var i = 0; i < count; i++) measurements[i] = _pending[(_first + i) % _pending.Length];
            _first = _count = 0;
        }

        History.Advance(elapsed);
        if (elapsed.TotalSeconds >= WaveformHistory.VisibleSeconds)
        {
            _smoother.Reset();
            _lastMeasurementTime = null;
            _lastInput = 0;
            _captureOrigin = captureTime;
            _presentationTime = TimeSpan.Zero;
        }
        else if (elapsed > TimeSpan.Zero) _presentationTime += elapsed;
        // Fix the mapping once, rather than re-anchoring history to callback/UI scheduling jitter.
        _captureOrigin ??= captureTime - _presentationTime;
        foreach (var measurement in measurements[..count])
        {
            var captureAge = captureTime - measurement.Timestamp;
            var age = _presentationTime - (measurement.Timestamp - _captureOrigin.Value);
            if (captureAge.TotalSeconds >= WaveformHistory.VisibleSeconds || captureAge < TimeSpan.Zero ||
                age.TotalSeconds >= WaveformHistory.VisibleSeconds ||
                (_lastMeasurementTime.HasValue && measurement.Timestamp < _lastMeasurementTime.Value))
            {
                continue;
            }
            var delta = _lastMeasurementTime.HasValue ? measurement.Timestamp - _lastMeasurementTime.Value : TimeSpan.Zero;
            var level = _smoother.Update(measurement.Level, delta);
            History.AddMeasurement(level, age);
            _lastMeasurementTime = measurement.Timestamp;
            _lastInput = measurement.Level;
        }

        // Release can continue at the live edge every frame, without committing those preview
        // values into history or changing the response at the next real measurement.
        if (_lastMeasurementTime.HasValue)
            History.SetLiveLevel(_smoother.Preview(_lastInput,
                _presentationTime - (_lastMeasurementTime.Value - _captureOrigin.Value)));
    }

    public void Reset()
    {
        lock (_sync) { _first = _count = 0; }
        _lastMeasurementTime = null;
        _captureOrigin = null;
        _presentationTime = TimeSpan.Zero;
        _lastInput = 0;
        _smoother.Reset();
        History.Reset();
    }

    private readonly record struct Measurement(float Level, TimeSpan Timestamp);
}
