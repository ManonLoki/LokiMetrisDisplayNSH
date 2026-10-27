"""Development signing keeps identity grants and prelinked instructions intact."""
from datetime import datetime, timedelta, timezone
import hashlib
import importlib.util
from pathlib import Path
import plistlib
import struct
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools/build'))
spec = importlib.util.spec_from_file_location('sign_device', ROOT / 'tools/build/sign_apple_device.py')
signing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(signing)
TEAM = 'ABCDEFGHIJ'
BUNDLE = 'org.example.phone'
CERT = b'public certificate fixture'
FINGERPRINT = hashlib.sha1(CERT).hexdigest().upper()


def provision(prefix=TEAM, grant=None):
    return {'UUID': 'profile', 'Name': 'development', 'TeamIdentifier': [TEAM],
            'ApplicationIdentifierPrefix': [prefix], 'ProvisionedDevices': ['phone'],
            'DeveloperCertificates': [CERT],
            'ExpirationDate': datetime.now(timezone.utc) + timedelta(days=1),
            'Entitlements': {'application-identifier': grant or prefix + '.*',
                             'keychain-access-groups': [prefix + '.*'],
                             'get-task-allow': True,
                             'com.apple.security.application-groups': ['retired.widget.group']}}


def macho(body=b'original instructions', signature=b'old signature'):
    header = struct.pack('<8I', 0xfeedfacf, 0x0100000c, 0, 2, 1, 152, 0, 0)
    segment = struct.pack('<II16s4Q4I', 0x19, 152, b'__TEXT', 0, 0, 0, 0, 7, 5, 1, 0)
    section = struct.pack('<16s16sQQ8I', b'__text', b'__TEXT', 0, len(body), 256, 0, 0, 0, 0, 0, 0, 0)
    return (header + segment + section).ljust(256, b'\0') + body + signature


class DeviceSigning(unittest.TestCase):
    def test_only_minimal_private_keychain_entitlements_are_requested(self):
        request = signing.development_entitlements(provision(), BUNDLE, TEAM, FINGERPRINT, device='phone')
        self.assertEqual(request, {'application-identifier': TEAM + '.' + BUNDLE,
                                  'com.apple.developer.team-identifier': TEAM,
                                  'get-task-allow': True,
                                  'keychain-access-groups': [TEAM + '.' + BUNDLE]})

    def test_legacy_app_prefix_is_kept_independently_of_certificate_team(self):
        request = signing.development_entitlements(provision(prefix='OLDPREFIX1'), BUNDLE,
                                                  TEAM, FINGERPRINT, device='phone')
        self.assertEqual(request['application-identifier'], 'OLDPREFIX1.' + BUNDLE)
        self.assertEqual(request['com.apple.developer.team-identifier'], TEAM)

    def test_profile_rejects_wrong_device_bundle_team_certificate_or_expiry(self):
        cases = [({'ProvisionedDevices': ['other']}, {}),
                 ({'TeamIdentifier': ['OTHERTEAM1']}, {}),
                 ({'DeveloperCertificates': [b'other certificate']}, {}),
                 ({'ExpirationDate': datetime.now(timezone.utc) - timedelta(days=1)}, {}),
                 ({}, {'application-identifier': TEAM + '.another.bundle'}),
                 ({}, {'get-task-allow': False}),
                 ({}, {'keychain-access-groups': ['OTHERTEAM1.*']})]
        for top, entitlements in cases:
            value = provision(); value.update(top); value['Entitlements'].update(entitlements)
            with self.subTest(top=top, entitlements=entitlements), self.assertRaises(RuntimeError):
                signing.development_entitlements(value, BUNDLE, TEAM, FINGERPRINT, device='phone')

    def test_code_hash_ignores_signing_blob_but_detects_changed_instructions(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'App'; path.write_bytes(macho())
            before = signing.instructions(path)
            path.write_bytes(macho(signature=b'new DER signature'))
            self.assertEqual(signing.instructions(path), before)
            path.write_bytes(macho(body=b'changed instructions'))
            self.assertNotEqual(signing.instructions(path), before)

    def test_cli_rejects_nested_output_before_writing_even_in_plan_only_mode(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); source = root / 'Source.app'; source.mkdir()
            marker = source / 'original'; marker.write_bytes(b'keep original')
            alias = root / 'source-alias'; alias.symlink_to(source, target_is_directory=True)
            for parent in [source, alias]:
                for execute in [[], ['--execute']]:
                    arguments = ['sign_apple_device.py', '--app', str(source),
                                 '--output', str(parent / 'Nested.app'), '--bundle-id', BUNDLE,
                                 '--team', TEAM, '--identity', FINGERPRINT,
                                 '--phone-profile', 'phone.mobileprovision',
                                 '--watch-profile', 'watch.mobileprovision', '--device', 'phone', *execute]
                    with self.subTest(parent=parent, execute=execute), \
                            patch.object(sys, 'argv', arguments), patch.object(signing, 'plan') as plan, \
                            self.assertRaisesRegex(RuntimeError, 'outside the source bundle'):
                        signing.main()
                    plan.assert_not_called()
                    self.assertEqual(list(source.iterdir()), [marker])
                    self.assertEqual(marker.read_bytes(), b'keep original')

    def test_signing_copy_updates_both_ids_signs_child_first_and_preserves_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); source = root / 'Source.app'; watch = source / 'Watch/Watch.app'
            watch.mkdir(parents=True); (source / 'nsh').mkdir()
            (source / 'nsh/bundle.nshc').write_bytes(b'unchanged NSH bytecode')
            for app, bundle in [(source, 'old.phone'), (watch, 'old.phone.watchkitapp')]:
                (app / 'App').write_bytes(macho())
                info = {'CFBundleExecutable': 'App', 'CFBundleIdentifier': bundle}
                if app == watch: info['WKCompanionAppBundleIdentifier'] = 'old.phone'
                (app / 'Info.plist').write_bytes(plistlib.dumps(info))
            profile = root / 'profile.mobileprovision'; profile.write_bytes(b'public signed profile')
            phone_ent = signing.development_entitlements(provision(), BUNDLE, TEAM, FINGERPRINT)
            watch_ent = signing.development_entitlements(provision(), BUNDLE + '.watchkitapp', TEAM, FINGERPRINT)
            report = {'bundleId': BUNDLE, 'watchBundleId': BUNDLE + '.watchkitapp',
                      'watchRelativePath': 'Watch/Watch.app', 'certificateSha1': FINGERPRINT,
                      'phoneProfile': {'entitlements': phone_ent}, 'watchProfile': {'entitlements': watch_ent},
                      'phoneInstructions': signing.instructions(source / 'App'),
                      'watchInstructions': signing.instructions(watch / 'App'),
                      'nshBundleSha256': signing.nsh_bundle_hash(source)}
            output = root / 'signed/Phone.app'; signed = []
            def codesign(arguments):
                if '--force' in arguments:
                    signed.append(Path(arguments[-1])); return b''
                if '--display' in arguments:
                    return plistlib.dumps(watch_ent if Path(arguments[-1]) == output / 'Watch/Watch.app' else phone_ent)
                return b''
            with patch.object(signing, 'command', side_effect=codesign):
                signing.sign_copy(source, output, report, profile, profile)
            self.assertEqual(signed, [output / 'Watch/Watch.app', output])
            self.assertEqual(plistlib.loads((source / 'Info.plist').read_bytes())['CFBundleIdentifier'], 'old.phone')
            actual = plistlib.loads((output / 'Watch/Watch.app/Info.plist').read_bytes())
            self.assertEqual(actual['WKCompanionAppBundleIdentifier'], BUNDLE)
            self.assertTrue(report['phoneInstructionsPreserved'] and report['watchInstructionsPreserved'])
            with self.assertRaisesRegex(RuntimeError, 'Output must be a new application'):
                signing.sign_copy(source, output, report, profile, profile)


if __name__ == '__main__':
    unittest.main()
