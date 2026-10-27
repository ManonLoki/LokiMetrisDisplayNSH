#!/usr/bin/env python3
"""Run the normal macOS app against a real TCP two-frame GIF fixture.

Saves six native screenshots and checks their pixels for both fixture colors.
Transfer, decoding and changing rendered frames must all pass.
"""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
CLIENT = "f0f0f0f0-f0f0-40f0-80f0-f0f0f0f0f0f0"


def stop(process):
    if process is None or process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)


def verify_frames(output):
    frames = [output / f"frame-{index}.png" for index in range(6)]
    result = subprocess.run(["swift", str(Path(__file__).resolve().with_name("verify-animation-frames.swift")),
                             *map(str, frames)], cwd=ROOT, capture_output=True, text=True, timeout=90)
    (output / "frame-check.log").write_text(result.stdout + result.stderr)
    if result.returncode:
        raise RuntimeError("Rendered GIF frames did not change; inspect frame-check.log")
    report = json.loads(result.stdout)
    (output / "frames.json").write_text(json.dumps(report, indent=2) + "\n")
    if not report.get("changingRenderedFrames"):
        raise RuntimeError("Native screenshots did not contain both fixture colors")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "output/tests/integration")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    scenario = (ROOT / "src/tests/scenarios/DesktopAnimation.scenario.tsx").read_text()
    scenario = scenario.replace('"__APP_ENTRY__"', json.dumps(str(ROOT / "src/tests/desktop/NativeApp.tsx")))
    scenario = scenario.replace('"__OUTPUT_DIRECTORY__"', json.dumps(str(output)))
    target = ROOT / ".nsh/test/DesktopAnimation.test.tsx"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(scenario)
    fixture = app = None
    try:
        with (output / "fixture.log").open("w") as fixture_log:
            fixture = subprocess.Popen(["cargo", "run", "--example", "protocol_host", "--", "--id", CLIENT,
                                        "--name", "Loki 动图验证台", "--asset",
                                        "src/tests/fixtures/assets/two-frame.gif"],
                                       cwd=ROOT, stdin=subprocess.PIPE, stdout=fixture_log,
                                       stderr=subprocess.STDOUT, text=True)
            deadline = time.monotonic() + 60
            while "FIXTURE_READY" not in (output / "fixture.log").read_text():
                if fixture.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError("Animation fixture did not become ready; inspect fixture.log")
                time.sleep(0.1)
            env = os.environ.copy()
            env["LOKI_TEST_DATA_DIR"] = str(output / "native-data")
            with (output / "app.log").open("w") as log:
                app = subprocess.Popen(["nsh", "test", str(target), "--target", "macos", "--verbose",
                                        "--junit", str(output / "results.xml")], cwd=ROOT, env=env,
                                       stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
                assert app.stdout is not None
                for line in app.stdout:
                    code = re.search(r"ANIMATION_PAIR_CODE=([0-9]{4})", line)
                    if code:
                        assert fixture.stdin is not None
                        fixture.stdin.write(code[1] + "\n")
                        fixture.stdin.flush()
                    else:
                        log.write(line)
                        log.flush()
                        print(line, end="", flush=True)
                result = app.wait()
                if result != 0:
                    raise SystemExit(result)
        verify_frames(output)
    finally:
        stop(app)
        stop(fixture)
        target.unlink(missing_ok=True)
    print("Authenticated GIF transfer, decoding and changing native red/blue frames passed.")


if __name__ == "__main__":
    main()
