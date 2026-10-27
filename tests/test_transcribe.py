"""Worker unit tests without Whisper, CUDA, downloads, microphone or clipboard access."""
import contextlib
import importlib.util
import io
import os
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch


WORKER_PATH = Path(__file__).resolve().parents[1] / "src" / "transcribe" / "transcribe.py"
spec = importlib.util.spec_from_file_location("voicepaste_test_worker", WORKER_PATH)
worker = importlib.util.module_from_spec(spec)
with patch.dict(sys.modules, {"faster_whisper": SimpleNamespace(WhisperModel=Mock())}):
    spec.loader.exec_module(worker)


class TranscriptionTests(unittest.TestCase):
    def setUp(self) -> None:
        directory = tempfile.TemporaryDirectory(prefix="VoicePaste.Tests-")
        self.addCleanup(directory.cleanup)
        self.audio_path = Path(directory.name) / "audio.wav"
        self.audio_path.touch()
        self.model = Mock()
        self.model.transcribe.return_value = (
            [SimpleNamespace(text=" Hello "), SimpleNamespace(text=" world ")],
            SimpleNamespace(language="en", language_probability=0.95),
        )
        environment = patch.dict(os.environ, {"VOICEPASTE_DEBUG": "0"})
        environment.start()
        self.addCleanup(environment.stop)

    def test_import_preserves_standard_streams(self) -> None:
        with contextlib.redirect_stdout(io.StringIO()) as stdout, \
             contextlib.redirect_stderr(io.StringIO()) as stderr, \
             patch.dict(sys.modules, {"faster_whisper": SimpleNamespace(WhisperModel=Mock())}):
            spec.loader.exec_module(importlib.util.module_from_spec(spec))
            self.assertIs(stdout, sys.stdout)
            self.assertIs(stderr, sys.stderr)

    def test_missing_audio_does_not_invoke_model(self) -> None:
        with self.assertRaisesRegex(FileNotFoundError, "Audio file not found"):
            worker.transcribe_audio(self.audio_path.with_name("missing.wav"), self.model)
        self.model.transcribe.assert_not_called()

    def test_combines_segments_and_preserves_metadata(self) -> None:
        result = worker.transcribe_audio(self.audio_path, self.model)
        self.assertEqual("Hello world", result["text"])
        self.assertEqual("en", result["language"])
        self.assertEqual(0.95, result["language_prob"])
        self.assertGreaterEqual(result["duration_ms"], 0)
        self.assertNotIn("vad_filter", self.model.transcribe.call_args.kwargs)

    def test_empty_segments_are_a_successful_empty_result(self) -> None:
        self.model.transcribe.return_value = ([], SimpleNamespace(language="en", language_probability=0.5))
        self.assertEqual("", worker.transcribe_audio(self.audio_path, self.model)["text"])

    def test_decoding_options_reach_model(self) -> None:
        worker.transcribe_audio(self.audio_path, self.model, language_mode="en", beam_size=3,
                                custom_initial_prompt="Project vocabulary", enable_vad=True)
        self.model.transcribe.assert_called_once_with(
            str(self.audio_path), language="en", beam_size=3, vad_filter=True,
            initial_prompt="Project vocabulary", suppress_tokens=[-1],
        )

    def test_default_suppression_is_preserved_in_every_language_mode(self) -> None:
        for language_mode in ("auto", "en", "ua", "uk", "bilingual"):
            for enable_vad in (False, True):
                with self.subTest(language_mode=language_mode, enable_vad=enable_vad), \
                     patch.object(worker, "get_russian_suppress_tokens", return_value=[42, 43]) as russian_tokens:
                    self.model.reset_mock()
                    worker.transcribe_audio(self.audio_path, self.model,
                                            language_mode=language_mode, enable_vad=enable_vad)
                    self.model.transcribe.assert_called_once()
                    if language_mode in ("ua", "uk", "bilingual"):
                        expected = [-1, 42, 43]
                        russian_tokens.assert_called_once_with(self.model.hf_tokenizer)
                    else:
                        expected = [-1]
                        russian_tokens.assert_not_called()
                    self.assertEqual(expected, self.model.transcribe.call_args.kwargs["suppress_tokens"])

    def test_empty_custom_suppression_preserves_default_suppression(self) -> None:
        with patch.object(worker, "get_russian_suppress_tokens", return_value=[]):
            worker.transcribe_audio(self.audio_path, self.model, language_mode="ua")
        self.assertEqual([-1], self.model.transcribe.call_args.kwargs["suppress_tokens"])

    def test_model_error_is_not_silently_swallowed(self) -> None:
        self.model.transcribe.side_effect = RuntimeError("Invalid audio")
        with self.assertRaisesRegex(RuntimeError, "Invalid audio"):
            worker.transcribe_audio(self.audio_path, self.model)

    def test_bilingual_russian_detection_retries_ukrainian(self) -> None:
        self.model.transcribe.side_effect = [
            ([], SimpleNamespace(language="ru", language_probability=0.9)),
            ([SimpleNamespace(text="Привіт")], SimpleNamespace(language="uk", language_probability=0.99)),
        ]
        with patch.object(worker, "get_russian_suppress_tokens", return_value=[42]):
            result = worker.transcribe_audio(self.audio_path, self.model, language_mode="bilingual",
                                             custom_initial_prompt="Vocabulary")
        self.assertEqual("Привіт", result["text"])
        self.assertEqual("uk", result["language"])
        first, second = self.model.transcribe.call_args_list
        self.assertIsNone(first.kwargs["language"])
        self.assertEqual("uk", second.kwargs["language"])
        self.assertEqual([-1, 42], first.kwargs["suppress_tokens"])
        self.assertEqual([-1, 42], second.kwargs["suppress_tokens"])
        self.assertTrue(second.kwargs["initial_prompt"].endswith(" Vocabulary"))

    def test_normal_transcription_has_no_diagnostic_output(self) -> None:
        with contextlib.redirect_stderr(io.StringIO()) as stderr:
            worker.transcribe_audio(self.audio_path, self.model)
        self.assertEqual("", stderr.getvalue())

    def test_debug_output_requires_opt_in_and_does_not_include_audio_path(self) -> None:
        with patch.dict(os.environ, {"VOICEPASTE_DEBUG": "true"}), \
             contextlib.redirect_stderr(io.StringIO()) as stderr:
            worker.transcribe_audio(self.audio_path, self.model)
        self.assertIn("[Transcribe]", stderr.getvalue())
        self.assertNotIn(str(self.audio_path), stderr.getvalue())

    def test_check_model_is_local_only(self) -> None:
        with patch.object(sys, "argv", ["transcribe.py", "--check-model", "--model", "medium"]), \
             patch.object(worker, "WhisperModel") as model_class, \
             patch.object(worker, "configure_cuda_paths"), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(0, worker.main())
        model_class.assert_called_once_with("medium", device="cpu", compute_type="int8", local_files_only=True)


class CudaPathTests(unittest.TestCase):
    def test_cuda_discovery_handles_active_and_user_installs_and_missing_path(self) -> None:
        packages = Path("mock Python") / "site-packages"
        user_packages = Path("mock User") / "site-packages"
        cublas = packages / "nvidia" / "cublas" / "bin"
        cudnn = user_packages / "nvidia" / "cudnn" / "bin"
        cases = [
            ({cublas, cudnn}, {"PATH": "existing"}, os.pathsep.join([str(cublas), str(cudnn), "existing"])),
            ({cudnn}, {"PATH": "existing"}, os.pathsep.join([str(cudnn), "existing"])),
            ({cublas}, {}, str(cublas)),
            (set(), {"PATH": "existing"}, "existing"),
            (set(), {}, None),
        ]
        for existing, environment, expected in cases:
            with self.subTest(existing=existing, environment=environment), \
                 patch.object(worker.site, "getsitepackages", return_value=[str(packages), str(packages)]), \
                 patch.object(worker.site, "getusersitepackages", return_value=str(user_packages)), \
                 patch.object(Path, "glob", return_value=[]), \
                 patch.object(Path, "is_dir", autospec=True, side_effect=lambda path: path in existing), \
                 patch.dict(os.environ, environment, clear=True):
                worker.configure_cuda_paths()
                self.assertEqual(expected, os.environ.get("PATH"))
                worker.configure_cuda_paths()
                self.assertEqual(expected, os.environ.get("PATH"))

    def test_native_dll_fallback_does_not_override_active_environment(self) -> None:
        active = Path("mock active") / "site-packages"
        fallback = Path("mock user") / "Python399" / "site-packages"
        active_cublas = active / "nvidia" / "cublas" / "bin"
        fallback_cublas = fallback / "nvidia" / "cublas" / "bin"
        fallback_cudnn = fallback / "nvidia" / "cudnn" / "bin"
        existing = {active_cublas, fallback_cublas, fallback_cudnn}
        with patch.object(worker.site, "getsitepackages", return_value=[str(active)]), \
             patch.object(worker.site, "getusersitepackages", return_value=str(active)), \
             patch.object(Path, "glob", return_value=[fallback]) as glob, \
             patch.object(Path, "is_dir", autospec=True, side_effect=lambda path: path in existing), \
             patch.dict(os.environ, {"PATH": "existing"}, clear=True):
            worker.configure_cuda_paths()
            self.assertEqual(os.pathsep.join([str(active_cublas), str(fallback_cudnn), "existing"]),
                             os.environ["PATH"])
            glob.assert_called_once_with("Python*/site-packages")


if __name__ == "__main__":
    unittest.main()
