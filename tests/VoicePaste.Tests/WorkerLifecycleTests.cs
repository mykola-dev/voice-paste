using System.Collections.Concurrent;
using System.Diagnostics;
using System.IO;
using System.Text;
using VoicePaste.Settings;
using VoicePaste.Transcription;

namespace VoicePaste.Tests;

public sealed class WorkerLifecycleTests : IDisposable
{
    private readonly string _directory = Path.Combine(Path.GetTempPath(), "VoicePaste.Tests", Guid.NewGuid().ToString("N"));
    private readonly ConcurrentQueue<string> _startedDevices = new();
    private readonly string _python = PythonFinder.Find();
    private TranscriptionService? _service;

    private TranscriptionService CreateService(bool keep = true, string device = "cpu", bool fallback = true,
        string mode = "normal", int readyTimeoutMs = 5000, int resultTimeoutMs = 5000)
    {
        _service = new TranscriptionService("test-model", device, LanguageMode.Auto, 5, fallback, "", false, keep,
            actualDevice =>
            {
                _startedDevices.Enqueue(actualDevice);
                var info = new ProcessStartInfo
                {
                    FileName = _python,
                    ArgumentList = { "-u", Path.Combine(AppContext.BaseDirectory, "Fixtures", "fake_worker.py"), actualDevice, mode },
                    UseShellExecute = false,
                    RedirectStandardInput = true,
                    RedirectStandardOutput = true,
                    RedirectStandardError = true,
                    StandardInputEncoding = new UTF8Encoding(false),
                    StandardOutputEncoding = Encoding.UTF8,
                    StandardErrorEncoding = Encoding.UTF8,
                    CreateNoWindow = true
                };
                info.Environment["PYTHONIOENCODING"] = "utf-8";
                return new Process { StartInfo = info };
            }, TimeSpan.FromMilliseconds(readyTimeoutMs), TimeSpan.FromMilliseconds(resultTimeoutMs));
        return _service;
    }

    private string Audio(string text)
    {
        Directory.CreateDirectory(_directory);
        var path = Path.Combine(_directory, $"запис {Guid.NewGuid():N}.wav");
        File.WriteAllText(path, text, new UTF8Encoding(false));
        return path;
    }

    private static async Task<WorkerMemoryUsage> WaitUntilReadyAsync(TranscriptionService service)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(10));
        while (true)
        {
            var usage = await service.GetMemoryUsageAsync(timeout.Token);
            if (usage.IsReady) return usage;
            await Task.Delay(20, timeout.Token);
        }
    }

    [Fact]
    public async Task RetainedWorker_ReusesProcessAndReturnsFreshUnicodeResults()
    {
        var service = CreateService();
        service.StartPreloading();
        var initial = await WaitUntilReadyAsync(service);
        service.StartPreloading();
        var first = await service.TranscribeAsync(Audio("Привіт\nEnglish"));
        var second = await service.TranscribeAsync(Audio("READY"));
        Assert.Equal($"{initial.ProcessId}|1|cpu|Привіт\nEnglish", first);
        Assert.Equal($"{initial.ProcessId}|2|cpu|READY", second);
        Assert.Single(_startedDevices);
        Assert.True(initial.RamBytes > 0);
        Assert.Null(initial.VramBytes);
    }

    [Fact]
    public async Task Disabled_UnloadsAfterEveryResult()
    {
        var service = CreateService(keep: false);
        service.StartPreloading();
        var first = await service.TranscribeAsync(Audio("first"));
        Assert.Null((await service.GetMemoryUsageAsync()).ProcessId);
        var second = await service.TranscribeAsync(Audio("second"));
        Assert.NotEqual(first.Split('|')[0], second.Split('|')[0]);
        Assert.Equal(2, _startedDevices.Count);
        Assert.Null((await service.GetMemoryUsageAsync()).ProcessId);
    }

    [Fact]
    public async Task EmptyResult_CompletesWithoutStaleTextOrTimeout()
    {
        var service = CreateService();
        await service.TranscribeAsync(Audio("first"));
        Assert.Equal("", await service.TranscribeAsync(Audio("EMPTY")));
        Assert.EndsWith("|3|cpu|third", await service.TranscribeAsync(Audio("third")));
        Assert.Single(_startedDevices);
    }

    [Theory]
    [InlineData("ERROR")]
    [InlineData("MALFORMED")]
    [InlineData("EXIT")]
    public async Task WorkerFailure_IsPromptAndNextRequestRecovers(string action)
    {
        var service = CreateService();
        await Assert.ThrowsAsync<TranscriptionException>(() => service.TranscribeAsync(Audio(action)));
        Assert.Null((await service.GetMemoryUsageAsync()).ProcessId);
        Assert.EndsWith("|1|cpu|recovered", await service.TranscribeAsync(Audio("recovered")));
        Assert.Equal(2, _startedDevices.Count);
    }

    [Fact]
    public async Task Timeout_DiscardsWorkerAndCannotLeakLateResult()
    {
        var service = CreateService(resultTimeoutMs: 200);
        await Assert.ThrowsAsync<TranscriptionException>(() => service.TranscribeAsync(Audio("TIMEOUT")));
        Assert.Null((await service.GetMemoryUsageAsync()).ProcessId);
        Assert.EndsWith("|1|cpu|fresh", await service.TranscribeAsync(Audio("fresh")));
    }

    [Fact]
    public async Task EnableAndDisable_LoadsThenReleasesIdleWorker()
    {
        var service = CreateService(keep: false);
        Assert.Null((await service.GetMemoryUsageAsync()).ProcessId);
        service.SetKeepInMemory(true);
        service.StartPreloading();
        await WaitUntilReadyAsync(service);
        service.SetKeepInMemory(false);
        Assert.Null((await service.GetMemoryUsageAsync()).ProcessId);
    }

    [Fact]
    public async Task DisableDuringRequest_FinishesRequestBeforeUnloading()
    {
        var service = CreateService();
        service.StartPreloading();
        await WaitUntilReadyAsync(service);
        var result = service.TranscribeAsync(Audio("SLOW"));
        service.SetKeepInMemory(false);
        Assert.NotNull((await service.GetMemoryUsageAsync()).ProcessId);
        Assert.EndsWith("|1|cpu|SLOW", await result);
        Assert.Null((await service.GetMemoryUsageAsync()).ProcessId);
    }

    [Fact]
    public async Task CudaRequestFailure_RetainsCpuFallbackForSubsequentRequests()
    {
        var service = CreateService(device: "cuda");
        var notifications = 0;
        service.CudaFallbackOccurred += (_, _) => Interlocked.Increment(ref notifications);
        Assert.EndsWith("|1|cpu|CUDA_ERROR", await service.TranscribeAsync(Audio("CUDA_ERROR")));
        Assert.EndsWith("|2|cpu|next", await service.TranscribeAsync(Audio("next")));
        Assert.Equal(new[] { "cuda", "cpu" }, _startedDevices.ToArray());
        Assert.Equal(1, notifications);
        Assert.Equal("cpu", (await service.GetMemoryUsageAsync()).Device);
    }

    [Fact]
    public async Task CudaOnly_DoesNotFallback()
    {
        var service = CreateService(device: "cuda", fallback: false);
        await Assert.ThrowsAsync<TranscriptionException>(() => service.TranscribeAsync(Audio("CUDA_ERROR")));
        Assert.Equal(new[] { "cuda" }, _startedDevices.ToArray());
    }

    [Fact]
    public async Task NonCudaRequestError_DoesNotFallbackBecauseOfOldCudaLogs()
    {
        var service = CreateService(device: "cuda");
        await Assert.ThrowsAsync<TranscriptionException>(() => service.TranscribeAsync(Audio("ERROR")));
        Assert.Equal(new[] { "cuda" }, _startedDevices.ToArray());
    }

    [Fact]
    public async Task DisableDuringPreload_CancelsQueuedPreloadsWithoutResurrection()
    {
        var service = CreateService(mode: "hang-init");
        service.StartPreloading();
        service.StartPreloading();
        service.SetKeepInMemory(false);
        await Task.Delay(200);
        Assert.Null((await service.GetMemoryUsageAsync()).ProcessId);
        Assert.Single(_startedDevices);
    }

    [Fact]
    public async Task ConcurrentRequests_AreSerializedWithDistinctResults()
    {
        var service = CreateService();
        var first = service.TranscribeAsync(Audio("SLOW"));
        var second = service.TranscribeAsync(Audio("second"));
        Assert.EndsWith("|1|cpu|SLOW", await first);
        Assert.EndsWith("|2|cpu|second", await second);
        Assert.Single(_startedDevices);
    }

    [Fact]
    public async Task CudaInitializationExit_FallsBackUsingDrainedStderr()
    {
        var service = CreateService(device: "cuda", mode: "init-exit");
        Assert.EndsWith("|1|cpu|hello", await service.TranscribeAsync(Audio("hello")));
        Assert.Equal(new[] { "cuda", "cpu" }, _startedDevices.ToArray());
    }

    [Fact]
    public async Task WorkerSideFallback_ReportsActualCpuDeviceAndNotifiesOnce()
    {
        var service = CreateService(device: "cuda", mode: "init-fallback");
        var notifications = 0;
        service.CudaFallbackOccurred += (_, _) => Interlocked.Increment(ref notifications);
        await service.TranscribeAsync(Audio("hello"));
        var memory = await service.GetMemoryUsageAsync();
        Assert.Equal("cpu", memory.Device);
        Assert.Null(memory.VramBytes);
        Assert.Equal(1, notifications);
    }

    [Fact]
    public async Task PreloadTimeout_IsReportedAndUnloadsWorker()
    {
        var service = CreateService(mode: "hang-init", readyTimeoutMs: 200);
        var failure = new TaskCompletionSource<string>(TaskCreationOptions.RunContinuationsAsynchronously);
        service.PreloadingFailed += (_, error) => failure.TrySetResult(error);
        service.StartPreloading();
        await failure.Task.WaitAsync(TimeSpan.FromSeconds(10));
        Assert.Null((await service.GetMemoryUsageAsync()).ProcessId);
    }

    [Fact]
    public async Task Dispose_ReleasesWorkerAndPreventsRestart()
    {
        var service = CreateService();
        service.StartPreloading();
        await WaitUntilReadyAsync(service);
        service.Dispose();
        Assert.Null((await service.GetMemoryUsageAsync()).ProcessId);
        Assert.Throws<ObjectDisposedException>(service.StartPreloading);
        await Assert.ThrowsAsync<ObjectDisposedException>(() => service.TranscribeAsync(Audio("hello")));
    }

    public void Dispose()
    {
        _service?.Dispose();
        if (Directory.Exists(_directory)) Directory.Delete(_directory, true);
    }
}
