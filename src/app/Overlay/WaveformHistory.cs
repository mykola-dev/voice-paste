namespace VoicePaste.Overlay;

/// <summary>Timestamped volume envelope. Committed points translate without changing height.</summary>
internal sealed class WaveformHistory
{
    public const double VisibleSeconds = 1.62;
    internal const int Capacity = 512;
    private readonly Sample[] _samples = new Sample[Capacity];
    private int _first;
    private double _time;

    public int Count { get; private set; }

    public WaveformHistory() => Reset();

    public void Update(double level, TimeSpan elapsed)
    {
        level = double.IsFinite(level) ? Math.Clamp(level, 0, 1) : 0;
        var seconds = Math.Max(0, elapsed.TotalSeconds);
        // Do not stretch an old peak across the window after a long UI stall.
        if (seconds >= VisibleSeconds) Reset();
        else _time += seconds;

        var newest = (_first + Count - 1) % Capacity;
        if (_samples[newest].Time == _time)
            _samples[newest] = new Sample(_time, level);
        else
            Append(_time, level);

        // Keep one offscreen predecessor so the contour crosses the left clip edge naturally.
        while (Count > 2 && _samples[(_first + 1) % Capacity].Time <= _time - VisibleSeconds)
        {
            _first = (_first + 1) % Capacity;
            Count--;
        }
    }

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
        Append(-VisibleSeconds, 0);
        Append(-0.06, 0);
        Append(0, 0);
    }

    private readonly record struct Sample(double Time, double Level);
}
