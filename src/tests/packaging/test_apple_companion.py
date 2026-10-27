"""The companion archive preserves the NSH runner and Xcode's Watch build graph."""
import importlib.util
import json
from pathlib import Path
import plistlib
import struct
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location('companion', ROOT / 'tools/build/package_apple_companion.py')
companion = importlib.util.module_from_spec(spec)
spec.loader.exec_module(companion)


def macho(platform):
    # Identical ARM64 CPU headers with different LC_BUILD_VERSION platforms.
    header = struct.pack('<8I', 0xfeedfacf, 0x0100000c, 0, 2, 1, 24, 0, 0)
    return header + struct.pack('<6I', 0x32, 24, platform, 0, 0, 0)


class CompanionPackaging(unittest.TestCase):
    def test_reassembly_preserves_phone_and_removes_previous_companion_from_copy(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); app = root / 'Phone.app'; app.mkdir()
            (app / 'App').write_bytes(b'prelinked NSH machine code')
            old = app / 'Custom Embed/Old.app'; old.mkdir(parents=True)
            (old / 'Info.plist').write_bytes(plistlib.dumps({'WKCompanionAppBundleIdentifier': 'org.example.phone'}))
            (old / 'old-resource').write_bytes(b'stale Watch resource')
            prepared = root / 'Prelinked/Phone.app'
            companion.prepare_app(app, prepared, 'org.example.phone')
            self.assertEqual((prepared / 'App').read_bytes(), (app / 'App').read_bytes())
            self.assertTrue(old.is_dir())
            self.assertFalse((prepared / old.relative_to(app)).exists())

    def test_watch_source_snapshot_tracks_code_but_ignores_xcode_user_state(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); project = root / 'Watch.xcodeproj'; project.mkdir()
            source = root / 'App.swift'; source.write_text('first source')
            before = companion.watch_source_hashes(project)
            user = project / 'xcuserdata'; user.mkdir()
            (user / 'window-state').write_text('editor state')
            self.assertEqual(companion.watch_source_hashes(project), before)
            source.write_text('updated source')
            self.assertNotEqual(companion.watch_source_hashes(project), before)

    def test_platform_comes_from_build_version_in_every_macho_slice(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'App'
            path.write_bytes(macho(7))
            self.assertEqual(companion.macho_platforms(path), {7})
            path.write_bytes(macho(2))
            self.assertEqual(companion.macho_platforms(path), {2})
            first, second = macho(7), macho(2)
            fat = struct.pack('>2I', 0xcafebabe, 2)
            fat += struct.pack('>5I', 0x0100000c, 0, 48, len(first), 0)
            fat += struct.pack('>5I', 0x0100000c, 0, 48 + len(first), len(second), 0)
            path.write_bytes(fat + first + second)
            self.assertEqual(companion.macho_platforms(path), {2, 7})

    def test_device_archive_rejects_an_arm64_simulator_before_building(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); app = root / 'App.app'; app.mkdir()
            (app / 'Info.plist').write_bytes(plistlib.dumps({'CFBundleExecutable': 'App'}))
            (app / 'App').write_bytes(macho(7))
            with patch.object(companion, 'run') as build:
                with self.assertRaisesRegex(RuntimeError, 'requested iOS platform'):
                    companion.assemble(app, root / 'Device.xcarchive', simulator=False)
                build.assert_not_called()

    def test_assembly_has_no_phone_source_and_uses_cross_project_watch_embed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); app = root / 'App.app'; app.mkdir()
            (app / 'Info.plist').write_bytes(plistlib.dumps({
                'CFBundleIdentifier': 'org.example.app', 'CFBundleExecutable': 'App'}))
            watch = root / 'Watch.xcodeproj'
            with patch.object(companion, 'watch_target', return_value=('WATCH_TARGET', 'WATCH_PRODUCT', 'Watch.app')), \
                 patch.object(companion, 'deployment_target', return_value='15.0'), \
                 patch.object(companion, 'project_value', side_effect=json.dumps):
                project = companion.generate_project(root / 'assembly', app, watch, simulator=True, team='TEAM')
            objects = json.loads((project / 'project.pbxproj').read_text().split('\n', 1)[1])['objects']
            host = next(value for value in objects.values() if value['isa'] == 'PBXNativeTarget')
            phases = [objects[key] for key in host['buildPhases']]
            self.assertEqual([value['isa'] for value in phases], ['PBXShellScriptBuildPhase', 'PBXCopyFilesBuildPhase'])
            self.assertIn('ditto', phases[0]['shellScript'])
            self.assertEqual(phases[1]['dstPath'], '$(CONTENTS_FOLDER_PATH)/Watch')
            self.assertEqual(phases[1]['dstSubfolderSpec'], 16)
            dependency = objects[host['dependencies'][0]]
            proxy = objects[dependency['targetProxy']]
            self.assertEqual(proxy['remoteGlobalIDString'], 'WATCH_TARGET')
            product = objects[objects[phases[1]['files'][0]]['fileRef']]
            self.assertEqual(objects[product['remoteRef']]['remoteGlobalIDString'], 'WATCH_PRODUCT')
            self.assertEqual(objects[proxy['containerPortal']]['path'], str(watch))

    def test_archive_application_uses_xcode_metadata_and_rejects_escape(self):
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory)
            def write_path(value):
                (archive / 'Info.plist').write_bytes(plistlib.dumps({'ApplicationProperties': {'ApplicationPath': value}}))
            write_path('Applications/Generated Name.app')
            self.assertEqual(companion.archived_application(archive), archive / 'Products/Applications/Generated Name.app')
            for invalid in ['../../outside.app', '/outside.app']:
                write_path(invalid)
                with self.assertRaisesRegex(RuntimeError, 'Invalid application path'):
                    companion.archived_application(archive)


if __name__ == '__main__':
    unittest.main()
