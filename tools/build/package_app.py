#!/usr/bin/env python3
"""Package five NSH applications with the official SDK and app-owned system adapters."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import shutil
import shlex
import subprocess
import struct
import sys
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[2]
ANDROID_ID = 'com.manonloki.lokimetrisdisplay'
APPLE_ID = 'com.manonloki.lokimetrisdisplay.LokiMetrisDisplay'


def run(command, *, env=None, log=None):
    subprocess.run([str(item) for item in command], cwd=ROOT, env=env, check=True,
                   stdout=log, stderr=subprocess.STDOUT if log else None)


def android_signing(directory=None):
    """Passwords travel through the subprocess environment, never command text."""
    directory = directory or ROOT / 'signing/android'
    store = directory / 'lokimetris.keystore'
    properties = directory / 'signing.properties'
    if not store.is_file() or not properties.is_file():
        raise RuntimeError('Restore signing/android/lokimetris.keystore and signing.properties; no new key was generated')
    values = dict(line.split('=', 1) for line in properties.read_text().splitlines()
                  if '=' in line and not line.lstrip().startswith('#'))
    if not values.get('keyAlias') or not values.get('storePassword'):
        raise RuntimeError('Android signing properties are incomplete')
    env = os.environ.copy()
    env['NSH_KEYSTORE_PASSWORD'] = values['storePassword']
    env['LOKI_SIGNING_KEY_PASSWORD'] = values.get('keyPassword', values['storePassword'])
    return ['--keystore', store, '--key-alias', values['keyAlias']], env


def apple_identity():
    path = ROOT / 'signing/apple/identity.json'
    return json.loads(path.read_text()) if path.is_file() else {'bundleId': APPLE_ID, 'version': '1.0.10', 'build': 19}


def widget_group(application_id):
    identity = apple_identity()
    group = identity.get('widgetAppGroup') if application_id == identity['bundleId'] else None
    return group or 'group.' + application_id


def widget_linker(directory, response_file, *, sdk, runner_manifest, linker=None):
    """Apply the host shim only to the NSH executable, never dependency dylibs."""
    configuration = {'response': str(response_file),
                     'responseSha256': hashlib.sha256(response_file.read_bytes()).hexdigest(),
                     'runnerManifest': str(runner_manifest),
                     'linker': linker or ['xcrun', '--sdk', sdk, 'clang']}
    script = '#!' + sys.executable + '\n' + '''import json
import os
from pathlib import Path
import re
import shlex
import sys
import tomllib

CONFIGURATION = ''' + repr(configuration) + '''

def expanded(arguments, depth=0):
    if depth > 4:
        raise RuntimeError('Linker response files are nested too deeply')
    result = []
    for argument in arguments:
        if argument.startswith('@') and Path(argument[1:]).is_file():
            result.extend(expanded(shlex.split(Path(argument[1:]).read_text()), depth + 1))
        else:
            result.append(argument)
    return result

def final_runner(arguments):
    values = expanded(arguments)
    if any(flag in values for flag in ['-dynamiclib', '-shared', '-bundle']):
        return False
    outputs = [values[index + 1] for index, value in enumerate(values[:-1]) if value == '-o']
    if not outputs:
        return False
    manifest = tomllib.loads(Path(CONFIGURATION['runnerManifest']).read_text())
    names = [entry['name'] for entry in manifest.get('bin', [])]
    if len(names) != 1:
        raise RuntimeError('Expected the generated NSH iPhone runner to declare one executable')
    name = names[0]
    output = Path(outputs[-1]).name
    normalized = name.replace('-', '_')
    return output in {name, normalized} or re.fullmatch(re.escape(normalized) + '-[0-9a-f]{8,64}', output) is not None

if __name__ == '__main__':
    arguments = sys.argv[1:]
    applies = final_runner(arguments)
    if applies:
        arguments.append('@' + CONFIGURATION['response'])
        entitlements = os.environ.get('LOKI_WIDGET_SIMULATOR_ENTITLEMENTS')
        if entitlements:
            arguments.append('-Wl,-sectcreate,__TEXT,__entitlements,' + entitlements)
    audit = Path(__file__).with_name('link-events.jsonl')
    values = expanded(sys.argv[1:])
    outputs = [values[index + 1] for index, value in enumerate(values[:-1]) if value == '-o']
    with audit.open('a') as output:
        output.write(json.dumps({'output': outputs[-1] if outputs else None, 'hostShimAdded': applies}) + '\\n')
    command = CONFIGURATION['linker'] + arguments
    os.execvp(command[0], command)
'''
    path = directory / ('WidgetHostLinker-' + hashlib.sha256(script.encode()).hexdigest() + '.py')
    if not path.exists() or path.read_text() != script:
        path.write_text(script)
    path.chmod(0o755)
    return path


def build_widget_support(env, *, simulator, sdk_path, developer, deployment, log=None):
    """Compile the extension core and link a small native host shim into the NSH runner."""
    target = 'aarch64-apple-ios-sim' if simulator else 'aarch64-apple-ios'
    sdk = 'iphonesimulator' if simulator else 'iphoneos'
    shared = ROOT / 'platform/ios/shared'
    sources = [shared / (name + '.swift') for name in
               ['LokiCore', 'PlatformRules', 'SharedSupport', 'WidgetContainer', 'WidgetRefresh', 'WidgetRefreshCommitGate']]
    sources.append(ROOT / 'platform/ios/bridge/WidgetHostBridge.swift')
    bootstrap = ROOT / 'platform/ios/bridge/WidgetHostBootstrap.c'
    digest = hashlib.sha256()
    for source in sources + [bootstrap]:
        digest.update(str(source.relative_to(ROOT)).encode()); digest.update(source.read_bytes())
    digest.update((str(sdk_path) + deployment).encode())
    output = ROOT / '.nsh/widget-support' / target / digest.hexdigest()
    output.mkdir(parents=True, exist_ok=True)
    rust_output = ROOT / 'target/widget-core'
    run(['cargo', 'build', '--manifest-path', ROOT / 'Cargo.toml', '-p', 'loki_widget_core',
         '--release', '--locked', '--target', target, '--target-dir', rust_output], env=env, log=log)
    library = rust_output / target / 'release/libloki_widget_core.a'
    if not library.is_file():
        raise RuntimeError('Widget core build did not produce its static library')
    swift_object = output / 'WidgetHostBridge.o'
    bootstrap_object = output / 'WidgetHostBootstrap.o'
    triple = 'arm64-apple-ios' + deployment + ('-simulator' if simulator else '')
    if not swift_object.is_file():
        run(['xcrun', '--sdk', sdk, 'swiftc', '-swift-version', '5', '-parse-as-library', '-whole-module-optimization',
             '-emit-object', '-O', '-module-name', 'LokiWidgetHost', '-target', triple,
             '-sdk', sdk_path, *sources, '-o', swift_object], env=env, log=log)
    if not bootstrap_object.is_file():
        run(['xcrun', '--sdk', sdk, 'clang', '-target', triple, '-isysroot', sdk_path,
             '-c', bootstrap, '-o', bootstrap_object], env=env, log=log)
    toolchain = Path(developer) / 'Toolchains/XcodeDefault.xctoolchain/usr/lib/swift' / sdk
    arguments = [str(swift_object), str(bootstrap_object), '-L', str(toolchain),
                 '-L', str(Path(sdk_path) / 'usr/lib/swift'), '-Wl,-rpath,/usr/lib/swift']
    for framework in ['Foundation', 'UIKit', 'SwiftUI', 'WidgetKit', 'BackgroundTasks']:
        arguments.extend(['-framework', framework])
    # Cargo links some dependencies as dylibs. A target linker wrapper adds the
    # Swift host objects only when linking the generated NSH application binary.
    response = ''.join(shlex.quote(value) + '\n' for value in arguments)
    response_file = output / ('WidgetHostLink-' + hashlib.sha256(response.encode()).hexdigest() + '.rsp')
    if response_file.exists() and response_file.read_text() != response:
        raise RuntimeError('Widget link response path contains unexpected data')
    response_file.write_text(response)
    wrapper = widget_linker(output, response_file, sdk=sdk,
                            runner_manifest=ROOT / '.nsh/rust/ios-app/Cargo.toml')
    key = 'CARGO_TARGET_' + target.upper().replace('-', '_') + '_LINKER'
    env[key] = str(wrapper)
    return library, {'target': target, 'hostSourceSha256': digest.hexdigest(),
                     'hostObjectSha256': hashlib.sha256(swift_object.read_bytes()).hexdigest(),
                     'bootstrapObjectSha256': hashlib.sha256(bootstrap_object.read_bytes()).hexdigest(),
                     'hostLinker': str(wrapper), 'hostLinkResponse': str(response_file),
                     'widgetCoreSha256': hashlib.sha256(library.read_bytes()).hexdigest()}


def mobile_command(cli, target, stage, *, ios_device=False, application_id=None):
    sdk_target = 'ios' if ios_device else 'ios-simulator'
    identity = apple_identity() if target == 'ios' else {'bundleId': ANDROID_ID, 'version': '1.0.8', 'build': 10}
    return [cli, 'package', '--target', 'android' if target == 'android' else sdk_target,
            '--id', application_id or identity['bundleId'], '--app-version', identity['version'],
            '--build', str(identity['build']), '--out', stage]



def android_runtime_seed(cli, stage, env):
    """Let the CLI build the project's supported 64-bit ABIs without global edits."""
    original = Path(env.get('NSH_RUNTIME') or Path(cli).resolve().parent.parent / 'runtime') / 'android/release'
    seed = stage / 'runtime-seed/android/release'
    seed.mkdir(parents=True)
    shutil.copy2(original / 'classes.dex', seed / 'classes.dex')
    for abi in ['arm64-v8a', 'x86_64']:
        source = original / 'lib' / abi / 'libnsh.so'
        if not source.is_file():
            raise RuntimeError('Installed NSH runtime is missing the supported ABI: ' + abi)
        directory = seed / 'lib' / abi
        directory.mkdir(parents=True)
        shutil.copy2(source, directory / 'libnsh.so')
    # Cargo rebuilds these libraries; the seed only selects which ABIs exist.
    env['NSH_RUNTIME'] = str(stage / 'runtime-seed')
    generated = ROOT / '.nsh/rust/android-release/lib'
    if generated.is_dir():
        for path in generated.iterdir():
            if path.name not in ['arm64-v8a', 'x86_64']:
                shutil.rmtree(path)


def verify_runner(target):
    """Generated runners must include the local application's Rust module."""
    import tomllib
    path = ROOT / '.nsh/rust' / ('android/Cargo.toml' if target == 'android' else 'ios-app/Cargo.toml')
    dependencies = tomllib.loads(path.read_text()).get('dependencies', {}).values()
    if not any(isinstance(value, dict) and value.get('path')
               and Path(value['path']).resolve() == ROOT for value in dependencies):
        raise RuntimeError('NSH runner omitted the local application Rust module')


def verify_android(path):
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        if not any(name.endswith('bundle.nshc') for name in names):
            raise RuntimeError('APK is missing the NSH application bundle')
        libraries = [name for name in names if name.startswith('lib/') and name.endswith('/libnsh.so')]
        if not libraries:
            raise RuntimeError('APK is missing its NSH Rust runtime')
        architectures = []
        for name in libraries:
            abi = name.split('/')[1]
            expected = {'arm64-v8a': 183, 'x86_64': 62, 'armeabi-v7a': 40, 'x86': 3}.get(abi)
            with archive.open(name) as library:
                header = library.read(20)
            if expected is None or header[:4] != b'\x7fELF' or int.from_bytes(header[18:20], 'little') != expected:
                raise RuntimeError('Invalid NSH runtime architecture: ' + abi)
            architectures.append(abi)
    return {'architectures': architectures, 'bundleVerified': True}


def simulator_link_permissions(env, application_id):
    """Simulator permissions belong in __TEXT, not restricted code-sign blobs."""
    identifier = apple_identity().get('applicationIdentifier')
    if not identifier or not identifier.endswith('.' + application_id):
        raise RuntimeError('Restore the matching applicationIdentifier in signing/apple/identity.json')
    data = plistlib.dumps({'application-identifier': identifier,
                          'com.apple.security.application-groups': [widget_group(application_id)]})
    directory = ROOT / '.nsh/signing'
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / (hashlib.sha256(data).hexdigest() + '.plist')
    if not path.exists():
        path.write_bytes(data)
    env['LOKI_WIDGET_SIMULATOR_ENTITLEMENTS'] = str(path)


def embedded_apple_identifier(executable):
    with executable.open('rb') as source:
        header = source.read(32)
        if header[:4] != b'\xcf\xfa\xed\xfe':
            raise RuntimeError('Expected a 64-bit simulator Mach-O runner')
        commands = struct.unpack_from('<I', header, 16)[0]
        for _ in range(commands):
            position = source.tell()
            kind, size = struct.unpack('<II', source.read(8))
            source.seek(position)
            command = source.read(size)
            if kind != 0x19:
                continue
            for index in range(struct.unpack_from('<I', command, 64)[0]):
                section = 72 + index * 80
                name = command[section:section + 16].split(b'\0')[0]
                segment = command[section + 16:section + 32].split(b'\0')[0]
                if name == b'__entitlements' and segment == b'__TEXT':
                    length = struct.unpack_from('<Q', command, section + 40)[0]
                    offset = struct.unpack_from('<I', command, section + 48)[0]
                    source.seek(offset)
                    return plistlib.loads(source.read(length)).get('application-identifier')
    raise RuntimeError('Simulator runtime is missing its embedded Keychain identity entitlement')


def configure_apple(app, *, simulator, application_id, minimum_os=None):
    """Supply discovery declarations before final signing of the NSH runner."""
    info = app / 'Info.plist'
    value = plistlib.loads(info.read_bytes())
    if value.get('CFBundleIdentifier') != application_id:
        raise RuntimeError('iOS package identifier differs from its configured identity')
    if minimum_os:
        value['MinimumOSVersion'] = minimum_os
    value['NSLocalNetworkUsageDescription'] = '连接同一局域网内已配对的电脑，接收 Token 用量、账户额度与 Agent 活动。'
    value['NSBonjourServices'] = ['_loki-host._tcp']
    value['LokiWidgetAppGroup'] = widget_group(application_id)
    value['BGTaskSchedulerPermittedIdentifiers'] = ['com.manonloki.lokimetrisdisplay.widget-refresh']
    value['UIBackgroundModes'] = sorted(set(value.get('UIBackgroundModes', []) + ['fetch']))
    value['NSAppTransportSecurity'] = dict(value.get('NSAppTransportSecurity', {}), NSAllowsLocalNetworking=True)
    info.write_bytes(plistlib.dumps(value, fmt=plistlib.FMT_BINARY))
    if simulator:
        expected = apple_identity().get('applicationIdentifier')
        executable = app / value['CFBundleExecutable']
        if embedded_apple_identifier(executable) != expected:
            raise RuntimeError('Embedded simulator Keychain identity differs from the preserved identity')
        # AMFI rejects application-identifier in an ad hoc signature (-424).
        # Xcode simulator runners use a __TEXT section with an empty signature.
        run(['codesign', '--force', '--sign', '-', app])
        run(['codesign', '--verify', '--deep', '--strict', app])
    return {'simulatorOnly': simulator, 'signatureVerified': simulator,
            'keychainApplicationIdentifierDeclared': simulator,
            'localNetworkDeclared': True, 'bonjourServices': value['NSBonjourServices'],
            'minimumOsVersion': value.get('MinimumOSVersion')}


def package_mobile(target, output, *, ios_device=False, application_id=None, with_watch=False):
    cli = shutil.which('nsh')
    if not cli:
        raise RuntimeError('An existing NSH installation is required; no installation was attempted')
    destination = output / ('ios-device' if ios_device else target)
    if destination.exists():
        raise RuntimeError('Choose a new --out; package already exists: ' + str(destination))
    output.mkdir(parents=True, exist_ok=True)
    log_dir = ROOT / 'output/build' / destination.name
    log_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='nsh-package-', dir=output) as temporary:
        stage = Path(temporary)
        command = mobile_command(cli, target, stage, ios_device=ios_device, application_id=application_id)
        env = None
        widget_library = None; widget_support = None
        if target == 'ios':
            from package_apple_companion import deployment_target
            env = os.environ.copy()
            sdk_name = 'iphoneos' if ios_device else 'iphonesimulator'
            sdk_path = subprocess.check_output(['xcrun', '--sdk', sdk_name, '--show-sdk-path'], text=True).strip()
            developer = subprocess.check_output(['xcode-select', '-p'], text=True).strip()
            env['SDKROOT'] = sdk_path
            # Rust otherwise defaults device linking to iOS 10 and emits the
            # legacy minimum-version command instead of LC_BUILD_VERSION.
            env['IPHONEOS_DEPLOYMENT_TARGET'] = deployment_target(not ios_device, env.get('IPHONEOS_DEPLOYMENT_TARGET', '15.0'))
            if not list(Path(env.get('LIBCLANG_PATH', '')).glob('libclang*.dylib')):
                env['LIBCLANG_PATH'] = str(Path(developer) / 'Toolchains/XcodeDefault.xctoolchain/usr/lib')
            env['BINDGEN_EXTRA_CLANG_ARGS'] = env.get('BINDGEN_EXTRA_CLANG_ARGS', '') + ' -isysroot ' + shlex.quote(sdk_path)
        if target == 'android':
            signing, env = android_signing()
            if platform.system() == 'Darwin' and not list(Path(env.get('LIBCLANG_PATH', '')).glob('libclang*.dylib')):
                developer = subprocess.check_output(['xcode-select', '-p'], text=True).strip()
                env['LIBCLANG_PATH'] = str(Path(developer) / 'Toolchains/XcodeDefault.xctoolchain/usr/lib')
            command.extend(signing)
            android_runtime_seed(cli, stage, env)
        with (log_dir / 'package.log').open('wb') as log:
            if target == 'ios':
                widget_library, widget_support = build_widget_support(env, simulator=not ios_device,
                    sdk_path=sdk_path, developer=developer, deployment=env['IPHONEOS_DEPLOYMENT_TARGET'], log=log)
                if not ios_device:
                    simulator_link_permissions(env, command[command.index('--id') + 1])
            run(command, env=env, log=log)
            verify_runner(target)
            if target == 'android':
                shutil.rmtree(stage / 'runtime-seed')
            suffix = '*.apk' if target == 'android' else '*.app'
            packages = list(stage.glob(suffix))
            if len(packages) != 1:
                raise RuntimeError('Expected exactly one standard NSH package; inspect ' + str(log_dir / 'package.log'))
            if target == 'android':
                from package_android import update_java_and_sign
                target_sdk = update_java_and_sign(packages[0], signing, env, log)
                metadata = verify_android(packages[0])
                metadata['sha256'] = hashlib.sha256(packages[0].read_bytes()).hexdigest()
                metadata['officialSdkUnmodified'] = True
                metadata['appJavaBridgeCompiled'] = True
                metadata['targetSdk'] = target_sdk
                metadata['lanPermissionsDeclared'] = True
            else:
                metadata = configure_apple(packages[0], simulator=not ios_device,
                                           application_id=command[command.index('--id') + 1],
                                           minimum_os=env['IPHONEOS_DEPLOYMENT_TARGET'])
                from package_apple_companion import assemble
                archived, companion = assemble(packages[0], stage / 'NSHCompanion.xcarchive',
                                                simulator=not ios_device, log=log,
                                                with_watch=with_watch, widget_library=widget_library)
                shutil.rmtree(packages[0])
                shutil.copytree(archived, packages[0], symlinks=True)
                metadata['appleCompanions'] = companion
                metadata['widgetSupport'] = widget_support
                if with_watch:
                    metadata['watchCompanion'] = companion
        metadata.update({'target': target, 'applicationId': command[command.index('--id') + 1],
                         'package': packages[0].name, 'runnerIncludesLocalRust': True, 'runtimeVerified': False,
                         'note': 'Build and package checks only; runtime acceptance is recorded separately.'})
        (stage / 'platform-verification.json').write_text(json.dumps(metadata, ensure_ascii=False, indent=2) + '\n')
        if destination.exists():
            raise RuntimeError('Output appeared during packaging: ' + str(destination))
        stage.rename(destination)
    print(destination)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', choices=['macos', 'windows', 'linux', 'android', 'ios'], required=True)
    parser.add_argument('--out', type=Path, default=ROOT / 'output/packages')
    parser.add_argument('--id', help='Explicit mobile application identifier; defaults preserve this project\'s existing identities')
    parser.add_argument('--ios-device', action='store_true', help='Build an unsigned physical-device .app; iOS otherwise targets the simulator')
    parser.add_argument('--with-watch', action='store_true', help='Include the independent Watch companion; iOS always includes Widgets')
    args = parser.parse_args()
    if args.ios_device and args.target != 'ios':
        parser.error('--ios-device requires --target ios')
    if args.with_watch and args.target != 'ios':
        parser.error('--with-watch requires --target ios')
    if args.id and args.target not in ['ios', 'android']:
        parser.error('--id is a mobile override; desktop identity comes from nsh.json')
    output = args.out.expanduser().resolve()
    if output == ROOT or any(output.is_relative_to(ROOT / part) for part in ['src', 'platform', 'tools', 'signing', 'vendor', 'docs']):
        parser.error('Output must be separate from maintained source directories')
    if args.target in ['macos', 'windows', 'linux']:
        cross = args.target == 'windows' and platform.system() == 'Darwin'
        script = 'package_windows_cross.py' if cross else 'package_desktop.py'
        command = [sys.executable, Path(__file__).with_name(script), '--out', output]
        if not cross:
            command.extend(['--target', args.target])
        run(command)
    else:
        package_mobile(args.target, output, ios_device=args.ios_device, application_id=args.id,
                       with_watch=args.with_watch)


if __name__ == '__main__':
    try:
        main()
    except (OSError, RuntimeError, subprocess.CalledProcessError, zipfile.BadZipFile) as error:
        raise SystemExit(str(error))
