"""Tests for the transcription worker."""
import pytest
import sys
import tempfile
from pathlib import Path
from unittest.mock import Mock, patch, MagicMock

# Add src directory to path
sys.path.insert(0, str(Path(__file__).parent.parent.parent / "src" / "transcribe"))

import transcribe


def test_transcribe_audio_nonexistent_file():
    """Test that transcribe_audio raises FileNotFoundError for nonexistent files."""
    nonexistent_path = Path("/nonexistent/audio.wav")
    
    with pytest.raises(FileNotFoundError, match="Audio file not found"):
        transcribe.transcribe_audio(nonexistent_path, "medium", "cpu")


def test_transcribe_audio_empty_file():
    """Test handling of empty audio file."""
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tmp:
        tmp_path = Path(tmp.name)
    
    try:
        # This will likely raise an error when trying to load
        with pytest.raises(Exception):
            transcribe.transcribe_audio(tmp_path, "medium", "cpu")
    finally:
        tmp_path.unlink()


@patch('transcribe.WhisperModel')
def test_transcribe_audio_cuda_fallback(mock_model_class):
    """Test CUDA failure falls back to CPU."""
    # Create temp file
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tmp:
        tmp_path = Path(tmp.name)
    
    try:
        # First call (CUDA) raises error, second (CPU) succeeds
        cuda_instance = MagicMock()
        cpu_instance = MagicMock()
        
        # Mock segments and info
        mock_segment = MagicMock()
        mock_segment.text = "Test transcription"
        cpu_instance.transcribe.return_value = ([mock_segment], MagicMock())
        
        def model_side_effect(*args, **kwargs):
            if kwargs.get('device') == 'cuda':
                raise RuntimeError("CUDA not available")
            return cpu_instance
        
        mock_model_class.side_effect = model_side_effect
        
        # Act
        result = transcribe.transcribe_audio(tmp_path, "medium", "cuda")
        
        # Assert
        assert result == "Test transcription"
        assert mock_model_class.call_count == 2  # CUDA then CPU
    finally:
        tmp_path.unlink()


@patch('transcribe.WhisperModel')
def test_transcribe_audio_returns_text(mock_model_class):
    """Test that transcribe_audio returns combined text from segments."""
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tmp:
        tmp_path = Path(tmp.name)
    
    try:
        # Mock model and segments
        mock_instance = MagicMock()
        mock_model_class.return_value = mock_instance
        
        # Create mock segments
        segment1 = MagicMock()
        segment1.text = " Hello "
        segment2 = MagicMock()
        segment2.text = " world "
        
        mock_info = MagicMock()
        mock_instance.transcribe.return_value = ([segment1, segment2], mock_info)
        
        # Act
        result = transcribe.transcribe_audio(tmp_path, "medium", "cpu")
        
        # Assert
        assert result == "Hello world"
        mock_instance.transcribe.assert_called_once()
    finally:
        tmp_path.unlink()


def test_main_missing_input_argument(capsys):
    """Test that main requires --input argument."""
    sys.argv = ["transcribe.py"]
    
    with pytest.raises(SystemExit) as exc_info:
        transcribe.main()
    
    assert exc_info.value.code == 2  # argparse error code


@patch('transcribe.transcribe_audio')
def test_main_with_valid_arguments(mock_transcribe):
    """Test main function with valid arguments."""
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tmp:
        tmp_path = Path(tmp.name)
    
    try:
        mock_transcribe.return_value = "Test output"
        sys.argv = ["transcribe.py", "--input", str(tmp_path), "--model", "small", "--device", "cpu"]
        
        # Act
        result = transcribe.main()
        
        # Assert
        assert result == 0
        mock_transcribe.assert_called_once_with(tmp_path, "small", "cpu")
    finally:
        tmp_path.unlink()


@patch('transcribe.transcribe_audio')
def test_main_with_error(mock_transcribe, capsys):
    """Test main function error handling."""
    with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tmp:
        tmp_path = Path(tmp.name)
    
    try:
        mock_transcribe.side_effect = RuntimeError("Test error")
        sys.argv = ["transcribe.py", "--input", str(tmp_path)]
        
        # Act
        result = transcribe.main()
        
        # Assert
        assert result == 1
        captured = capsys.readouterr()
        assert "Test error" in captured.err
    finally:
        tmp_path.unlink()
