using System.Runtime.ExceptionServices;
using System.Windows;
using System.Windows.Interop;
using VoicePaste.Overlay;

namespace VoicePaste.Tests;

public class RecordingOverlayRenderModeTests
{
    [Theory]
    [InlineData(null, RenderMode.SoftwareOnly)]
    [InlineData("", RenderMode.SoftwareOnly)]
    [InlineData("0", RenderMode.Default)]
    [InlineData("true", RenderMode.SoftwareOnly)]
    [InlineData("1", RenderMode.SoftwareOnly)]
    public void SoftwareRendering_IsDefault_WithExplicitHardwareOptOut(string? value, RenderMode expected)
        => Assert.Equal(expected, RecordingOverlay.SelectRenderMode(value));

    [Theory]
    [InlineData(RenderMode.Default)]
    [InlineData(RenderMode.SoftwareOnly)]
    public void RenderOverride_AppliesWhenHandleIsCreated_AndDoesNotAffectOtherWindows(RenderMode mode)
    {
        Exception? error = null;
        var thread = new Thread(() =>
        {
            HwndSource? overlaySource = null, otherSource = null;
            try
            {
                var processMode = System.Windows.Media.RenderOptions.ProcessRenderMode;
                var overlay = new RecordingOverlay(mode);
                var hwnd = new WindowInteropHelper(overlay).EnsureHandle();
                overlaySource = HwndSource.FromHwnd(hwnd);
                Assert.Equal(mode, overlaySource.CompositionTarget.RenderMode);
                Assert.False(overlay.IsVisible);
                Assert.False(overlay.ShowActivated);
                Assert.True(overlay.AllowsTransparency);

                var otherWindow = new Window();
                otherSource = HwndSource.FromHwnd(new WindowInteropHelper(otherWindow).EnsureHandle());
                Assert.Equal(RenderMode.Default, otherSource.CompositionTarget.RenderMode);
                Assert.Equal(processMode, System.Windows.Media.RenderOptions.ProcessRenderMode);
            }
            catch (Exception ex) { error = ex; }
            finally
            {
                overlaySource?.Dispose();
                otherSource?.Dispose();
                System.Windows.Threading.Dispatcher.CurrentDispatcher.InvokeShutdown();
            }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        Assert.True(thread.Join(TimeSpan.FromSeconds(10)), "Overlay rendering-mode test timed out.");
        if (error != null) ExceptionDispatchInfo.Capture(error).Throw();
    }
}
