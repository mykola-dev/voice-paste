namespace VoicePaste.Overlay;

/// <summary>Timestamped measurements; presentation frames advance time without duplicating held input.</summary>
internal sealed class WaveformHistory
{
    public const double VisibleSeconds = 1.62;
    internal const int Capacity = 512;
    private readonly Sample[] _samples = new Sample[Capacity];
    private int _first;
    private double _time;
    private bool _hasMeasurements;

    public int Count { get; private set; }
    public double LiveLevel { get; private set; }

    public WaveformHistory() => Reset();

    public void Update(double level, TimeSpan elapsed)
    {
        Advance(elapsed);
        AddMeasurement(level, TimeSpan.Zero);
    }

    public void Advance(TimeSpan elapsed)
    {
        var seconds = Math.Max(0, elapsed.TotalSeconds);
        // Do not stretch an old peak across the window after a long UI stall.
        if (seconds >= VisibleSeconds) Reset();
        else _time += seconds;

        // Two predecessors preserve the tangent of the segment crossing the left clip edge.
        while (Count > 3 && _samples[(_first + 2) % Capacity].Time <= _time - VisibleSeconds)
        {
            _first = (_first + 1) % Capacity;
            Count--;
        }
    }

    public void AddMeasurement(double level, TimeSpan age)
    {
        level = Sanitize(level);
        var time = _time - Math.Max(0, age.TotalSeconds);
        if (!_hasMeasurements && time <= _samples[_first].Time) return;
        if (!_hasMeasurements)
        {
            // The reset baseline at the live edge is not a real observation. A first callback
            // may predate the first render frame, so retain only seeds preceding its timestamp.
            while (Count > 1 && _samples[(_first + Count - 1) % Capacity].Time > time) Count--;
        }
        // Late/stale callbacks cannot rewrite already committed measurements.
        var newest = (_first + Count - 1) % Capacity;
        if (time < _samples[newest].Time) return;
        if (time == _samples[newest].Time)
            _samples[newest] = new Sample(time, level);
        else
            Append(time, level);
        _hasMeasurements = true;
        LiveLevel = level;
    }

    public void SetLiveLevel(double level) => LiveLevel = Sanitize(level);

    public (double AgeSeconds, double Level) GetSample(int index)
    {
        ArgumentOutOfRangeException.ThrowIfNegative(index);
        ArgumentOutOfRangeException.ThrowIfGreaterThanOrEqual(index, Count);
        var sample = _samples[(_first + index) % Capacity];
        return (_time - sample.Time, sample.Level);
    }

    private void Append(double time, double level)
    {
        if (Count == Capacity)
        {
            _first = (_first + 1) % Capacity;
            Count--;
        }
        _samples[(_first + Count) % Capacity] = new Sample(time, level);
        Count++;
    }

    public void Reset()
    {
        _first = 0;
        _time = 0;
        Count = 0;
        LiveLevel = 0;
        _hasMeasurements = false;
        Append(-VisibleSeconds, 0);
        Append(-0.06, 0);
        Append(0, 0);
    }

    private static double Sanitize(double level) => double.IsFinite(level) ? Math.Clamp(level, 0, 1) : 0;

    private readonly record struct Sample(double Time, double Level);
}
