#!/usr/bin/env python3
"""Run current project tests with the existing Rust, NSH and platform toolchains."""
import argparse
import os
from pathlib import Path
import shlex
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def commands(suite, output):
    python = sys.executable
    if suite == 'unit':
        result = [
            ['cargo', 'fmt', '--all', '--check'],
            ['cargo', 'clippy', '--workspace', '--all-targets', '--all-features', '--locked', '--', '-D', 'warnings'],
            ['cargo', 'test', '--workspace', '--all-targets', '--locked'],
            [python, '-m', 'unittest', 'discover', '-s', 'src/tests/packaging', '-p', 'test_*.py'],
        ]
        if os.name == 'posix':
            result.append([python, 'src/native/credentials/test_linux.py'])
        if sys.platform == 'darwin':
            result.append([python, 'platform/ios/Tests/run_tests.py', '--output', str(output / 'widget-host')])
        result.append(['nsh', 'check'])
        for name in ['app.test.tsx', 'mobile-app.test.tsx', 'image-playback.test.ts']:
            result.append(['nsh', 'test', 'src/tests/desktop/' + name, '--junit',
                           str(output / (name + '.xml'))])
        return result
    if suite == 'android':
        return [[python, 'src/tests/android/test_entry.py']]
    if suite == 'watch':
        return [[python, 'platform/watchos/Tests/run_tests.py', '--report', str(output / 'cache.json')]]
    if suite == 'ios':
        return [[python, 'platform/ios/Tests/run_tests.py', '--output', str(output)]]
    if suite == 'visual':
        return [[python, 'src/tests/visual/run.py', '--suite', 'all', '--output', str(output)]]
    return [[python, 'src/tests/integration/run.py', '--output', str(output)]]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--suite', choices=['unit', 'android', 'ios', 'watch', 'visual', 'integration'], default='unit')
    parser.add_argument('--output', type=Path, default=ROOT / 'output/tests', help='Results root, grouped by suite')
    args = parser.parse_args()
    if args.suite in {'ios', 'watch', 'visual', 'integration'} and sys.platform != 'darwin':
        parser.error('This suite requires the existing macOS/Xcode tools')
    output = args.output.expanduser().resolve() / args.suite
    output.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    environment['PYTHONDONTWRITEBYTECODE'] = '1'
    for command in commands(args.suite, output):
        print('+ ' + shlex.join(command), flush=True)
        subprocess.run(command, cwd=ROOT, env=environment, check=True)


if __name__ == '__main__':
    try:
        main()
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.returncode) from None
