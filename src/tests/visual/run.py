#!/usr/bin/env python3
"""Run isolated macOS screenshot regressions for card motion and GIF playback."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
IMAGE_COUNTS = {'moving': 8, 'inactive': 5, 'resumed': 8, 'independent': 8, 'background': 5,
                'foreground-resumed': 8, 'reduced': 5, 'finite-running': 8, 'finite-ended': 5}
SUITES = {
    'cards': ('CardAppearance', 'verify-card-frames.swift'),
    'images': ('ImagePlayback', 'verify-image-playback.swift'),
}


def finite_fixture(output):
    source = ROOT / 'src/tests/fixtures/assets/two-frame.gif'
    original = source.read_bytes()
    tag = b'\x21\xff\x0bNETSCAPE2.0\x03\x01'
    if not original.startswith((b'GIF87a', b'GIF89a')) or original.count(tag) != 1:
        raise RuntimeError('Expected one Netscape repeat extension in the real fixture')
    offset = original.index(tag) + len(tag)
    if original[offset:offset + 2] != b'\0\0' or original[offset + 2] != 0:
        raise RuntimeError('The real fixture must have an infinite loop extension')
    finite = original[:offset] + b'\x01\x00' + original[offset + 2:]
    target = output / 'finite-two-frame.gif'
    target.write_bytes(finite)
    provenance = {
        'source': str(source), 'sourceSha256': hashlib.sha256(original).hexdigest(),
        'generated': str(target), 'generatedSha256': hashlib.sha256(finite).hexdigest(),
        'repeatCount': 1, 'repeatByteOffset': offset,
        'changedByteOffsets': [i for i, (a, b) in enumerate(zip(original, finite)) if a != b],
        'sourceDimensions': {'width': int.from_bytes(original[6:8], 'little'),
                             'height': int.from_bytes(original[8:10], 'little')},
        'renderedImagePoints': {'width': 200, 'height': 200},
    }
    (output / 'fixture.json').write_text(json.dumps(provenance, indent=2) + '\n')
    return source, target, provenance


def scenario_source(suite, output):
    name, _ = SUITES[suite]
    source = (ROOT / f'src/tests/scenarios/{name}.scenario.tsx').read_text()
    replacements = {'__APP_ENTRY__': str(ROOT / f'src/tests/desktop/{name}App.tsx'),
                    '__OUTPUT_DIRECTORY__': str(output)}
    provenance = None
    if suite == 'images':
        infinite, finite, provenance = finite_fixture(output)
        replacements.update({'__INFINITE_SOURCE__': infinite.as_uri(), '__FINITE_SOURCE__': finite.as_uri()})
    for key, value in replacements.items():
        source = source.replace(json.dumps(key), json.dumps(value))
    return source, provenance


def capture_manifest(suite, log, provenance):
    captures, visuals = [], []
    for line in log.splitlines():
        if suite == 'cards':
            match = re.search(r'CARD_APPEARANCE_(CAPTURE|VISUAL)=(\{.*\})', line)
            if match:
                (captures if match[1] == 'CAPTURE' else visuals).append(json.loads(match[2]))
        else:
            match = re.search(r'IMAGE_PLAYBACK_CAPTURE=(\{.*\})', line)
            if match:
                captures.append(json.loads(match[1]))
    if suite == 'cards':
        if len(captures) != 20 or len(visuals) != 4:
            raise RuntimeError(f'Incomplete native capture: {len(captures)} motion frames and {len(visuals)} layouts')
        return {'captures': captures, 'visuals': visuals}
    actual = {group: sum(item['group'] == group for item in captures) for group in IMAGE_COUNTS}
    if actual != IMAGE_COUNTS or len(captures) != sum(IMAGE_COUNTS.values()):
        raise RuntimeError(f'Incomplete native captures: expected {IMAGE_COUNTS}, got {actual}')
    return {'captures': captures, 'fixture': provenance}


def run_suite(suite, output, timeout):
    output.mkdir(parents=True, exist_ok=True)
    source, provenance = scenario_source(suite, output)
    temporary = ROOT / '.nsh/test'
    temporary.mkdir(parents=True, exist_ok=True)
    descriptor, filename = tempfile.mkstemp(prefix=SUITES[suite][0] + '-', suffix='.test.tsx', dir=temporary)
    target = Path(filename)
    try:
        with os.fdopen(descriptor, 'w') as stream:
            stream.write(source)
        environment = os.environ.copy()
        environment['LOKI_TEST_DATA_DIR'] = str(output / 'native-data')
        command = ['nsh', 'test', str(target), '--target', 'macos', '--verbose', '--junit', str(output / 'results.xml')]
        try:
            result = subprocess.run(command, cwd=ROOT, env=environment, capture_output=True, text=True, timeout=timeout)
        except subprocess.TimeoutExpired as error:
            parts = [error.stdout or b'', error.stderr or b'']
            (output / 'app.log').write_bytes(b''.join(p.encode() if isinstance(p, str) else p for p in parts))
            raise RuntimeError(f'Native {suite} capture timed out; inspect app.log') from error
        log = result.stdout + result.stderr
        (output / 'app.log').write_text(log)
        print(log, end='')
        if result.returncode:
            raise SystemExit(result.returncode)
        manifest = output / 'captures.json'
        manifest.write_text(json.dumps(capture_manifest(suite, log, provenance), indent=2) + '\n')
        check = subprocess.run(['swift', str(HERE / SUITES[suite][1]), str(manifest)], cwd=ROOT,
                               capture_output=True, text=True, timeout=90)
        (output / 'pixel-check.log').write_text(check.stdout + check.stderr)
        if check.stdout.strip():
            (output / 'pixels.json').write_text(json.dumps(json.loads(check.stdout), indent=2) + '\n')
        if check.returncode:
            raise RuntimeError(f'Native {suite} pixel verification failed; inspect pixels.json and pixel-check.log')
    finally:
        target.unlink(missing_ok=True)
        target.with_name(target.name + '.run.js').unlink(missing_ok=True)
    print(f'Native {suite} evidence: {output}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--suite', choices=(*SUITES, 'all'), default='all')
    parser.add_argument('--output', type=Path, default=ROOT / 'output/tests/visual')
    parser.add_argument('--timeout', type=int, default=360, help='Deadline in seconds for each native capture suite')
    args = parser.parse_args()
    output = args.output.expanduser().resolve()
    if output.is_relative_to(ROOT) and not output.is_relative_to(ROOT / 'output'):
        parser.error('Choose an evidence directory under output/ or outside maintained sources')
    if args.timeout <= 0:
        parser.error('--timeout must be positive')
    for suite in SUITES if args.suite == 'all' else [args.suite]:
        run_suite(suite, output / suite if args.suite == 'all' else output, args.timeout)


if __name__ == '__main__':
    main()
