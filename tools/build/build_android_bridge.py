#!/usr/bin/env python3
"""Compile the app-owned Android adapter against the unmodified official NSH API."""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "src/native/android/java"
NSH_VERSION = '0.3.0'


def sdk_directory(explicit):
    if explicit:
        return explicit.expanduser().resolve(strict=True)
    value = os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT")
    if value:
        return Path(value).expanduser().resolve(strict=True)
    candidates = [Path.home() / "Library/Android/sdk", Path.home() / "Android/Sdk"]
    if os.environ.get("LOCALAPPDATA"):
        candidates.append(Path(os.environ["LOCALAPPDATA"]) / "Android/Sdk")
    for path in candidates:
        if path.is_dir():
            return path.resolve()
    raise RuntimeError("Android SDK not found; set --sdk or ANDROID_HOME")


def java_compiler(explicit):
    if explicit:
        return explicit.expanduser().resolve(strict=True)
    if os.environ.get("JAVA_HOME"):
        path = Path(os.environ["JAVA_HOME"]) / "bin" / ("javac.exe" if os.name == "nt" else "javac")
        if path.is_file():
            return path.resolve()
    path = shutil.which("javac")
    if path:
        return Path(path).resolve()
    raise RuntimeError("An existing JDK with javac is required; no installation attempted")


def platform_jar(sdk, api):
    candidates = [sdk / 'platforms' / name / 'android.jar' for name in
                  ['android-' + str(api), 'android-' + str(api) + '.0']]
    path = next((candidate for candidate in candidates if candidate.is_file()), None)
    if path is None:
        raise RuntimeError('Installed Android platform jar missing for API ' + str(api))
    return path


def official_aar(explicit=None):
    if explicit:
        path = explicit.expanduser().resolve(strict=True)
        if path.is_relative_to(ROOT):
            raise RuntimeError('NSH AAR must come from the official installed SDK outside the app source tree')
        return path
    cli = shutil.which('nsh')
    if cli:
        path = Path(cli).resolve().parent.parent / 'embed/android/nsh.aar'
        if path.is_file():
            return path
    return None


def locked_android_checksum():
    checksums = set()
    for path in [ROOT / 'Cargo.lock', ROOT / '.nsh/rust/Cargo.lock']:
        if not path.is_file():
            continue
        for package in tomllib.loads(path.read_text()).get('package', []):
            if (package['name'] == 'nsh-platform-android' and package['version'] == NSH_VERSION
                    and package.get('source') == 'registry+https://github.com/rust-lang/crates.io-index'):
                if package.get('checksum'):
                    checksums.add(package['checksum'])
    if len(checksums) != 1:
        raise RuntimeError('Official NSH Android crate checksum is missing or inconsistent in Cargo.lock; generate the standard NSH runner first')
    return checksums.pop()


def official_sources():
    """Compare cached Java against a release archive authenticated by the Cargo lockfile."""
    checksum = locked_android_checksum()
    cargo = Path(os.environ.get('CARGO_HOME', Path.home() / '.cargo'))
    for source in sorted((cargo / 'registry/src').glob('index.crates.io-*/nsh-platform-android-' + NSH_VERSION)):
        sources = sorted((source / 'java').rglob('*.java'))
        if not sources:
            continue
        archive = source.parents[2] / 'cache' / source.parent.name / (source.name + '.crate')
        legacy = source / '.cargo-checksum.json'
        if archive.is_file():
            if hashlib.sha256(archive.read_bytes()).hexdigest() != checksum:
                raise RuntimeError('Official Cargo archive checksum differs: ' + source.name)
            with tarfile.open(archive, 'r:gz') as release:
                prefix = source.name + '/'
                expected = {member.name[len(prefix):]: hashlib.sha256(release.extractfile(member).read()).hexdigest()
                            for member in release.getmembers() if member.isfile()
                            and member.name.startswith(prefix + 'java/') and member.name.endswith('.java')}
        elif legacy.is_file():
            checksums = json.loads(legacy.read_text())
            if checksums.get('package') != checksum:
                raise RuntimeError('Official Cargo package checksum differs: ' + source.name)
            expected = {name: digest for name, digest in checksums.get('files', {}).items()
                        if name.startswith('java/') and name.endswith('.java')}
        else:
            continue
        if {path.relative_to(source).as_posix() for path in sources} != set(expected):
            raise RuntimeError('Official Cargo Java source inventory differs: ' + source.name)
        for path in sources:
            relative = path.relative_to(source).as_posix()
            if hashlib.sha256(path.read_bytes()).hexdigest() != expected[relative]:
                raise RuntimeError('Official Cargo Java source checksum differs: ' + relative)
        return source, sources, checksum
    raise RuntimeError('Official NSH AAR or cached crates.io NSH ' + NSH_VERSION + ' Java sources are required')


def jar_entries(data):
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        return {entry.filename: archive.read(entry) for entry in archive.infolist() if not entry.is_dir()}


def write_jar(path, entries):
    with zipfile.ZipFile(path, 'w', zipfile.ZIP_DEFLATED) as archive:
        for name, data in sorted(entries.items()):
            info = zipfile.ZipInfo(name, (2020, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, data)


def build(args):
    sdk = sdk_directory(args.sdk)
    android = platform_jar(sdk, args.compile_sdk)
    javac = java_compiler(args.javac)
    version = subprocess.check_output([str(javac), "-version"], stderr=subprocess.STDOUT, text=True).strip()
    out = args.out.expanduser().resolve()
    out.mkdir(parents=True, exist_ok=True)
    sources = sorted(SOURCE.rglob("*.java"))
    if not sources:
        raise RuntimeError("No app-owned Android Java adapter sources found")
    with tempfile.TemporaryDirectory(prefix="android-bridge-", dir=out.parent) as temporary:
        temporary = Path(temporary)
        classes = Path(temporary) / "classes"
        classes.mkdir()
        dependencies = [str(android)]
        original = {}
        aar = official_aar(getattr(args, 'aar', None))
        sdk_sources = []
        if aar:
            with zipfile.ZipFile(aar) as archive:
                sdk_jar = archive.read('classes.jar')
            original = jar_entries(sdk_jar)
            if 'com/longbridge/nsh/NshView.class' not in original:
                raise RuntimeError('Official NSH AAR has no NshView API')
            dependency = temporary / 'official-nsh.jar'
            dependency.write_bytes(sdk_jar)
            dependencies.append(str(dependency))
            sdk_input = {'kind': 'official-installed-aar', 'path': str(aar),
                         'aarSha256': hashlib.sha256(aar.read_bytes()).hexdigest(),
                         'classesSha256': hashlib.sha256(sdk_jar).hexdigest(), 'modified': False}
        else:
            source, sdk_sources, checksum = official_sources()
            sdk_input = {'kind': 'official-crates-io-java', 'path': str(source), 'version': NSH_VERSION,
                         'crateChecksum': checksum, 'modified': False,
                         'sourceSha256': {p.relative_to(source).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                                          for p in sdk_sources}}
        command = [str(javac), '--release', '8', '-classpath', os.pathsep.join(dependencies), '-d', str(classes)]
        subprocess.run(command + [str(path) for path in sdk_sources + sources], check=True)
        app_entries = {p.relative_to(classes).as_posix(): p.read_bytes() for p in sorted(classes.rglob('*.class'))}
        if not app_entries:
            raise RuntimeError("javac produced no classes")
        if original.keys() & app_entries.keys():
            raise RuntimeError('App Java adapter would replace an official NSH class')
        entries = {**original, **app_entries}
        write_jar(out / 'classes.jar', entries)
    report = {
        "source": "src/native/android/java", "compileSdk": args.compile_sdk, 'officialSdk': sdk_input,
        "javac": str(javac), "javacVersion": version, "androidJar": str(android),
        "javaSources": len(sources), "classes": len([name for name in entries if name.endswith('.class')]),
        'viewApi': 'public NshView embedding API',
        'vmLimits': {'heapMiB': 256, 'stackMiB': 4, 'loadMs': 10000, 'callMs': 2000, 'maxNodes': 200000},
        "sourceSha256": {p.relative_to(SOURCE).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources},
        "jarSha256": hashlib.sha256((out / "classes.jar").read_bytes()).hexdigest(),
    }
    (out / "java-build.json").write_text(json.dumps(report, indent=2) + "\n")
    print("Built " + str(out / "classes.jar") + " with unmodified official NSH and " + str(len(sources)) + " app Java sources (" + version + ")")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=ROOT / "output/build/android-bridge")
    parser.add_argument("--sdk", type=Path)
    parser.add_argument("--compile-sdk", type=int, default=37)
    parser.add_argument("--javac", type=Path)
    parser.add_argument('--aar', type=Path, help='Official installed NSH release AAR (default: beside the nsh executable)')
    args = parser.parse_args()
    if args.compile_sdk < 28:
        parser.error("The Android backend needs API 28 or newer")
    build(args)


if __name__ == "__main__":
    main()
