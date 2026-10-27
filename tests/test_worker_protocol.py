"""Exercise the real worker CLI with a deterministic in-process Whisper stub.

Run: python -m unittest discover -s tests -p test_worker_protocol.py -v
No third-party dependencies, model downloads or GPU required.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


WORKER = Path(__file__).resolve().parents[1] / "src" / "transcribe" / "transcribe.py"
RUNNER = r'''
import os
from pathlib import Path
import runpy
import sys
import types

class FakeModel:
    def __init__(self, name, device, compute_type, **kwargs):
        print("MODEL_LOAD:" + device, file=sys.stderr, flush=True)
        if device == "cuda" and os.environ.get("FAKE_CUDA_FAIL"):
            raise RuntimeError("CUDA unavailable")

    def transcribe(self, path, **kwargs):
        text = Path(path).read_text(encoding="utf-8")
        if text == "ERROR":
            raise RuntimeError("Invalid audio")
        segments = [] if text == "EMPTY" else [types.SimpleNamespace(text=text)]
        return segments, types.SimpleNamespace(language="en", language_probability=1.0)

sys.modules["faster_whisper"] = types.SimpleNamespace(WhisperModel=FakeModel)
worker = sys.argv[1]
sys.argv = sys.argv[1:]
runpy.run_path(worker, run_name="__main__")
'''


class WorkerProtocolTests(unittest.TestCase):
    def run_worker(self, texts: list[str], *arguments: str, cuda_fail: bool = False,
                   debug: bool = False) -> subprocess.CompletedProcess:
        with tempfile.TemporaryDirectory(prefix="VoicePaste.Tests-") as directory:
            paths = []
            for index, text in enumerate(texts):
                path = Path(directory) / f"запис {index}.wav"
                path.write_text(text, encoding="utf-8")
                paths.append(str(path))
            environment = {**os.environ, "PYTHONIOENCODING": "utf-8", "VOICEPASTE_DEBUG": "1" if debug else "0"}
            environment.pop("FAKE_CUDA_FAIL", None)
            if cuda_fail:
                environment["FAKE_CUDA_FAIL"] = "1"
            return subprocess.run(
                [sys.executable, "-u", "-c", RUNNER, str(WORKER), "--wait", *arguments],
                input="\n" + "\n".join(paths) + "\nQUIT\n",
                capture_output=True, encoding="utf-8", env=environment, timeout=10,
            )

    def test_multiple_results_empty_text_and_error_have_distinct_frames(self) -> None:
        completed = self.run_worker(["Привіт\nEnglish", "EMPTY", "ERROR", "next"], "--device", "cpu")
        self.assertEqual(0, completed.returncode, completed.stderr)
        messages = [json.loads(line) for line in completed.stdout.splitlines()]
        self.assertEqual([
            {"type": "ready", "device": "cpu"},
            {"type": "result", "text": "Привіт\nEnglish"},
            {"type": "result", "text": ""},
            {"type": "error", "message": "Invalid audio"},
            {"type": "result", "text": "next"},
        ], messages)
        self.assertEqual(1, completed.stderr.count("MODEL_LOAD:"))

    def test_constructor_fallback_reports_actual_device(self) -> None:
        completed = self.run_worker(["hello"], "--device", "cuda", cuda_fail=True)
        self.assertEqual(0, completed.returncode, completed.stderr)
        messages = [json.loads(line) for line in completed.stdout.splitlines()]
        self.assertEqual({"type": "ready", "device": "cpu"}, messages[0])
        self.assertEqual({"type": "result", "text": "hello"}, messages[1])
        self.assertIn("on cpu (compute_type=int8)", completed.stderr)
        self.assertEqual(2, completed.stderr.count("MODEL_LOAD:"))

    def test_gpu_only_does_not_load_cpu(self) -> None:
        completed = self.run_worker([], "--device", "cuda", "--no-fallback", cuda_fail=True)
        self.assertEqual(1, completed.returncode)
        self.assertEqual("", completed.stdout)
        self.assertIn("CUDA_ERROR", completed.stderr)
        self.assertNotIn("MODEL_LOAD:cpu", completed.stderr)

    def test_verbose_request_diagnostics_are_disabled_by_default(self) -> None:
        completed = self.run_worker(["private dictation"], "--device", "cpu")
        self.assertEqual(0, completed.returncode, completed.stderr)
        self.assertNotIn("private dictation", completed.stderr)
        self.assertNotIn("[Timer]", completed.stderr)
        self.assertNotIn("[Transcribe]", completed.stderr)
        self.assertNotIn("Received path", completed.stderr)

    def test_debug_diagnostics_preserve_json_protocol_without_logging_text(self) -> None:
        completed = self.run_worker(["private dictation"], "--device", "cpu", debug=True)
        self.assertEqual(0, completed.returncode, completed.stderr)
        self.assertIn("[Transcribe]", completed.stderr)
        self.assertNotIn("private dictation", completed.stderr)
        messages = [json.loads(line) for line in completed.stdout.splitlines()]
        self.assertEqual("private dictation", messages[1]["text"])


if __name__ == "__main__":
    unittest.main()
