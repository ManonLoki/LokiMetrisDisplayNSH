"""Official NSH runners preserve application identity and app-owned runtime adapters."""
import argparse
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import plistlib
import struct
import sys
import tarfile
import tempfile
import unittest
import zipfile
from contextlib import redirect_stdout
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools/build'))
from android_manifest import APPLICATION, ACTIVITY, PERMISSIONS, RUNNER_ACTIVITY, component_manifest, lan_permissions, pool, NO_INDEX, strings
import build_android_bridge as java_tools
import package_android as android_package

spec = importlib.util.spec_from_file_location('package_app', ROOT / 'tools/build/package_app.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


def manifest_fixture(*, utf8, named_application, runner=RUNNER_ACTIVITY):
    """A compiled-style manifest with launcher/exported/Rust metadata and an unrelated activity."""
    namespace = 'http://schemas.android.com/apk/res/android'
    values = ['manifest', 'application', 'activity', 'uses-sdk', 'meta-data', 'intent-filter',
              'action', 'category', 'uses-permission', 'name', 'package', 'targetSdkVersion',
              'exported', 'value', 'theme', 'label', 'android', namespace, runner,
              'org.example.product', 'org.example.OriginalApplication', 'org.example.HelpActivity',
              'com.longbridge.nsh.RUST', 'android.intent.action.MAIN',
              'android.intent.category.LAUNCHER', 'android.permission.INTERNET', '诡秘神谕']
    index = values.index
    def attribute(name, value, kind=3, android=True):
        value = index(value) if kind == 3 else value
        return struct.pack('<IIIHBBI', index(namespace) if android else NO_INDEX,
                           index(name), value if kind == 3 else NO_INDEX, 8, 0, kind, value)
    def start(tag, *attributes):
        body = b''.join(attributes)
        return struct.pack('<HHI4I6H', 0x102, 16, 36 + len(body), 0, NO_INDEX, NO_INDEX,
                           index(tag), 20, 20, len(attributes), 0, 0, 0) + body
    def end(tag):
        return struct.pack('<HHI4I', 0x103, 16, 24, 0, NO_INDEX, NO_INDEX, index(tag))
    resources = [0] * len(values)
    for name, resource in {'theme': 0x01010000, 'label': 0x01010001, 'name': 0x01010003,
                           'exported': 0x01010010, 'value': 0x01010024,
                           'targetSdkVersion': 0x01010270}.items():
        resources[index(name)] = resource
    body = pool(values, utf8)
    body += struct.pack('<HHI', 0x180, 8, 8 + 4 * len(resources)) + struct.pack('<' + 'I' * len(resources), *resources)
    body += struct.pack('<HHI4I', 0x100, 16, 24, 0, NO_INDEX, index('android'), index(namespace))
    body += start('manifest', attribute('package', 'org.example.product', android=False))
    body += start('uses-sdk', attribute('targetSdkVersion', 35, 0x10)) + end('uses-sdk')
    body += start('uses-permission', attribute('name', 'android.permission.INTERNET')) + end('uses-permission')
    application = [attribute('theme', 0x7f010001, 1), attribute('label', '诡秘神谕')]
    if named_application:
        application.append(attribute('name', 'org.example.OriginalApplication'))
    body += start('application', *application)
    body += start('meta-data', attribute('name', 'com.longbridge.nsh.RUST'), attribute('value', 0xffffffff, 0x12)) + end('meta-data')
    body += start('activity', attribute('name', runner), attribute('exported', 0xffffffff, 0x12))
    body += start('intent-filter')
    body += start('action', attribute('name', 'android.intent.action.MAIN')) + end('action')
    body += start('category', attribute('name', 'android.intent.category.LAUNCHER')) + end('category')
    body += end('intent-filter') + end('activity')
    body += start('activity', attribute('name', 'org.example.HelpActivity'), attribute('exported', 0, 0x12)) + end('activity')
    body += end('application') + end('manifest')
    body += struct.pack('<HHI4I', 0x101, 16, 24, 0, NO_INDEX, index('android'), index(namespace))
    return struct.pack('<HHI', 3, 8, 8 + len(body)) + body


def manifest_nodes(data):
    values = []; result = []; offset = 8
    while offset < len(data):
        kind, _, size = struct.unpack_from('<HHI', data, offset)
        chunk = data[offset:offset + size]; offset += size
        if kind == 1:
            values, _ = strings(chunk)
        if kind not in {0x102, 0x103}:
            continue
        tag = values[struct.unpack_from('<I', chunk, 20)[0]]
        attributes = {}
        if kind == 0x102:
            start, stride, count = struct.unpack_from('<3H', chunk, 24)
            for position in range(count):
                namespace, name, _, _, _, value_type, value = struct.unpack_from('<IIIHBBI', chunk, 16 + start + stride * position)
                key = (values[namespace] if namespace != NO_INDEX else None, values[name])
                attributes[key] = (value_type, values[value] if value_type == 3 else value)
        result.append((kind, tag, attributes))
    return result


def elf_runtime(*, defined=True, visibility=0, function=True, named=True):
    """Small ELF64 fixture with a linked dynamic string/symbol table and one text function."""
    name = android_package.NATIVE_INIT if named else b'OtherNativeFunction'
    names = b'\0' + name + b'\0'
    name_offset = 64 + 4 * 64
    symbol_offset = (name_offset + len(names) + 7) // 8 * 8
    text_offset = symbol_offset + 48
    output = bytearray(text_offset + 4)
    identity = b'\x7fELF\x02\x01\x01' + bytes(9)
    output[:64] = struct.pack('<16sHHIQQQIHHHHHH', identity, 3, 183, 1, 0, 0, 64, 0, 64, 0, 0, 64, 4, 0)
    def section(index, kind, offset, size, link=0, entry=0):
        output[64 + index * 64:64 + (index + 1) * 64] = struct.pack('<IIQQQQIIQQ',
            0, kind, 0, 0, offset, size, link, 0, 1, entry)
    section(1, 3, name_offset, len(names))
    section(2, 11, symbol_offset, 48, 1, 24)
    section(3, 1, text_offset, 4)
    output[name_offset:name_offset + len(names)] = names
    output[symbol_offset + 24:symbol_offset + 48] = struct.pack('<IBBHQQ',
        1, 0x12 if function else 0x11, visibility, 3 if defined else 0, 0x1000, 4)
    return bytes(output)


class StandardBuildInputs(unittest.TestCase):
    def test_packaging_uses_standard_runner_targets_and_preserves_mobile_ids(self):
        android = package.mobile_command('nsh', 'android', Path('/tmp/output'))
        ios = package.mobile_command('nsh', 'ios', Path('/tmp/output'))
        self.assertEqual(android[android.index('--id') + 1], 'com.manonloki.lokimetrisdisplay')
        app = json.loads((package.ROOT / 'nsh.json').read_text())
        self.assertEqual(android[android.index('--app-version') + 1], app['version'])
        self.assertEqual(android[android.index('--build') + 1], str(app['build']))
        self.assertEqual(ios[ios.index('--target') + 1], 'ios-simulator')
        self.assertEqual(ios[ios.index('--id') + 1], package.apple_identity()['bundleId'])
        device = package.mobile_command('nsh', 'ios', Path('/tmp/output'), ios_device=True)
        self.assertEqual(device[device.index('--target') + 1], 'ios')

    def test_android_seed_selects_supported_abis_without_editing_installed_runtime(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'installed/android/release'
            source.mkdir(parents=True)
            (source / 'classes.dex').write_bytes(b'dex')
            for abi in ['arm64-v8a', 'x86_64', 'armeabi-v7a', 'x86']:
                leaf = source / 'lib' / abi
                leaf.mkdir(parents=True)
                (leaf / 'libnsh.so').write_bytes(abi.encode())
            stage = root / 'stage'; stage.mkdir()
            env = {'NSH_RUNTIME': str(root / 'installed')}
            with patch.object(package, 'ROOT', root):
                package.android_runtime_seed('/unused/nsh', stage, env)
            seed = Path(env['NSH_RUNTIME']) / 'android/release/lib'
            self.assertEqual(sorted(path.name for path in seed.iterdir()), ['arm64-v8a', 'x86_64'])
            self.assertEqual(len(list((source / 'lib').iterdir())), 4)

    def test_signing_credentials_stay_out_of_process_arguments(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'lokimetris.keystore').write_bytes(b'fixture key')
            (root / 'signing.properties').write_text('keyAlias=fixture\nstorePassword=fixture-secret\nkeyPassword=fixture-key-secret\n')
            args, env = package.android_signing(root)
            self.assertNotIn('fixture-secret', str(args))
            self.assertNotIn('fixture-key-secret', str(args))
            self.assertEqual(env['NSH_KEYSTORE_PASSWORD'], 'fixture-secret')
            self.assertEqual(env['LOKI_SIGNING_KEY_PASSWORD'], 'fixture-key-secret')

    def test_ios_discovery_metadata_is_added_before_signing(self):
        with tempfile.TemporaryDirectory() as directory:
            app = Path(directory) / 'Display.app'; app.mkdir()
            info = app / 'Info.plist'
            info.write_bytes(plistlib.dumps({'CFBundleIdentifier': package.APPLE_ID, 'CFBundleExecutable': 'App', 'Other': 'preserved'}))
            signed = []
            def inspect_signing(command):
                signed.append(command)
            with patch.object(package, 'run', side_effect=inspect_signing) as run, \
                 patch.object(package, 'embedded_apple_identifier', return_value=package.apple_identity()['applicationIdentifier']):
                package.configure_apple(app, simulator=True, application_id=package.APPLE_ID, minimum_os='15.0')
            value = plistlib.loads(info.read_bytes())
            self.assertEqual(value['Other'], 'preserved')
            self.assertEqual(value['NSBonjourServices'], ['_loki-host._tcp'])
            self.assertEqual(value['MinimumOSVersion'], '15.0')
            self.assertTrue(value['NSLocalNetworkUsageDescription'])
            self.assertEqual(run.call_count, 2)
            self.assertTrue(all('--entitlements' not in command for command in signed))

    def test_manifest_permissions_are_idempotent_and_keep_target_sdk(self):
        values = ['manifest', 'application', 'uses-sdk', 'targetSdkVersion', 'name',
                  'http://schemas.android.com/apk/res/android']
        def start(tag, attrs=b''):
            return struct.pack('<HHI4I6H', 0x102, 16, 36 + len(attrs), 0, NO_INDEX, NO_INDEX,
                               values.index(tag), 20, 20, len(attrs) // 20, 0, 0, 0) + attrs
        def end(tag):
            return struct.pack('<HHI4I', 0x103, 16, 24, 0, NO_INDEX, NO_INDEX, values.index(tag))
        attribute = struct.pack('<IIIHBBI', 5, 3, NO_INDEX, 8, 0, 0x10, 35)
        body = pool(values, True) + start('manifest') + start('uses-sdk', attribute) + end('uses-sdk')
        body += start('application') + end('application') + end('manifest')
        manifest = struct.pack('<HHI', 3, 8, len(body) + 8) + body
        output, target = lan_permissions(manifest)
        repeated, _ = lan_permissions(output)
        self.assertEqual(output, repeated)
        self.assertEqual(target, 35)
        size = struct.unpack_from('<I', output, 12)[0]
        names, _ = strings(output[8:8 + size])
        self.assertIn('android.permission.CHANGE_WIFI_MULTICAST_STATE', names)
        self.assertIn('android.permission.ACCESS_NETWORK_STATE', names)

    def test_component_manifest_only_changes_app_classes_and_preserves_launcher_contract(self):
        namespace = 'http://schemas.android.com/apk/res/android'
        name = (namespace, 'name')
        for utf8 in [False, True]:
            for named in [False, True]:
                with self.subTest(utf8=utf8, named_application=named):
                    original = manifest_fixture(utf8=utf8, named_application=named)
                    output, target = component_manifest(original)
                    repeated, repeated_target = component_manifest(output)
                    self.assertEqual(output, repeated)
                    self.assertEqual((target, repeated_target), (35, 35))
                    expected = manifest_nodes(original)
                    for kind, tag, attributes in expected:
                        if kind == 0x102 and tag == 'application':
                            attributes[name] = (3, APPLICATION)
                        if kind == 0x102 and tag == 'activity' and attributes.get(name) == (3, RUNNER_ACTIVITY):
                            attributes[name] = (3, ACTIVITY)
                    actual = manifest_nodes(output)
                    self.assertEqual([node for node in actual if node[1] != 'uses-permission'],
                                     [node for node in expected if node[1] != 'uses-permission'])
                    permissions = [attributes[name][1] for kind, tag, attributes in actual
                                   if kind == 0x102 and tag == 'uses-permission']
                    self.assertCountEqual(permissions, ['android.permission.INTERNET'] + PERMISSIONS)

    def test_component_manifest_rejects_an_unrecognized_runner(self):
        original = manifest_fixture(utf8=True, named_application=False, runner='org.example.UnrelatedActivity')
        with self.assertRaisesRegex(RuntimeError, 'standard NSH application and runner activity'):
            component_manifest(original)

    def test_java_merge_keeps_official_class_bytes_and_rejects_sdk_class_overrides(self):
        for collision in [False, True]:
            with self.subTest(collision=collision), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                platform = root / 'sdk/platforms/android-37/android.jar'
                platform.parent.mkdir(parents=True); platform.write_bytes(b'fixture API')
                sources = root / 'app-java'; sources.mkdir()
                (sources / 'DisplayActivity.java').write_text('class DisplayActivity {}')
                original = {'com/longbridge/nsh/NshView.class': b'official view bytecode',
                            'com/longbridge/nsh/Native.class': b'official native bytecode'}
                sdk_jar = root / 'official.jar'; java_tools.write_jar(sdk_jar, original)
                aar = root / 'nsh.aar'
                with zipfile.ZipFile(aar, 'w') as archive:
                    archive.write(sdk_jar, 'classes.jar')
                def javac(command, **_):
                    destination = Path(command[command.index('-d') + 1])
                    name = 'com/longbridge/nsh/NshView.class' if collision else 'com/manonloki/lokimetrisdisplay/bridge/DisplayActivity.class'
                    path = destination / name; path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(b'new application bytecode')
                args = argparse.Namespace(out=root / 'out', sdk=root / 'sdk', compile_sdk=37, javac=None, aar=aar)
                with patch.object(java_tools, 'SOURCE', sources), \
                     patch.object(java_tools, 'java_compiler', return_value=Path('/fixture/javac')), \
                     patch.object(java_tools.subprocess, 'check_output', return_value='javac fixture'), \
                     patch.object(java_tools.subprocess, 'run', side_effect=javac), redirect_stdout(io.StringIO()):
                    if collision:
                        with self.assertRaisesRegex(RuntimeError, 'replace an official NSH class'):
                            java_tools.build(args)
                        self.assertFalse((args.out / 'classes.jar').exists())
                    else:
                        report = java_tools.build(args)
                        self.assertFalse(report['officialSdk']['modified'])
                        merged = java_tools.jar_entries((args.out / 'classes.jar').read_bytes())
                        for name, bytecode in original.items():
                            self.assertEqual(merged[name], bytecode)
                        self.assertEqual(len(merged), len(original) + 1)

    def test_java_source_fallback_checks_locked_archive_without_legacy_checksum_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); cargo = root / 'cargo'
            source = cargo / 'registry/src/index.crates.io-fixture/nsh-platform-android-0.3.0'
            relative = 'java/com/longbridge/nsh/NshView.java'
            original = b'package com.longbridge.nsh; public class NshView {}'
            path = source / relative; path.parent.mkdir(parents=True); path.write_bytes(original)
            archive = source.parents[2] / 'cache' / source.parent.name / (source.name + '.crate')
            archive.parent.mkdir(parents=True)
            with tarfile.open(archive, 'w:gz') as release:
                member = tarfile.TarInfo(source.name + '/' + relative); member.size = len(original)
                release.addfile(member, io.BytesIO(original))
            checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
            (root / 'Cargo.lock').write_text('version = 4\n[[package]]\nname = "nsh-platform-android"\n'
                'version = "0.3.0"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\n'
                'checksum = "' + checksum + '"\n')
            platform = root / 'sdk/platforms/android-37/android.jar'
            platform.parent.mkdir(parents=True); platform.write_bytes(b'fixture API')
            app = root / 'app-java'; app.mkdir(); (app / 'Bridge.java').write_text('class Bridge {}')
            def javac(command, **_):
                self.assertIn(str(path), command)
                destination = Path(command[command.index('-d') + 1])
                for name in ['com/longbridge/nsh/NshView.class', 'com/example/Bridge.class']:
                    compiled = destination / name; compiled.parent.mkdir(parents=True, exist_ok=True)
                    compiled.write_bytes(b'compiled ' + name.encode())
            args = argparse.Namespace(out=root / 'out', sdk=root / 'sdk', compile_sdk=37, javac=None)
            with patch.object(java_tools, 'ROOT', root), patch.object(java_tools, 'SOURCE', app), \
                 patch.dict(java_tools.os.environ, {'CARGO_HOME': str(cargo)}), \
                 patch.object(java_tools, 'official_aar', return_value=None), \
                 patch.object(java_tools, 'java_compiler', return_value=Path('/fixture/javac')), \
                 patch.object(java_tools.subprocess, 'check_output', return_value='javac fixture'), \
                 patch.object(java_tools.subprocess, 'run', side_effect=javac) as compile_java, redirect_stdout(io.StringIO()):
                self.assertFalse((source / '.cargo-checksum.json').exists())
                report = java_tools.build(args)
                self.assertEqual(report['officialSdk']['kind'], 'official-crates-io-java')
                self.assertEqual(report['officialSdk']['crateChecksum'], checksum)
                self.assertEqual(path.read_bytes(), original)
                self.assertEqual(compile_java.call_count, 1)
                path.write_bytes(original + b' // locally modified')
                compile_java.reset_mock()
                with self.assertRaisesRegex(RuntimeError, 'Java source .*differs|Java source checksum differs'):
                    java_tools.build(args)
                compile_java.assert_not_called()
                path.write_bytes(original)
                archive.write_bytes(archive.read_bytes() + b'corrupt cache')
                with self.assertRaisesRegex(RuntimeError, 'archive checksum differs'):
                    java_tools.build(args)
                compile_java.assert_not_called()

    def test_jni_export_verifier_requires_a_defined_visible_dynamic_function(self):
        self.assertTrue(android_package.has_native_init(elf_runtime()))
        self.assertFalse(android_package.has_native_init(elf_runtime(defined=False)))
        self.assertFalse(android_package.has_native_init(elf_runtime(visibility=2)))
        self.assertFalse(android_package.has_native_init(elf_runtime(function=False)))
        self.assertFalse(android_package.has_native_init(elf_runtime(named=False) + android_package.NATIVE_INIT))

    def test_android_packaging_rejects_stale_abi_before_java_or_signing_and_preserves_input(self):
        with tempfile.TemporaryDirectory() as directory:
            apk = Path(directory) / 'App.apk'
            with zipfile.ZipFile(apk, 'w') as archive:
                archive.writestr('lib/arm64-v8a/libnsh.so', elf_runtime())
                archive.writestr('lib/x86_64/libnsh.so', elf_runtime(named=False) + android_package.NATIVE_INIT)
                archive.writestr('classes.dex', b'original dex')
            before = apk.read_bytes()
            with patch.object(android_package, 'sdk_directory') as sdk, \
                 patch.object(android_package.subprocess, 'run') as run:
                with self.assertRaisesRegex(RuntimeError, 'lacks exported .*lib/x86_64/libnsh.so'):
                    android_package.update_java_and_sign(apk, [], {}, None)
                sdk.assert_not_called()
                run.assert_not_called()
            self.assertEqual(apk.read_bytes(), before)
            with zipfile.ZipFile(apk, 'w') as archive:
                for abi in ['x86_64', 'arm64-v8a']:
                    archive.writestr('lib/' + abi + '/libnsh.so', elf_runtime())
            self.assertEqual(android_package.verify_jni_exports(apk), ['arm64-v8a', 'x86_64'])


if __name__ == '__main__':
    unittest.main()
