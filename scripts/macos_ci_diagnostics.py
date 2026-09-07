"""Print native CI sandbox denials and crash reasons without dumping full reports."""

import json
import os
from pathlib import Path
import subprocess

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
