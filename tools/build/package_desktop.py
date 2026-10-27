#!/usr/bin/env python3
"""Package on the target OS and reject executables for another platform.

NSH 0.2.0 can report success for --target windows/linux on macOS while
packaging a Mach-O runner. This entry point prevents that false success.
"""
import argparse
import json
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def executable_platform(path):
    with Path(path).open("rb") as source:
        magic = source.read(64)
        if magic[:4] == b"\x7fELF":
            return "linux"
        if magic[:4] in (b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xfe\xed\xfa\xcf", b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca", b"\xca\xfe\xba\xbf", b"\xbf\xba\xfe\xca"):
            return "macos"
        if len(magic) >= 64 and magic[:2] == b"MZ":
            pe = int.from_bytes(magic[60:64], "little")
            source.seek(pe)
            if source.read(4) == b"PE\0\0":
                return "windows"
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=("macos", "windows", "linux"), required=True)
    parser.add_argument("--out", type=Path, default=ROOT / "output/packages")
    args = parser.parse_args()
    host = {"Darwin": "macos", "Windows": "windows", "Linux": "linux"}.get(platform.system())
    if host != args.target:
        raise SystemExit(f"Build {args.target} on a {args.target} host. Current host: {host}. No package was produced.")
    cli = shutil.which("nsh")
    if not cli:
        raise SystemExit("nsh is not installed; no tool installation was attempted")
    output = args.out.resolve()
    destination = output / args.target
    if destination.exists():
        raise SystemExit(f"Refusing to replace existing output: {destination}")
    output.mkdir(parents=True, exist_ok=True)
    metadata = json.loads((ROOT / "nsh.json").read_text())
    with tempfile.TemporaryDirectory(prefix="loki-package-", dir=output) as stage:
        stage = Path(stage)
        subprocess.run([cli, "package", "--target", args.target, "--out", str(stage)], cwd=ROOT, check=True)
        name = metadata["name"]
        if args.target == "macos":
            executables = list((stage / f"{name}.app/Contents/MacOS").glob("*"))
        elif args.target == "windows":
            executables = [stage / name / f"{name}.exe"]
        else:
            executables = [stage / name / "App"]
        if not executables or any(not path.is_file() or executable_platform(path) != args.target for path in executables):
            raise SystemExit("Runner executable format does not match target; package rejected")
        (stage / "platform-verification.json").write_text(json.dumps({"target": args.target, "buildHost": host, "nshExecutable": str(Path(cli).resolve()), "runnerFormatVerified": True, "runtimeVerified": False, "note": "Executable format is a packaging check, not an application runtime test."}, indent=2) + "\n")
        # Only validated packages leave the staging directory. Avoid replacing
        # any earlier deliverable, even when two builds finish concurrently.
        if destination.exists():
            raise SystemExit(f"Output already exists: {destination}")
        stage.rename(destination)
    print(f"Validated native package: {destination}")


if __name__ == "__main__":
    main()
