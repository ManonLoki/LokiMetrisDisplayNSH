#!/usr/bin/env python3
"""Test the Widget host's cancellation lifecycle using the existing Swift compiler."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[3]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'output/tests/ios')
    args = parser.parse_args()
    compiler = shutil.which('swiftc')
    if not compiler:
        parser.error('The existing Swift compiler is required; no installation was attempted')
    output = args.output.expanduser().resolve()
    output.mkdir(parents=True, exist_ok=True)
    sources = [ROOT / 'platform/ios/bridge/WidgetHostBridge.swift',
               ROOT / 'platform/ios/shared/WidgetRefreshCommitGate.swift',
               ROOT / 'platform/ios/Tests/WidgetBackgroundExecutionTests.swift']
    executable = output / 'widget-background-execution-tests'
    subprocess.run([compiler, '-parse-as-library', *map(str, sources), '-o', str(executable)],
                   cwd=ROOT, check=True)
    subprocess.run([str(executable)], cwd=ROOT, check=True)
    report = {
        'passed': True,
        'testCount': 5,
        'scope': 'Foundation-only background execution cancellation and completion lifecycle; '
                 'does not prove OS scheduling, WidgetKit rendering or device refresh',
        'sourceHashes': {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
                         for path in sources},
    }
    (output / 'widget-background-execution.json').write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
