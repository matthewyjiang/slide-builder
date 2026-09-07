"""Best-effort macOS diagnostics, without requiring macOS or sudo."""

from contextlib import redirect_stdout
import io
import json
from pathlib import Path
import runpy
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import macos_ci_diagnostics as diagnostics


class DiagnosticsTests(unittest.TestCase):
    def test_script_collects_reports_when_log_fails(self):
        for error in (subprocess.CalledProcessError(1, "log"), FileNotFoundError("sudo")):
            with self.subTest(error=error), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                report = root / "slide-builder.ips"
                report.write_text('{}\n' + json.dumps({
                    "exception": "test exception",
                    "termination": "test termination",
                    "asi": "test information",
                    "threads": [{"triggered": True, "frames": ["test frame"]}],
                }))
                output = io.StringIO()
                with (
                    patch.object(Path, "is_file", return_value=False),
                    patch.object(Path, "glob", side_effect=[[report], []]),
                    patch.dict(diagnostics.os.environ, SANDBOX_STARTED_AT="2026-09-07 00:00:00"),
                    patch.object(subprocess, "run", side_effect=error) as run,
                    redirect_stdout(output),
                ):
                    runpy.run_path(str(Path(diagnostics.__file__)), run_name="__main__")
                self.assertIn("could not collect sandbox log:", output.getvalue())
                self.assertIn("exception: test exception", output.getvalue())
                self.assertIn("termination: test termination", output.getvalue())
                self.assertIn("application information: test information", output.getvalue())
                self.assertIn("faulting frames: ['test frame']", output.getvalue())
                self.assertTrue(run.call_args.kwargs["check"])
                self.assertIn("2026-09-07 00:00:00", run.call_args.args[0])

    def test_bad_reports_do_not_skip_later_reports(self):
        bad_contents = [b"not json", b"{}\nnot json", b"{}\n[]", b"{}\n{\"threads\": null}",
                        b"{}\n{\"threads\": [42]}", b"\xff"]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            paths = []
            for index, content in enumerate(bad_contents):
                path = root / f"slide-bad-{index}.ips"
                path.write_bytes(content)
                paths.append(path)
            unreadable = root / "slide-unreadable.ips"
            unreadable.write_text("{}")
            paths.append(unreadable)
            good = root / "slide-good.ips"
            good.write_text('{}\n{"exception": "survived"}')
            paths.append(good)
            original_read = Path.read_text

            def read_text(path, *args, **kwargs):
                if path == unreadable:
                    raise PermissionError("access denied")
                return original_read(path, *args, **kwargs)

            output = io.StringIO()
            with (
                patch.object(Path, "glob", return_value=iter(paths)),
                patch.object(Path, "read_text", read_text),
                redirect_stdout(output),
            ):
                diagnostics.print_crash_reports([root])
            self.assertEqual(output.getvalue().count("could not read crash report"), len(paths) - 1)
            self.assertIn("exception: survived", output.getvalue())

    def test_unreadable_policy_does_not_skip_other_policy(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "application.sb").write_bytes(b"\xff")
            (root / "system.sb").write_text("irrelevant\nprocess-info\n")
            output = io.StringIO()
            with redirect_stdout(output):
                diagnostics.print_system_policies(root)
            self.assertIn("could not read system policy", output.getvalue())
            self.assertIn("2: process-info", output.getvalue())

    def test_missing_start_time_skips_command_with_diagnostic(self):
        output = io.StringIO()
        with patch.object(subprocess, "run") as run, redirect_stdout(output):
            diagnostics.print_sandbox_log(None)
        run.assert_not_called()
        self.assertIn("SANDBOX_STARTED_AT is not set", output.getvalue())

    def test_failed_directory_scan_does_not_skip_next_root(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report = root / "true.ips"
            report.write_text('{}\n{"exception": "survived"}')
            output = io.StringIO()
            with (
                patch.object(Path, "glob", side_effect=[PermissionError("access denied"), [report]]),
                redirect_stdout(output),
            ):
                diagnostics.print_crash_reports([root / "unreadable", root])
            self.assertIn("could not scan crash reports", output.getvalue())
            self.assertIn("exception: survived", output.getvalue())


if __name__ == "__main__":
    unittest.main()
