"""Print native CI sandbox denials and crash reasons without dumping full reports."""

import json
import os
from pathlib import Path
import subprocess

for name in ("application.sb", "system.sb"):
    path = Path("/System/Library/Sandbox/Profiles") / name
    if not path.is_file():
        print("system policy not present:", path)
        continue
    print("relevant system policy clauses:", path)
    for number, line in enumerate(path.read_text().splitlines(), start=1):
        if any(term in line for term in ("procargs", "kern.proc", "process-info")):
            print(f"{number}: {line}")

subprocess.run(
    [
        "sudo", "log", "show", "--info", "--debug", "--style", "compact",
        "--start", os.environ["SANDBOX_STARTED_AT"],
        "--predicate", 'process == "sandboxd" OR subsystem == "com.apple.sandbox.reporting"',
    ],
    check=True,
)

for root in [
    Path.home() / "Library/Logs/DiagnosticReports",
    Path("/Library/Logs/DiagnosticReports"),
]:
    for path in root.glob("*.ips"):
        if not path.name.startswith(("slide", "true", "sandbox-exec", "Google Chrome")):
            continue
        print(path)
        text = path.read_text()
        _, end = json.JSONDecoder().raw_decode(text)
        report = json.loads(text[end:])
        print("exception:", report.get("exception"))
        print("termination:", report.get("termination"))
        print("application information:", report.get("asi"))
        for thread in report.get("threads", []):
            if thread.get("triggered"):
                print("faulting frames:", thread.get("frames"))
