#!/usr/bin/env python3
"""Run Watch cache, presentation and image regressions on macOS."""
import argparse
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--interop', type=Path, help='Rust-produced manifest.json, clear.json and hash-named assets')
    parser.add_argument('--report', type=Path, default=ROOT / 'output/tests/watch/cache.json')
    parser.add_argument('--image-report', type=Path, help='Image test report; defaults beside the cache report')
    args = parser.parse_args()
    report = args.report.expanduser().resolve()
    report.parent.mkdir(parents=True, exist_ok=True)
    image_report = (args.image_report.expanduser().resolve() if args.image_report
                    else report.with_name(report.stem + '-images.json'))
    sources = ROOT / 'platform/watchos'
    with tempfile.TemporaryDirectory(prefix='loki-watch-cache-tests-') as directory:
        executable = Path(directory) / 'cache-tests'
        subprocess.run(['xcrun', 'swiftc', str(sources / 'Sources/Contract.swift'),
                        str(sources / 'Sources/SnapshotCache.swift'), str(sources / 'Sources/BackgroundDelivery.swift'),
                        str(sources / 'Tests/CacheTests.swift'),
                        '-o', str(executable)], check=True, cwd=ROOT)
        command = [str(executable), str(ROOT / 'src/assets/branding/app-icon.png'), str(report)]
        if args.interop:
            command.extend(['--interop', str(args.interop.expanduser().resolve())])
        subprocess.run(command, check=True, cwd=ROOT)
        presentation = Path(directory) / 'presentation-tests'
        subprocess.run(['xcrun', 'swiftc', str(sources / 'Sources/Contract.swift'),
                        str(sources / 'Sources/AmountText.swift'), str(sources / 'Sources/DisplayNames.swift'), str(sources / 'Tests/PresentationTests.swift'),
                        '-o', str(presentation)], check=True, cwd=ROOT)
        subprocess.run([str(presentation), str(ROOT / 'src/tests/fixtures/display-names.json')], check=True, cwd=ROOT)
        images = Path(directory) / 'image-tests'
        subprocess.run(['xcrun', 'swiftc', str(sources / 'Sources/WatchImageAsset.swift'),
                        str(sources / 'Tests/ImageTests.swift'), '-o', str(images)], check=True, cwd=ROOT)
        subprocess.run([str(images), str(ROOT / 'src/tests/fixtures/assets/hook.png'),
                        str(Path(directory) / 'image-assets'), str(image_report)], check=True, cwd=ROOT)
    print(report)
    print(image_report)


if __name__ == '__main__':
    main()
