using System.Runtime.ExceptionServices;
using System.Windows.Controls;
using System.Windows;
using System.Windows.Media;
using VoicePaste.Settings;

namespace VoicePaste.Tests;

public class SettingsResidencyUiTests
{
    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void Checkbox_LoadsSavedValueAndMemoryStatusHasItsOwnRow(bool enabled)
    {
        Exception? error = null;
        var thread = new Thread(() =>
        {
            SettingsWindow? window = null;
            try
            {
                window = new SettingsWindow(new SettingsManager(), AppSettings.Default with { KeepInMemory = enabled });
                var checkbox = Assert.IsType<CheckBox>(window.FindName("KeepInMemoryCheck"));
                var status = Assert.IsType<TextBlock>(window.FindName("MemoryUsageText"));
                Assert.Equal(enabled, checkbox.IsChecked);
                Assert.Equal("Keep in memory", checkbox.Content);
                Assert.Equal("Model not loaded", status.Text);
                var panel = Assert.IsType<StackPanel>(checkbox.Parent);
                var grid = Assert.IsType<Grid>(panel.Parent);
                Assert.Equal(5, Grid.GetRow(panel));
                Assert.True(grid.RowDefinitions.Count > Grid.GetRow(panel));
                Assert.Equal(enabled, window.GetSettingsFromUi().KeepInMemory);
            }
            catch (Exception ex) { error = ex; }
            finally { window?.Close(); }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        Assert.True(thread.Join(TimeSpan.FromSeconds(10)), "Settings construction timed out");
        if (error != null) ExceptionDispatchInfo.Capture(error).Throw();
    }

    [Theory]
    [InlineData(RecordingAnimation.LevelBar)]
    [InlineData(RecordingAnimation.Waveform)]
    public void AnimationDropdown_LoadsAndSavesSelectedValue(RecordingAnimation animation)
    {
        Exception? error = null;
        var thread = new Thread(() =>
        {
            SettingsWindow? window = null;
            try
            {
                window = new SettingsWindow(new SettingsManager(), AppSettings.Default with
                    { RecordingAnimation = animation, IndicatorColor = "#1A9FE8" });
                var combo = Assert.IsType<ComboBox>(window.FindName("RecordingAnimationCombo"));
                Assert.Equal(2, combo.Items.Count);
                Assert.Equal(animation, combo.SelectedValue);
                Assert.Equal(animation, window.GetSettingsFromUi().RecordingAnimation);
                var other = animation == RecordingAnimation.LevelBar ? RecordingAnimation.Waveform : RecordingAnimation.LevelBar;
                combo.SelectedValue = other;
                Assert.Equal(other, window.GetSettingsFromUi().RecordingAnimation);
                var swatch = Assert.IsType<Border>(window.FindName("IndicatorColorSwatch"));
                Assert.Equal(Color.FromRgb(26, 159, 232), Assert.IsType<SolidColorBrush>(swatch.Background).Color);
                window.SetIndicatorColorSelection("#aa3377");
                Assert.Equal("#AA3377", window.GetSettingsFromUi().IndicatorColor);
                var hex = Assert.IsType<TextBlock>(window.FindName("IndicatorColorText"));
                Assert.Equal("#AA3377", hex.Text);
                var colorPanel = Assert.IsType<StackPanel>(hex.Parent);
                colorPanel.Children.OfType<Button>().Single(button => Equals(button.Content, "Reset"))
                    .RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
                Assert.Equal("#FF8C00", window.GetSettingsFromUi().IndicatorColor);
                Assert.Null(window.SavedSettings); // Editing is not an implicit Save.
            }
            catch (Exception ex) { error = ex; }
            finally { window?.Close(); }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        Assert.True(thread.Join(TimeSpan.FromSeconds(10)), "Settings construction timed out");
        if (error != null) ExceptionDispatchInfo.Capture(error).Throw();
    }
}
