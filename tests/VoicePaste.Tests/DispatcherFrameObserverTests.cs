using System.Runtime.ExceptionServices;
using System.Windows.Threading;
using VoicePaste.Overlay;

namespace VoicePaste.Tests;

public class DispatcherFrameObserverTests
{
    [Fact]
    public void LongUiOperation_IsObservedOutsideTheWaveformHandler_AndUnsubscribesOnStop()
    {
        RunOnSta(() =>
        {
            var dispatcher = Dispatcher.CurrentDispatcher;
            using var observer = new DispatcherFrameObserver(dispatcher);
            observer.Start();
            RunOperation(dispatcher, () => Thread.Sleep(30));
            var snapshot = observer.Snapshot();
            Assert.True(snapshot.NativeThreadId > 0);
            Assert.True(snapshot.ThreadCpuMs.HasValue);
            Assert.True(snapshot.CompletedOperations >= 2);
            Assert.True(snapshot.MaxOperationMs >= 25);
            Assert.Equal("Normal", snapshot.MaxOperationPriority);
            Assert.Contains(snapshot.SlowOperations, operation => operation.Milliseconds >= 25 && operation.Priority == "Normal");
            Assert.Equal(0, snapshot.OpenOperations);
            Assert.Empty(observer.Snapshot().SlowOperations);

            observer.Dispose();
            RunOperation(dispatcher, () => Thread.Sleep(25));
            Assert.Equal(0, observer.Snapshot().CompletedOperations);

            observer.Start();
            RunOperation(dispatcher, () => Thread.Sleep(25));
            Assert.True(observer.Snapshot().CompletedOperations >= 2);
        });
    }

    [Fact]
    public void NestedMessagePump_DoesNotRetainOpenOperationsOrDoubleCountOuterScope()
    {
        RunOnSta(() =>
        {
            var dispatcher = Dispatcher.CurrentDispatcher;
            using var observer = new DispatcherFrameObserver(dispatcher);
            observer.Start();
            RunOperation(dispatcher, () => RunOperation(dispatcher, () => Thread.Sleep(20)));
            var snapshot = observer.Snapshot();
            Assert.Equal(0, snapshot.OpenOperations);
            Assert.Equal(0, snapshot.UntrackedOperations);
            Assert.True(snapshot.CompletedOperations >= 4);
            Assert.True(snapshot.OutermostOperationMs >= snapshot.MaxOperationMs);
            Assert.True(snapshot.SlowOperations.Sum(operation => operation.Milliseconds) > snapshot.OutermostOperationMs);
        });
    }

    private static void RunOperation(Dispatcher dispatcher, Action action)
    {
        var frame = new DispatcherFrame();
        dispatcher.BeginInvoke(DispatcherPriority.Normal, action);
        dispatcher.BeginInvoke(DispatcherPriority.Background, new Action(() => frame.Continue = false));
        Dispatcher.PushFrame(frame);
    }

    private static void RunOnSta(Action action)
    {
        Exception? error = null;
        var thread = new Thread(() =>
        {
            try { action(); }
            catch (Exception ex) { error = ex; }
            finally { Dispatcher.CurrentDispatcher.InvokeShutdown(); }
        }) { IsBackground = true };
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        Assert.True(thread.Join(TimeSpan.FromSeconds(10)), "Dispatcher observer test timed out.");
        if (error != null) ExceptionDispatchInfo.Capture(error).Throw();
    }
}
