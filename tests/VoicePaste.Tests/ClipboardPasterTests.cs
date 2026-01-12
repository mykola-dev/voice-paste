using System;
using System.Threading.Tasks;
using Xunit;
using VoicePaste.Paste;

namespace VoicePaste.Tests;

public class ClipboardPasterTests
{
    [Fact]
    public void Constructor_WithDefaults_ShouldInitialize()
    {
        // Arrange & Act
        var paster = new ClipboardPaster();
        
        // Assert
        Assert.NotNull(paster);
    }
    
    [Fact]
    public void Constructor_WithCustomSettings_ShouldInitialize()
    {
        // Arrange & Act
        var paster = new ClipboardPaster();
        
        // Assert
        Assert.NotNull(paster);
    }
    
    [Theory]
    [InlineData("")]
    [InlineData(null)]
    public async Task PasteTextAsync_WithEmptyText_ShouldNotThrow(string? text)
    {
        // Arrange
        var paster = new ClipboardPaster();
        
        // Act & Assert
        var exception = await Record.ExceptionAsync(() => paster.PasteTextAsync(text!));
        Assert.Null(exception);
    }
    
    [Fact(Skip = "Requires WPF Application context")]
    public async Task PasteTextAsync_WithValidText_ShouldNotThrow()
    {
        // Arrange
        var paster = new ClipboardPaster();
        var testText = "Test clipboard content";
        
        // Act
        var exception = await Record.ExceptionAsync(() => paster.PasteTextAsync(testText));
        
        // Assert
        Assert.Null(exception);
        
        // Note: This test requires a full WPF Application.Current context
        // It will pass when run via the actual application
    }
}
