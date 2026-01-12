#!/usr/bin/env python3
"""
Test transcription with sample audio files.
Tests both English and Ukrainian samples.
"""
import sys
import os
from pathlib import Path

# Add src/transcribe to path
sys.path.insert(0, str(Path(__file__).parent.parent / "src" / "transcribe"))

try:
    from transcribe import transcribe_audio
except ImportError:
    print("ERROR: Could not import transcribe module")
    print("Make sure faster-whisper is installed: pip install faster-whisper")
    sys.exit(1)


def test_transcription(audio_file: Path, expected_lang: str, description: str):
    """Test transcription of a single audio file."""
    print(f"\n{'='*60}")
    print(f"Testing: {description}")
    print(f"File: {audio_file.name}")
    print(f"Expected language: {expected_lang}")
    print(f"{'='*60}")
    
    if not audio_file.exists():
        print(f"ERROR: File not found: {audio_file}")
        return False
    
    try:
        # Test with CPU (CUDA might not be available)
        print("\nTranscribing with CPU...")
        result = transcribe_audio(
            str(audio_file),
            model_size="medium",
            device="cpu"
        )
        
        print(f"\nTranscript: '{result['text']}'")
        print(f"Detected language: {result['language']}")
        print(f"Language probability: {result['language_prob']:.2%}")
        
        # Verify language detection
        if result['language'] == expected_lang:
            print(f"✅ Language detection correct: {expected_lang}")
        else:
            print(f"⚠️  Expected {expected_lang}, got {result['language']}")
        
        # Check for empty transcription
        if not result['text'].strip():
            print("❌ ERROR: Empty transcription")
            return False
        
        print("✅ Test passed")
        return True
        
    except Exception as e:
        print(f"❌ ERROR: {e}")
        import traceback
        traceback.print_exc()
        return False


def main():
    """Run all transcription tests."""
    samples_dir = Path(__file__).parent / "samples"
    
    print("VoicePaste Transcription Test Suite")
    print("=" * 60)
    print(f"Samples directory: {samples_dir}")
    
    if not samples_dir.exists():
        print(f"ERROR: Samples directory not found: {samples_dir}")
        sys.exit(1)
    
    tests = [
        ("en_sample.wav", "en", "English sample"),
        ("ua_sample.wav", "uk", "Ukrainian sample"),
    ]
    
    results = []
    for filename, expected_lang, description in tests:
        audio_file = samples_dir / filename
        passed = test_transcription(audio_file, expected_lang, description)
        results.append((description, passed))
    
    # Summary
    print("\n" + "=" * 60)
    print("TEST SUMMARY")
    print("=" * 60)
    
    passed_count = sum(1 for _, passed in results if passed)
    total_count = len(results)
    
    for description, passed in results:
        status = "✅ PASS" if passed else "❌ FAIL"
        print(f"{status}: {description}")
    
    print(f"\nTotal: {passed_count}/{total_count} tests passed")
    
    if passed_count == total_count:
        print("\n🎉 All tests passed!")
        sys.exit(0)
    else:
        print(f"\n❌ {total_count - passed_count} test(s) failed")
        sys.exit(1)


if __name__ == "__main__":
    main()
