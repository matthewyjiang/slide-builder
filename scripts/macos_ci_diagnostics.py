"""Print native CI sandbox denials and crash reasons without dumping full reports."""

import json
import os
from pathlib import Path
import subprocess


def print_system_policies(root):
    for name in ("application.sb", "system.sb"):
        path = root / name
        try:
            if not path.is_file():
                print("system policy not present:", path)
                continue
            print("relevant system policy clauses:", path)
            for number, line in enumerate(path.read_text().splitlines(), start=1):
                if any(term in line for term in ("procargs", "kern.proc", "process-info")):
                    print(f"{number}: {line}")
        except (OSError, UnicodeError) as error:
            print(f"could not read system policy {path}: {error}")


def print_sandbox_log(started_at):
    if not started_at:
        print("could not collect sandbox log: SANDBOX_STARTED_AT is not set")
        return
    try:
        subprocess.run(
            [
                "sudo", "log", "show", "--info", "--debug", "--style", "compact",
                "--start", started_at,
                "--predicate", 'process == "sandboxd" OR subsystem == "com.apple.sandbox.reporting"',
            ],
            check=True,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        print(f"could not collect sandbox log: {error}")


def print_crash_reports(roots):
    for root in roots:
        try:
            for path in root.glob("*.ips"):
                if not path.name.startswith(("slide", "true", "sandbox-exec", "Google Chrome")):
                    continue
                print(path)
                try:
                    text = path.read_text()
                    _, end = json.JSONDecoder().raw_decode(text)
                    report = json.loads(text[end:])
                    if not isinstance(report, dict):
                        raise ValueError("report must be a JSON object")
                    threads = report.get("threads", [])
                    if not isinstance(threads, list) or any(
                        not isinstance(thread, dict) for thread in threads
                    ):
                        raise ValueError("threads must be a list of JSON objects")
                    print("exception:", report.get("exception"))
                    print("termination:", report.get("termination"))
                    print("application information:", report.get("asi"))
                    for thread in threads:
                        if thread.get("triggered"):
                            print("faulting frames:", thread.get("frames"))
                except (OSError, UnicodeError, ValueError) as error:
                    print(f"could not read crash report {path}: {error}")
        except OSError as error:
            print(f"could not scan crash reports in {root}: {error}")


def main():
    print_system_policies(Path("/System/Library/Sandbox/Profiles"))
    print_sandbox_log(os.environ.get("SANDBOX_STARTED_AT"))
    print_crash_reports([
        Path.home() / "Library/Logs/DiagnosticReports",
        Path("/Library/Logs/DiagnosticReports"),
    ])


if __name__ == "__main__":
    main()
