using System;
using System.IO;
using Xunit;
using VoicePaste.Transcription;

namespace VoicePaste.Tests;

public class TranscriptionServiceTests
{
    [Fact]
    public void Constructor_ShouldInitialize()
    {
        // Arrange & Act
        var service = new TranscriptionService("medium", "cpu");
        
        // Assert
        Assert.NotNull(service);
    }
    
    [Fact]
    public void Constructor_WithInvalidModel_ShouldNotThrow()
    {
        // Arrange & Act & Assert
        var exception = Record.Exception(() => new TranscriptionService("invalid-model", "cpu"));
        Assert.Null(exception);
    }
    
    [Fact]
    public async Task TranscribeAsync_WithNonExistentFile_ShouldThrowFileNotFoundException()
    {
        // Arrange
        var service = new TranscriptionService("medium", "cpu");
        var nonExistentPath = Path.Combine(Path.GetTempPath(), "nonexistent.wav");
        
        // Act & Assert
        await Assert.ThrowsAsync<FileNotFoundException>(() => 
            service.TranscribeAsync(nonExistentPath)
        );
    }
    
    [Fact]
    public void TranscriptionException_ShouldHaveMessage()
    {
        // Arrange & Act
        var exception = new TranscriptionException("Test error");
        
        // Assert
        Assert.Equal("Test error", exception.Message);
    }
    
    [Fact]
    public void TranscriptionException_WithInnerException_ShouldPreserveInner()
    {
        // Arrange
        var inner = new InvalidOperationException("Inner error");
        
        // Act
        var exception = new TranscriptionException("Outer error", inner);
        
        // Assert
        Assert.Equal("Outer error", exception.Message);
        Assert.Same(inner, exception.InnerException);
    }
}
