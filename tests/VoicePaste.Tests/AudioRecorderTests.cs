using System;
using System.IO;
using Xunit;
using VoicePaste.Audio;

namespace VoicePaste.Tests;

public class AudioRecorderTests
{
    [Fact]
    public void Constructor_ShouldInitialize()
    {
        // Arrange & Act
        using var recorder = new AudioRecorder();
        
        // Assert
        Assert.NotNull(recorder);
        Assert.False(recorder.IsRecording);
    }
    
    [Fact]
    public void IsRecording_InitialState_ShouldBeFalse()
    {
        // Arrange
        using var recorder = new AudioRecorder();
        
        // Act
        var isRecording = recorder.IsRecording;
        
        // Assert
        Assert.False(isRecording);
    }
    
    [Fact]
    public void StartRecording_ShouldSetIsRecordingToTrue()
    {
        // Arrange
        using var recorder = new AudioRecorder();
        
        try
        {
            // Act
            recorder.StartRecording();
            
            // Assert
            Assert.True(recorder.IsRecording);
        }
        finally
        {
            // Cleanup
            if (recorder.IsRecording)
                recorder.StopRecording();
        }
    }
    
    [Fact]
    public void StartRecording_WhenAlreadyRecording_ShouldThrowInvalidOperationException()
    {
        // Arrange
        using var recorder = new AudioRecorder();
        recorder.StartRecording();
        
        try
        {
            // Act & Assert
            Assert.Throws<InvalidOperationException>(() => recorder.StartRecording());
        }
        finally
        {
            // Cleanup
            recorder.StopRecording();
        }
    }
    
    [Fact]
    public void StopRecording_WhenNotRecording_ShouldThrowInvalidOperationException()
    {
        // Arrange
        using var recorder = new AudioRecorder();
        
        // Act & Assert
        Assert.Throws<InvalidOperationException>(() => recorder.StopRecording());
    }
    
    [Fact]
    public void StopRecording_ShouldReturnFilePath()
    {
        // Arrange
        using var recorder = new AudioRecorder();
        recorder.StartRecording();
        
        // Give it a moment to record something
        System.Threading.Thread.Sleep(100);
        
        // Act
        var filePath = recorder.StopRecording();
        
        // Assert
        Assert.NotNull(filePath);
        Assert.True(File.Exists(filePath), $"Audio file should exist at: {filePath}");
        Assert.EndsWith(".wav", filePath);
        
        // Cleanup
        if (File.Exists(filePath))
            File.Delete(filePath);
    }
    
    [Fact]
    public void LevelChanged_ShouldFireDuringRecording()
    {
        // Arrange
        using var recorder = new AudioRecorder();
        bool eventFired = false;
        float receivedLevel = 0f;
        
        recorder.LevelChanged += (sender, level) =>
        {
            eventFired = true;
            receivedLevel = level;
        };
        
        // Act
        recorder.StartRecording();
        System.Threading.Thread.Sleep(200); // Wait for audio data
        recorder.StopRecording();
        
        // Assert
        Assert.True(eventFired, "LevelChanged event should fire during recording");
        Assert.True(receivedLevel >= 0f && receivedLevel <= 1f, 
            $"Audio level should be between 0 and 1, got {receivedLevel}");
    }
    
    [Fact]
    public void Dispose_ShouldCleanup()
    {
        // Arrange
        var recorder = new AudioRecorder();
        recorder.StartRecording();
        var filePath = recorder.StopRecording();
        
        // Act
        recorder.Dispose();
        
        // Assert - should not throw
        Assert.True(true, "Dispose completed without exception");
        
        // Cleanup
        if (File.Exists(filePath))
            File.Delete(filePath);
    }
}
