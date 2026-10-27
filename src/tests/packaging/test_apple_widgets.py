"""Widgets retain reviewed identifiers, explicit shared grants and native code."""
from pathlib import Path
import importlib.util
import json
import os
import plistlib
import shlex
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from test_apple_device_signing import signing, provision, macho, TEAM, BUNDLE, FINGERPRINT

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location('widget_package', ROOT / 'tools/build/package_app.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


def application(root):
    phone = root / 'Phone.app'; widget = phone / 'PlugIns/Widgets.appex'
    widget.mkdir(parents=True); (phone / 'nsh').mkdir()
    (phone / 'nsh/bundle.nshc').write_bytes(b'unchanged NSH bytecode')
    for app, identifier in [(phone, 'old.phone'), (widget, 'old.phone.widgets')]:
        info = {'CFBundleIdentifier': identifier, 'CFBundleExecutable': 'App',
                'CFBundleVersion': '17', 'CFBundleShortVersionString': '1.0.9',
                'LokiWidgetAppGroup': 'group.old.phone'}
        if app == widget:
            info['NSExtension'] = {'NSExtensionPointIdentifier': 'com.apple.widgetkit-extension'}
        (app / 'Info.plist').write_bytes(plistlib.dumps(info))
        (app / 'App').write_bytes(macho())
    return phone, widget


class WidgetSigning(unittest.TestCase):
    def test_only_explicit_authorized_groups_are_requested(self):
        value = provision()
        value['Entitlements']['com.apple.security.application-groups'] = ['group.' + BUNDLE, 'group.other.app']
        request = signing.development_entitlements(value, BUNDLE, TEAM, FINGERPRINT,
                                                   app_groups=['group.' + BUNDLE])
        self.assertEqual(request['com.apple.security.application-groups'], ['group.' + BUNDLE])
        for groups in [['group.unauthorized.app'], ['invalid group']]:
            with self.subTest(groups=groups), self.assertRaises(RuntimeError):
                signing.development_entitlements(value, BUNDLE, TEAM, FINGERPRINT, app_groups=groups)

    def test_plan_validates_widget_profile_group_platform_identity_and_version(self):
        with tempfile.TemporaryDirectory() as directory:
            phone, widget = application(Path(directory))
            widget_info = plistlib.loads((widget / 'Info.plist').read_bytes())
            value = provision()
            value['Entitlements']['com.apple.security.application-groups'] = ['group.' + BUNDLE]
            def plan(widget_profile=Path('widget.mobileprovision')):
                return signing.plan(phone, BUNDLE, TEAM, FINGERPRINT, Path('phone.mobileprovision'),
                                    None, 'phone', widget_profile=widget_profile, app_group='group.' + BUNDLE)
            with patch.object(signing, 'certificate', return_value=TEAM), \
                 patch.object(signing, 'profile', return_value=value), \
                 patch.object(signing, 'macho_platforms', return_value={2}):
                report = plan()
                self.assertEqual(report['widgetBundleId'], BUNDLE + '.widgets')
                self.assertEqual(report['phoneProfile']['entitlements']['com.apple.security.application-groups'],
                                 report['widgetProfile']['entitlements']['com.apple.security.application-groups'])
                self.assertNotIn('watchRelativePath', report)
                with self.assertRaisesRegex(RuntimeError, 'own development profile'):
                    plan(None)
                for key, invalid in [('CFBundleIdentifier', 'unknown.extension'),
                                     ('LokiWidgetAppGroup', 'group.other.app'), ('CFBundleVersion', '16')]:
                    modified = dict(widget_info); modified[key] = invalid
                    (widget / 'Info.plist').write_bytes(plistlib.dumps(modified))
                    with self.subTest(key=key), self.assertRaises(RuntimeError):
                        plan()
                (widget / 'Info.plist').write_bytes(plistlib.dumps(widget_info))
                with patch.object(signing, 'macho_platforms', side_effect=lambda path: {7} if path.parent == widget.resolve() else {2}), \
                     self.assertRaisesRegex(RuntimeError, 'device platform'):
                    plan()
                value['Entitlements']['com.apple.security.application-groups'] = []
                with self.assertRaisesRegex(RuntimeError, 'App Group'):
                    plan()

    def test_widget_is_signed_before_phone_with_remapped_group_and_immutable_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); phone, widget = application(root)
            value = provision()
            value['Entitlements']['com.apple.security.application-groups'] = ['group.' + BUNDLE]
            profile = root / 'profile.mobileprovision'; profile.write_bytes(b'profile fixture')
            with patch.object(signing, 'certificate', return_value=TEAM), \
                 patch.object(signing, 'profile', return_value=value), \
                 patch.object(signing, 'macho_platforms', return_value={2}):
                report = signing.plan(phone, BUNDLE, TEAM, FINGERPRINT, profile, None, 'phone',
                                      widget_profile=profile, app_group='group.' + BUNDLE)
            output = root / 'signed/Phone.app'; signed = []
            def codesign(arguments):
                if '--force' in arguments:
                    signed.append(Path(arguments[-1])); return b''
                if '--display' in arguments:
                    entitlements = report['phoneProfile' if Path(arguments[-1]) == output else 'widgetProfile']['entitlements']
                    return plistlib.dumps(entitlements)
                return b''
            with patch.object(signing, 'command', side_effect=codesign):
                signing.sign_copy(phone, output, report, profile, widget_profile=profile)
            self.assertEqual(signed, [output / 'PlugIns/Widgets.appex', output])
            for target, identifier in [(output, BUNDLE), (output / 'PlugIns/Widgets.appex', BUNDLE + '.widgets')]:
                info = plistlib.loads((target / 'Info.plist').read_bytes())
                self.assertEqual(info['CFBundleIdentifier'], identifier)
                self.assertEqual(info['LokiWidgetAppGroup'], 'group.' + BUNDLE)
            self.assertEqual(plistlib.loads((phone / 'Info.plist').read_bytes())['CFBundleIdentifier'], 'old.phone')
            self.assertTrue(report['phoneInstructionsPreserved'] and report['widgetInstructionsPreserved'])


class WidgetBuildInputs(unittest.TestCase):
    def test_native_support_uses_target_scoped_linking_and_no_second_phone_rust_library(self):
        for simulator in [False, True]:
            with self.subTest(simulator=simulator), tempfile.TemporaryDirectory(prefix='widget fixture ') as directory:
                root = Path(directory)
                shared = root / 'platform/ios/shared'; shared.mkdir(parents=True)
                bridge = root / 'platform/ios/bridge'; bridge.mkdir()
                for name in ['LokiCore', 'PlatformRules', 'SharedSupport', 'WidgetContainer', 'WidgetRefresh', 'WidgetRefreshCommitGate']:
                    (shared / (name + '.swift')).write_text('// Swift fixture ' + name)
                for name in ['WidgetHostBridge.swift', 'WidgetHostBootstrap.c']:
                    (bridge / name).write_text('// native fixture ' + name)
                commands = []
                def run(arguments, **kwargs):
                    command = [str(value) for value in arguments]; commands.append(command)
                    if command[0] == 'cargo':
                        target = command[command.index('--target') + 1]
                        library = root / 'target/widget-core' / target / 'release/libloki_widget_core.a'
                        library.parent.mkdir(parents=True); library.write_bytes(b'static core fixture')
                    else:
                        Path(command[command.index('-o') + 1]).write_bytes(b'native object fixture')
                env = {}
                with patch.object(package, 'ROOT', root), patch.object(package, 'run', side_effect=run):
                    library, report = package.build_widget_support(env, simulator=simulator,
                        sdk_path=Path('/fixture SDK'), developer=Path('/fixture Xcode'), deployment='15.0')
                self.assertIn('--locked', commands[0])
                self.assertEqual(report['target'], 'aarch64-apple-ios-sim' if simulator else 'aarch64-apple-ios')
                swift = next(command for command in commands if 'swiftc' in command)
                self.assertIn('-whole-module-optimization', swift)
                self.assertIn(str(bridge / 'WidgetHostBridge.swift'), swift)
                self.assertTrue(any('clang' in command and str(bridge / 'WidgetHostBootstrap.c') in command
                                    for command in commands))
                key = 'CARGO_TARGET_' + report['target'].upper().replace('-', '_') + '_LINKER'
                self.assertEqual(Path(env[key]), Path(report['hostLinker']))
                response_file = Path(report['hostLinkResponse'])
                link = shlex.split(response_file.read_text())
                self.assertNotIn(str(library), link)
                self.assertIn('WidgetKit', link); self.assertIn('BackgroundTasks', link); self.assertIn('SwiftUI', link)
                self.assertEqual(set(env), {key})

    def test_generated_linker_keeps_dependency_cdylibs_clean_and_adds_host_only_to_final_runner(self):
        with tempfile.TemporaryDirectory(prefix='linker fixture ') as directory:
            root = Path(directory)
            manifest = root / 'Cargo.toml'
            manifest.write_text('[[bin]]\nname = "nsh-app-example-ios-app"\npath = "main.rs"\n')
            response = root / 'host objects.rsp'; response.write_text('WidgetHostBridge.o\n')
            capture = root / 'link.json'
            fake_linker = root / 'fake clang.py'
            fake_linker.write_text('import json, sys\nfrom pathlib import Path\n'
                                   + 'Path(' + repr(str(capture)) + ').write_text(json.dumps(sys.argv[1:]))\n')
            wrapper = package.widget_linker(root, response, sdk='iphoneos', runner_manifest=manifest,
                                             linker=[sys.executable, str(fake_linker)])
            executable = root / 'deps/nsh_app_example_ios_app-0123456789abcdef'
            rust_response = root / 'rust link.rsp'
            rust_response.write_text(shlex.join(['input.o', '-o', str(executable)]))
            cases = [(['input.o', '-dynamiclib', '-o', str(root / 'deps/liboxc_sourcemap-1234.dylib')], False),
                     (['input.o', '-dynamiclib', '-o', str(executable)], False),
                     (['input.o', '-o', str(root / 'deps/other_app-0123456789abcdef')], False),
                     (['input.o', '-o', str(executable)], True),
                     (['input.o', '-o', str(root / 'release/nsh-app-example-ios-app')], True),
                     (['@' + str(rust_response)], True)]
            env = dict(os.environ, LOKI_WIDGET_SIMULATOR_ENTITLEMENTS=str(root / 'simulator permissions.plist'))
            for arguments, applies in cases:
                with self.subTest(arguments=arguments):
                    subprocess.run([sys.executable, str(wrapper), *arguments], env=env,
                                   check=True, capture_output=True)
                    actual = json.loads(capture.read_text())
                    self.assertEqual(actual[:len(arguments)], arguments)
                    if applies:
                        self.assertEqual(actual[len(arguments):], ['@' + str(response),
                            '-Wl,-sectcreate,__TEXT,__entitlements,' + env['LOKI_WIDGET_SIMULATOR_ENTITLEMENTS']])
                    else:
                        self.assertEqual(actual, arguments)
            audit = [json.loads(line) for line in (root / 'link-events.jsonl').read_text().splitlines()]
            self.assertEqual([entry['hostShimAdded'] for entry in audit], [applies for _, applies in cases])


if __name__ == '__main__':
    unittest.main()
