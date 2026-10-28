"""The companion archive preserves the NSH runner and Xcode's Watch build graph."""
# 引入标准库与 unittest.mock
import importlib.util
import json
from pathlib import Path
import plistlib
import struct
import tempfile
import unittest
from unittest.mock import patch

# 仓库根目录
ROOT = Path(__file__).resolve().parents[3]
# 按路径加载组装脚本
spec = importlib.util.spec_from_file_location('companion', ROOT / 'tools/build/package_apple_companion.py')
companion = importlib.util.module_from_spec(spec)
spec.loader.exec_module(companion)


# 构造最小 Mach-O：CPU 头相同（ARM64），仅 LC_BUILD_VERSION 的平台不同
def macho(platform):
    # 相同的 ARM64 CPU 头部,但 LC_BUILD_VERSION 平台不同。
    header = struct.pack('<8I', 0xfeedfacf, 0x0100000c, 0, 2, 1, 24, 0, 0)
    return header + struct.pack('<6I', 0x32, 24, platform, 0, 0, 0)


# 配套应用打包的测试
class CompanionPackaging(unittest.TestCase):
    # 重新组装时保留手机应用，并从副本中移除先前的配套应用
    def test_reassembly_preserves_phone_and_removes_previous_companion_from_copy(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); app = root / 'Phone.app'; app.mkdir()
            (app / 'App').write_bytes(b'prelinked NSH machine code')
            # 构造一个旧的手表子应用（放在非标准目录里也要被找到）
            old = app / 'Custom Embed/Old.app'; old.mkdir(parents=True)
            (old / 'Info.plist').write_bytes(plistlib.dumps({'WKCompanionAppBundleIdentifier': 'org.example.phone'}))
            (old / 'old-resource').write_bytes(b'stale Watch resource')
            # 以及旧的小组件扩展
            widget = app / 'PlugIns/Old.appex'; widget.mkdir(parents=True)
            (widget / 'Info.plist').write_bytes(plistlib.dumps({'CFBundleIdentifier': 'org.example.phone.widgets'}))
            prepared = root / 'Prelinked/Phone.app'
            companion.prepare_app(app, prepared, 'org.example.phone')
            # 手机可执行文件原样保留
            self.assertEqual((prepared / 'App').read_bytes(), (app / 'App').read_bytes())
            # 源应用里的旧子应用不被动
            self.assertTrue(old.is_dir())
            # 副本里已移除旧手表与旧扩展
            self.assertFalse((prepared / old.relative_to(app)).exists())
            self.assertFalse((prepared / widget.relative_to(app)).exists())
            self.assertTrue(widget.is_dir())

    # 源码快照会跟踪代码变化，但忽略 Xcode 的用户状态
    def test_watch_source_snapshot_tracks_code_but_ignores_xcode_user_state(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); project = root / 'Watch.xcodeproj'; project.mkdir()
            source = root / 'App.swift'; source.write_text('first source')
            before = companion.watch_source_hashes(project)
            # 新增用户状态文件：摘要不应变化
            user = project / 'xcuserdata'; user.mkdir()
            (user / 'window-state').write_text('editor state')
            self.assertEqual(companion.watch_source_hashes(project), before)
            # 改动源码：摘要必须变化
            source.write_text('updated source')
            self.assertNotEqual(companion.watch_source_hashes(project), before)

    # 平台取自每个 Mach-O 切片的 LC_BUILD_VERSION
    def test_platform_comes_from_build_version_in_every_macho_slice(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'App'
            # 7 是 iOS 模拟器
            path.write_bytes(macho(7))
            self.assertEqual(companion.macho_platforms(path), {7})
            # 2 是 iOS 真机
            path.write_bytes(macho(2))
            self.assertEqual(companion.macho_platforms(path), {2})
            # 通用二进制含两个切片，应得到两个平台
            first, second = macho(7), macho(2)
            fat = struct.pack('>2I', 0xcafebabe, 2)
            fat += struct.pack('>5I', 0x0100000c, 0, 48, len(first), 0)
            fat += struct.pack('>5I', 0x0100000c, 0, 48 + len(first), len(second), 0)
            path.write_bytes(fat + first + second)
            self.assertEqual(companion.macho_platforms(path), {2, 7})

    # 真机归档在构建之前就拒绝 ARM64 的模拟器可执行文件
    def test_device_archive_rejects_an_arm64_simulator_before_building(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); app = root / 'App.app'; app.mkdir()
            (app / 'Info.plist').write_bytes(plistlib.dumps({'CFBundleExecutable': 'App'}))
            (app / 'App').write_bytes(macho(7))
            with patch.object(companion, 'run') as build:
                with self.assertRaisesRegex(RuntimeError, 'requested iOS platform'):
                    companion.assemble(app, root / 'Device.xcarchive', simulator=False)
                # 不应调用构建
                build.assert_not_called()

    # 组装工程不含手机源码，并通过跨工程引用嵌入手表
    def test_assembly_has_no_phone_source_and_uses_cross_project_watch_embed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); app = root / 'App.app'; app.mkdir()
            (app / 'Info.plist').write_bytes(plistlib.dumps({
                'CFBundleIdentifier': 'org.example.app', 'CFBundleExecutable': 'App'}))
            watch = root / 'Watch.xcodeproj'
            # 用假的目标与部署版本，并让序列化直接输出 JSON，便于断言
            with patch.object(companion, 'watch_target', return_value=('WATCH_TARGET', 'WATCH_PRODUCT', 'Watch.app')), \
                 patch.object(companion, 'deployment_target', return_value='15.0'), \
                 patch.object(companion, 'project_value', side_effect=json.dumps):
                project = companion.generate_project(root / 'assembly', app, watch, simulator=True,
                                                       team='TEAM', with_widgets=False)
            # 读回工程对象
            objects = json.loads((project / 'project.pbxproj').read_text().split('\n', 1)[1])['objects']
            # 宿主目标
            host = next(value for value in objects.values() if value['isa'] == 'PBXNativeTarget')
            # 构建阶段
            phases = [objects[key] for key in host['buildPhases']]
            # 只有“脚本阶段 + 拷贝文件阶段”，没有源码或框架阶段
            self.assertEqual([value['isa'] for value in phases], ['PBXShellScriptBuildPhase', 'PBXCopyFilesBuildPhase'])
            # 脚本阶段用 ditto 原样复制预链接的应用
            self.assertIn('ditto', phases[0]['shellScript'])
            # 手表嵌入到 Watch 目录（目标子目录 16）
            self.assertEqual(phases[1]['dstPath'], '$(CONTENTS_FOLDER_PATH)/Watch')
            self.assertEqual(phases[1]['dstSubfolderSpec'], 16)
            # 依赖与代理指向手表目标和产物
            dependency = objects[host['dependencies'][0]]
            proxy = objects[dependency['targetProxy']]
            self.assertEqual(proxy['remoteGlobalIDString'], 'WATCH_TARGET')
            product = objects[objects[phases[1]['files'][0]]['fileRef']]
            self.assertEqual(objects[product['remoteRef']]['remoteGlobalIDString'], 'WATCH_PRODUCT')
            self.assertEqual(objects[proxy['containerPortal']]['path'], str(watch))

    # 小组件默认嵌入，手表可以省略
    def test_widgets_embed_by_default_and_watch_can_be_omitted(self):
        for with_watch in [False, True]:
            with self.subTest(with_watch=with_watch), tempfile.TemporaryDirectory() as directory:
                root = Path(directory); app = root / 'App.app'; app.mkdir()
                (app / 'Info.plist').write_bytes(plistlib.dumps({
                    'CFBundleIdentifier': 'org.example.app', 'CFBundleExecutable': 'App',
                    'LokiWidgetAppGroup': 'group.org.example.shared'}))
                # 替换构建依赖后生成工程
                with patch.object(companion, 'watch_target', return_value=('WATCH_TARGET', 'WATCH_PRODUCT', 'Watch.app')), \
                     patch.object(companion, 'widget_target', return_value=('WIDGET_TARGET', 'WIDGET_PRODUCT', 'Widgets.appex')), \
                     patch.object(companion, 'deployment_target', return_value='15.0'), \
                     patch.object(companion, 'project_value', side_effect=json.dumps):
                    # 生成工程
                    project = companion.generate_project(root / 'assembly', app, simulator=True,
                                                         team='TEAM', with_watch=with_watch)
                # 读取对象表
                objects = json.loads((project / 'project.pbxproj').read_text().split('\n', 1)[1])['objects']
                # 找到宿主 target
                host = next(value for value in objects.values() if value['isa'] == 'PBXNativeTarget')
                # 取其构建阶段
                phases = [objects[key] for key in host['buildPhases']]
                # 不应有源码或框架构建阶段
                self.assertFalse(any(value['isa'] in {'PBXSourcesBuildPhase', 'PBXFrameworksBuildPhase'}
                                     for value in phases))
                self.assertIn('\n', phases[0]['shellScript'])
                # 小组件通过“Embed App Extensions”阶段嵌入（目标子目录 13）
                widget = next(value for value in phases if value.get('name') == 'Embed App Extensions')
                self.assertEqual(widget['dstSubfolderSpec'], 13)
                reference = objects[objects[widget['files'][0]]['fileRef']]
                self.assertEqual(objects[reference['remoteRef']]['remoteGlobalIDString'], 'WIDGET_PRODUCT')
                dependencies = [objects[objects[key]['targetProxy']]['remoteGlobalIDString']
                                for key in host['dependencies']]
                self.assertIn('WIDGET_TARGET', dependencies)
                # 手表目标是否出现取决于参数
                self.assertEqual('WATCH_TARGET' in dependencies, with_watch)
                # App Group 沿用应用声明的值
                self.assertEqual(plistlib.loads((project.parent / 'Info.plist').read_bytes())['LokiWidgetAppGroup'],
                                 'group.org.example.shared')

    # 内嵌应用的版本必须与手机一致
    def test_embedded_versions_must_match_phone(self):
        phone = {'CFBundleVersion': '17', 'CFBundleShortVersionString': '1.0.9'}
        # 一致则通过
        companion.verify_versions(phone, dict(phone))
        # 构建号或营销版本任一不同都应报错
        for key, invalid in [('CFBundleVersion', '16'), ('CFBundleShortVersionString', '1.0.8')]:
            child = dict(phone); child[key] = invalid
            with self.subTest(key=key), self.assertRaisesRegex(RuntimeError, 'versions differ'):
                companion.verify_versions(phone, child)

    # 归档中的应用路径取自 Xcode 的元数据，并拒绝逃出归档的路径
    def test_archive_application_uses_xcode_metadata_and_rejects_escape(self):
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory)
            def write_path(value):
                (archive / 'Info.plist').write_bytes(plistlib.dumps({'ApplicationProperties': {'ApplicationPath': value}}))
            # 正常路径（含空格）
            write_path('Applications/Generated Name.app')
            self.assertEqual(companion.archived_application(archive), archive / 'Products/Applications/Generated Name.app')
            # 绝对路径与 .. 都应被拒绝
            for invalid in ['../../outside.app', '/outside.app']:
                write_path(invalid)
                with self.assertRaisesRegex(RuntimeError, 'Invalid application path'):
                    companion.archived_application(archive)


# 作为脚本运行时
if __name__ == '__main__':
    # 运行全部用例
    unittest.main()
