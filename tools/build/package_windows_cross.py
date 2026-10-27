#!/usr/bin/env python3
"""Build the Windows MSVC runner on macOS, preserving an explicit runtime gap.

Requires existing nsh, cargo-xwin and Xcode installations. NSH 0.2.0's
cross-package runner is replaced by the verified PE built for Windows;
its generated bundle and metadata remain the normal NSH package layout.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import struct
import subprocess
import tempfile
import zipfile

from package_desktop import ROOT, executable_platform

PACKAGE_README = """诡秘神谕 · Windows 展示端

解压整个目录，双击 {name}.exe。请保留旁边的 nsh 文件夹。
运行环境需要 Microsoft VC++ x64 Runtime（提供 VCRUNTIME140.dll 和
VCRUNTIME140_1.dll），以及 Microsoft Edge WebView2 Runtime。

电脑采集客户端需要包含桌面终端平台支持的 0.10.0 源码版本。
在展示端设置页选择电脑，再在采集客户端中输入展示端的四位配对码。

本包为 Windows x86-64 MSVC 交叉构建，已核对真实 PE 与应用图标。
Windows 实机启动、凭据存储和配对验收仍为待办。
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=ROOT / "output/packages")
    args = parser.parse_args()
    if platform.system() != "Darwin":
        raise SystemExit("This entry point uses the existing macOS Xcode/libclang installation")
    for command in ("nsh", "cargo", "cargo-xwin", "xcode-select", "sips", "x86_64-w64-mingw32-windres"):
        if not shutil.which(command):
            raise SystemExit(f"Required tool is missing: {command}; no installation was attempted")
    output = args.out.resolve()
    destination = output / "windows-cross"
    if destination.exists():
        raise SystemExit(f"Refusing to replace existing output: {destination}")
    output.mkdir(parents=True, exist_ok=True)
    app = json.loads((ROOT / "nsh.json").read_text())
    name = app["name"]
    with tempfile.TemporaryDirectory(prefix="loki-windows-cross-", dir=output) as temporary:
        stage = Path(temporary)
        subprocess.run(["nsh", "package", "--target", "windows", "--out", str(stage)], cwd=ROOT, check=True)
        manifest = ROOT / ".nsh/rust/Cargo.toml"
        runner_manifest = ROOT / ".nsh/rust/windows-app/Cargo.toml"
        # Read the generated package name rather than depending on a checkout's
        # directory name. Python 3.11+ is already required by this entry point.
        import tomllib
        runner_name = tomllib.loads(runner_manifest.read_text())["package"]["name"]
        env = os.environ.copy()
        developer = subprocess.check_output(["xcode-select", "-p"], text=True).strip()
        clang = Path(developer) / "Toolchains/XcodeDefault.xctoolchain/usr/lib"
        if not (clang / "libclang.dylib").is_file():
            raise SystemExit(f"Xcode libclang was not found at {clang}")
        env["LIBCLANG_PATH"] = str(clang)
        resources = stage / "resources"
        resources.mkdir()
        icon = (ROOT / app["icon"]).resolve()
        subprocess.run(["sips", "-z", "256", "256", str(icon), "--out", str(resources / "icon.png")], check=True)
        png = (resources / "icon.png").read_bytes()
        if png[:8] != b"\x89PNG\r\n\x1a\n" or struct.unpack(">II", png[16:24]) != (256, 256):
            raise SystemExit("Expected a 256-pixel PNG icon")
        # Modern Windows ICO supports a PNG payload. Keep the source artwork,
        # using standard-library container encoding and the installed linker.
        (resources / "icon.ico").write_bytes(struct.pack("<HHH", 0, 1, 1) +
                                              struct.pack("<BBBBHHII", 0, 0, 0, 0, 1, 32, len(png), 22) + png)
        (resources / "icon.rc").write_text('1 ICON "icon.ico"\n')
        resource_object = resources / "icon.o"
        subprocess.run(["x86_64-w64-mingw32-windres", "-i", "icon.rc", "-O", "coff", "-o", str(resource_object)],
                       cwd=resources, check=True)
        subprocess.run(["cargo", "xwin", "rustc", "--manifest-path", str(manifest), "-p", runner_name,
                        "--target", "x86_64-pc-windows-msvc", "--release", "--",
                        "-C", f"link-arg={resource_object}"], cwd=ROOT, env=env, check=True)
        cargo = json.loads(subprocess.check_output(["cargo", "metadata", "--manifest-path", str(manifest),
                                                  "--no-deps", "--format-version", "1"], cwd=ROOT, text=True))
        executable = Path(cargo["target_directory"]) / "x86_64-pc-windows-msvc/release" / (runner_name + ".exe")
        if executable_platform(executable) != "windows":
            raise SystemExit("Cross-built runner is not PE; package rejected")
        with executable.open("rb") as source:
            source.seek(60)
            offset = int.from_bytes(source.read(4), "little")
            source.seek(offset + 4)
            if struct.unpack("<H", source.read(2))[0] != 0x8664:
                raise SystemExit("Expected an x86-64 Windows PE runner")
        package = stage / name
        shutil.copy2(executable, package / f"{name}.exe")
        (package / "README.txt").write_text(PACKAGE_README.format(name=name), encoding="utf-8")
        archive = stage / f"{name}.zip"
        # The initial CLI archive contains its untrusted host runner. Rebuild
        # it only after the actual Windows executable has been substituted.
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
            for path in sorted(package.rglob("*")):
                if path.is_file():
                    bundle.write(path, path.relative_to(stage))
        shutil.rmtree(resources)
        evidence = {
            "target": "windows", "architecture": "x86_64", "buildHost": "macos",
            "targetTriple": "x86_64-pc-windows-msvc", "crossCompiled": True,
            "runnerFormatVerified": True, "runtimeVerified": False,
            "runnerSha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
            "iconSource": app["icon"], "iconResourceEmbedded": True,
            "note": "Windows launch, Credential Manager, WebView2 and production pairing require Windows runtime acceptance."
        }
        (stage / "platform-verification.json").write_text(json.dumps(evidence, indent=2) + "\n")
        if destination.exists():
            raise SystemExit(f"Output already exists: {destination}")
        stage.rename(destination)
    print(f"Cross-built Windows PE package (runtime pending): {destination}")


if __name__ == "__main__":
    main()
