#!/usr/bin/env python3
"""Compare fresh-process CLI captures, not browser page-load benchmarks.

Requires Pillow and psutil. Generate fixtures with examples/renderer_fixtures.rs.
Results retain commands, logs, PNGs, raw timings, and sampled process-tree RSS.
"""
import argparse
import concurrent.futures
import json
import os
from pathlib import Path
import platform
import shutil
import signal
import statistics
import subprocess
import threading
import time

from PIL import Image, ImageChops, ImageStat
import psutil


def command(engine, executable, html, png, profile, wait):
    if engine == "obscura":
        args = [executable, "fetch", html.as_uri(), "--screenshot", str(png)]
        return args if wait == "default" else args + ["--wait", wait]
    # Match src/render/browser.rs, including its sandbox and virtual-time budget.
    return [executable, "--headless=new", "--hide-scrollbars",
            "--disable-background-networking", "--disable-component-update",
            "--disable-default-apps", "--disable-domain-reliability",
            "--disable-features=Translate,MediaRouter,OptimizationHints,AutofillServerCommunication",
            "--disable-sync", "--metrics-recording-only", "--no-first-run", "--no-pings",
            "--password-store=basic", "--use-mock-keychain",
            f"--user-data-dir={profile}", "--window-size=1280,720",
            "--force-device-scale-factor=1", "--virtual-time-budget=60000",
            f"--screenshot={png}", html.as_uri()]


def capture(engine, executable, html, output, wait, active, lock):
    output.mkdir()
    png = output / "capture.png"
    profile = output / "profile"
    args = command(engine, executable, html, png, profile, wait)
    with (output / "capture.log").open("w") as log:
        start = time.perf_counter()
        process = subprocess.Popen(args, stdout=log, stderr=log, start_new_session=True)
        with lock:
            active.add(process.pid)
        expired = threading.Event()

        def expire():
            expired.set()
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

        # Popen.wait(timeout=...) polls with sleeps up to 50 ms on POSIX,
        # which distorts captures this short. A timer keeps wait() blocking.
        timer = threading.Timer(60, expire)
        timer.start()
        try:
            code = process.wait()
        finally:
            elapsed = time.perf_counter() - start
            timer.cancel()
            timer.join()
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
            with lock:
                active.discard(process.pid)
    result = {"fixture": html.name, "seconds": elapsed, "exit": code,
              "timeout": expired.is_set(), "command": args, "png": str(png)}
    try:
        with Image.open(png) as image:
            image.load()
            result["size"] = list(image.size)
            result["valid"] = image.format == "PNG" and image.size == (1280, 720)
            result["extrema"] = image.convert("RGB").getextrema()
    except (OSError, ValueError) as error:
        result.update(valid=False, error=str(error))
    shutil.rmtree(profile, ignore_errors=True)
    return result


def batch(engine, executable, fixtures, output, concurrency, wait):
    output.mkdir()
    active, lock, stop = set(), threading.Lock(), threading.Event()
    samples = []

    def sample():
        while not stop.is_set():
            with lock:
                roots = list(active)
            processes = {}
            for pid in roots:
                try:
                    root = psutil.Process(pid)
                    for process in [root] + root.children(recursive=True):
                        processes[process.pid] = process
                except psutil.Error:
                    pass
            rss = 0
            for process in processes.values():
                try:
                    rss += process.memory_info().rss
                except psutil.Error:
                    pass
            samples.append(rss)
            # The initial Obscura capture took ~64 ms; 5 ms samples resolve it.
            stop.wait(0.005)

    monitor = threading.Thread(target=sample)
    monitor.start()
    start = time.perf_counter()
    try:
        with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as pool:
            futures = [pool.submit(capture, engine, executable, html,
                                   output / f"{i:02d}", wait, active, lock)
                       for i, html in enumerate(fixtures)]
            captures = [future.result() for future in futures]
        elapsed = time.perf_counter() - start
    finally:
        stop.set()
        monitor.join()
    return {"engine": engine, "concurrency": concurrency, "seconds": elapsed,
            "peak_tree_rss_mib": max(samples, default=0) / 1024**2,
            "memory_samples": len(samples), "captures": captures}


def compare(left, right, output):
    with Image.open(left) as a, Image.open(right) as b:
        a, b = a.convert("RGB"), b.convert("RGB")
        if a.size != b.size:
            return {"size_mismatch": [a.size, b.size]}
        difference = ImageChops.difference(a, b)
        difference.save(output)
        # Exact disagreement is descriptive, not a fidelity acceptance threshold.
        red, green, blue = difference.split()
        histogram = ImageChops.lighter(ImageChops.lighter(red, green), blue).histogram()
        changed = a.width * a.height - histogram[0]
        return {"mean_absolute_channel_error": statistics.mean(ImageStat.Stat(difference).mean),
                "changed_pixel_percent": changed * 100 / (a.width * a.height)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--chromium", required=True)
    parser.add_argument("--obscura", required=True)
    parser.add_argument("--fixtures", type=Path, required=True)
    parser.add_argument("--fixture-prefix", default="",
                        help="Select manifest entries by HTML filename prefix")
    parser.add_argument("--output", type=Path, required=True)
    # Five paired repetitions are an exploratory sample, not a statistical claim.
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--concurrency", type=int, nargs="+", default=[1, 4])
    parser.add_argument("--obscura-wait", default="0")
    args = parser.parse_args()
    if args.repeats < 1 or any(n < 1 or n > 8 for n in args.concurrency):
        parser.error("repeats must be positive; concurrency must be 1..8")
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    fixtures = [(args.fixtures / row["html"]).resolve()
                for row in json.loads((args.fixtures / "manifest.json").read_text())
                if row["html"].startswith(args.fixture_prefix)]
    if not fixtures:
        parser.error(f"no fixtures match prefix {args.fixture_prefix!r}")
    executables = {"chromium": str(Path(args.chromium).resolve()),
                   "obscura": str(Path(args.obscura).resolve())}
    report = {"platform": platform.platform(), "cpu": platform.processor(),
              "logical_cpus": os.cpu_count(), "obscura_wait": args.obscura_wait,
              "versions": {name: subprocess.check_output([exe, "--version"], text=True).strip()
                           for name, exe in executables.items()},
              "fixtures": [str(p) for p in fixtures], "batches": []}
    # Warm filesystem/font caches equally. All measured captures still start a process.
    for engine, executable in executables.items():
        warmup = batch(engine, executable, fixtures, args.output / f"warmup-{engine}",
                       1, args.obscura_wait)
        if any(not c["valid"] or c["exit"] != 0 for c in warmup["captures"]):
            report["failed_warmup"] = warmup
            (args.output / "results.json").write_text(json.dumps(report, indent=2))
            raise SystemExit(f"{engine} warmup failed; see results.json")
    for concurrency in args.concurrency:
        for repetition in range(args.repeats):
            # Alternate engine order to reduce fixed-order bias.
            order = list(executables) if repetition % 2 == 0 else list(reversed(executables))
            for engine in order:
                result = batch(engine, executables[engine], fixtures,
                               args.output / f"c{concurrency}-r{repetition}-{engine}",
                               concurrency, args.obscura_wait)
                result["repetition"] = repetition
                report["batches"].append(result)
                (args.output / "results.json").write_text(json.dumps(report, indent=2))
    differences = {}
    for i, fixture in enumerate(fixtures):
        differences[fixture.name] = compare(
            args.output / f"warmup-chromium/{i:02d}/capture.png",
            args.output / f"warmup-obscura/{i:02d}/capture.png",
            args.output / f"diff-{fixture.stem}.png")
    report["differences"] = differences
    report["summary"] = []
    for concurrency in args.concurrency:
        for engine in executables:
            batches = [b for b in report["batches"]
                       if b["engine"] == engine and b["concurrency"] == concurrency]
            captures = [c for b in batches for c in b["captures"]]
            report["summary"].append({"engine": engine, "concurrency": concurrency,
                "captures": len(captures),
                "failures": sum(not c["valid"] or c["exit"] != 0 for c in captures),
                "median_capture_seconds": statistics.median(c["seconds"] for c in captures),
                "median_batch_seconds": statistics.median(b["seconds"] for b in batches),
                "median_peak_tree_rss_mib": statistics.median(b["peak_tree_rss_mib"] for b in batches)})
    (args.output / "results.json").write_text(json.dumps(report, indent=2))
    print(json.dumps({"summary": report["summary"], "differences": differences}, indent=2))


if __name__ == "__main__":
    main()
