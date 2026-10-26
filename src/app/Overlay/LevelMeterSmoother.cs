namespace VoicePaste.Overlay;

/// <summary>Display-only gain and time-based ballistics; never modifies recorded audio.</summary>
internal sealed class LevelMeterSmoother
{
    // AudioRecorder already supplies RMS * 5. Apply a further 5x to the existing meter input.
    internal const double VisualGain = 5;
    private const double ReleaseSeconds = 0.14;
    private double _level;

    public double Update(float input, TimeSpan elapsed)
    {
        var target = float.IsFinite(input) ? Math.Clamp(input * VisualGain, 0, 1) : 0;
        // No attack ramp: new peaks are displayed on the very next render frame.
        if (target >= _level) return _level = target;
        var seconds = Math.Max(0, elapsed.TotalSeconds);
        var blend = 1 - Math.Exp(-seconds / ReleaseSeconds);
        _level += (target - _level) * blend;
        return _level;
    }

    public void Reset() => _level = 0;
}
