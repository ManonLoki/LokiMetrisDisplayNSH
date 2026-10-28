"""Widgets retain reviewed identifiers, explicit shared grants and native code."""
# 引入标准库
from pathlib import Path
import importlib.util
import json
import os
import plistlib
import shlex
import subprocess
import sys
# 临时目录
import tempfile
# 测试框架
import unittest
# 补丁工具
from unittest.mock import patch

# 复用设备签名测试里的夹具
from test_apple_device_signing import signing, provision, macho, TEAM, BUNDLE, FINGERPRINT

# 仓库根目录
ROOT = Path(__file__).resolve().parents[3]
# 按路径加载打包脚本
spec = importlib.util.spec_from_file_location('widget_package', ROOT / 'tools/build/package_app.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


# 构造含一个小组件扩展的手机应用目录，两者版本与 App Group 一致
def application(root):
    phone = root / 'Phone.app'; widget = phone / 'PlugIns/Widgets.appex'
    widget.mkdir(parents=True); (phone / 'nsh').mkdir()
    (phone / 'nsh/bundle.nshc').write_bytes(b'unchanged NSH bytecode')
    for app, identifier in [(phone, 'old.phone'), (widget, 'old.phone.widgets')]:
        info = {'CFBundleIdentifier': identifier, 'CFBundleExecutable': 'App',
                'CFBundleVersion': '17', 'CFBundleShortVersionString': '1.0.9',
                'LokiWidgetAppGroup': 'group.old.phone'}
        # 扩展需声明 WidgetKit 扩展点
        if app == widget:
            info['NSExtension'] = {'NSExtensionPointIdentifier': 'com.apple.widgetkit-extension'}
        (app / 'Info.plist').write_bytes(plistlib.dumps(info))
        (app / 'App').write_bytes(macho())
    return phone, widget


# 小组件签名的测试
class WidgetSigning(unittest.TestCase):
    # 只请求显式声明且被授权的 App Group
    def test_only_explicit_authorized_groups_are_requested(self):
        value = provision()
        # 描述文件授权了两个组
        value['Entitlements']['com.apple.security.application-groups'] = ['group.' + BUNDLE, 'group.other.app']
        request = signing.development_entitlements(value, BUNDLE, TEAM, FINGERPRINT,
                                                   app_groups=['group.' + BUNDLE])
        # 只请求其中显式要求的那个
        self.assertEqual(request['com.apple.security.application-groups'], ['group.' + BUNDLE])
        # 未授权或格式非法的组都应被拒绝
        for groups in [['group.unauthorized.app'], ['invalid group']]:
            with self.subTest(groups=groups), self.assertRaises(RuntimeError):
                signing.development_entitlements(value, BUNDLE, TEAM, FINGERPRINT, app_groups=groups)

    # 签名计划会校验小组件的描述文件、App Group、平台、标识与版本
    def test_plan_validates_widget_profile_group_platform_identity_and_version(self):
        with tempfile.TemporaryDirectory() as directory:
            phone, widget = application(Path(directory))
            widget_info = plistlib.loads((widget / 'Info.plist').read_bytes())
            value = provision()
            value['Entitlements']['com.apple.security.application-groups'] = ['group.' + BUNDLE]
            # 辅助：用给定的小组件描述文件生成计划
            def plan(widget_profile=Path('widget.mobileprovision')):
                return signing.plan(phone, BUNDLE, TEAM, FINGERPRINT, Path('phone.mobileprovision'),
                                    None, 'phone', widget_profile=widget_profile, app_group='group.' + BUNDLE)
            # 替换证书、描述文件与平台检查
            with patch.object(signing, 'certificate', return_value=TEAM), \
                 patch.object(signing, 'profile', return_value=value), \
                 patch.object(signing, 'macho_platforms', return_value={2}):
                report = plan()
                self.assertEqual(report['widgetBundleId'], BUNDLE + '.widgets')
                # 手机与小组件请求的 App Group 一致
                self.assertEqual(report['phoneProfile']['entitlements']['com.apple.security.application-groups'],
                                 report['widgetProfile']['entitlements']['com.apple.security.application-groups'])
                # 没有手表
                self.assertNotIn('watchRelativePath', report)
                # 缺少小组件描述文件应报错
                with self.assertRaisesRegex(RuntimeError, 'own development profile'):
                    plan(None)
                # 标识、App Group 或版本被改动都应报错
                for key, invalid in [('CFBundleIdentifier', 'unknown.extension'),
                                     ('LokiWidgetAppGroup', 'group.other.app'), ('CFBundleVersion', '16')]:
                    modified = dict(widget_info); modified[key] = invalid
                    (widget / 'Info.plist').write_bytes(plistlib.dumps(modified))
                    with self.subTest(key=key), self.assertRaises(RuntimeError):
                        plan()
                # 恢复后再测平台不匹配
                (widget / 'Info.plist').write_bytes(plistlib.dumps(widget_info))
                # 小组件被判为模拟器平台
                with patch.object(signing, 'macho_platforms', side_effect=lambda path: {7} if path.parent == widget.resolve() else {2}), \
                     self.assertRaisesRegex(RuntimeError, 'device platform'):
                    plan()
                # 描述文件不再授权任何 App Group
                value['Entitlements']['com.apple.security.application-groups'] = []
                with self.assertRaisesRegex(RuntimeError, 'App Group'):
                    plan()

    # 小组件先于手机签名，App Group 被重映射，源包保持不变
    def test_widget_is_signed_before_phone_with_remapped_group_and_immutable_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); phone, widget = application(root)
            value = provision()
            value['Entitlements']['com.apple.security.application-groups'] = ['group.' + BUNDLE]
            profile = root / 'profile.mobileprovision'; profile.write_bytes(b'profile fixture')
            # 替换平台检测后生成签名计划
            with patch.object(signing, 'certificate', return_value=TEAM), \
                 patch.object(signing, 'profile', return_value=value), \
                 patch.object(signing, 'macho_platforms', return_value={2}):
                # 生成签名计划
                report = signing.plan(phone, BUNDLE, TEAM, FINGERPRINT, profile, None, 'phone',
                                      widget_profile=profile, app_group='group.' + BUNDLE)
            # 输出路径与已签名列表
            output = root / 'signed/Phone.app'; signed = []
            # 假的 codesign：记录签名顺序，返回对应目标的权限
            def codesign(arguments):
                if '--force' in arguments:
                    signed.append(Path(arguments[-1])); return b''
                if '--display' in arguments:
                    entitlements = report['phoneProfile' if Path(arguments[-1]) == output else 'widgetProfile']['entitlements']
                    return plistlib.dumps(entitlements)
                return b''
            with patch.object(signing, 'command', side_effect=codesign):
                # 对副本签名
                signing.sign_copy(phone, output, report, profile, widget_profile=profile)
            # 先签扩展，后签手机
            self.assertEqual(signed, [output / 'PlugIns/Widgets.appex', output])
            # 副本里两个标识都已更新，App Group 一致
            for target, identifier in [(output, BUNDLE), (output / 'PlugIns/Widgets.appex', BUNDLE + '.widgets')]:
                info = plistlib.loads((target / 'Info.plist').read_bytes())
                self.assertEqual(info['CFBundleIdentifier'], identifier)
                self.assertEqual(info['LokiWidgetAppGroup'], 'group.' + BUNDLE)
            # 源包的标识不变
            self.assertEqual(plistlib.loads((phone / 'Info.plist').read_bytes())['CFBundleIdentifier'], 'old.phone')
            # 指令均被保留
            self.assertTrue(report['phoneInstructionsPreserved'] and report['widgetInstructionsPreserved'])


# 小组件构建输入的测试
class WidgetBuildInputs(unittest.TestCase):
    # 原生支持使用按目标限定的链接方式，且不会给手机再链接一份 Rust 库
    def test_native_support_uses_target_scoped_linking_and_no_second_phone_rust_library(self):
        # 真机与模拟器各测一次
        for simulator in [False, True]:
            with self.subTest(simulator=simulator), tempfile.TemporaryDirectory(prefix='widget fixture ') as directory:
                root = Path(directory)
                # 用临时目录里的假源文件代替真实源码
                shared = root / 'platform/ios/shared'; shared.mkdir(parents=True)
                bridge = root / 'platform/ios/bridge'; bridge.mkdir()
                for name in ['LokiCore', 'PlatformRules', 'SharedSupport', 'WidgetContainer', 'WidgetRefresh', 'WidgetRefreshCommitGate']:
                    (shared / (name + '.swift')).write_text('// Swift fixture ' + name)
                for name in ['WidgetHostBridge.swift', 'WidgetHostBootstrap.c']:
                    (bridge / name).write_text('// native fixture ' + name)
                # 记录被调用的命令
                commands = []
                # 假的 run：记录命令，并按命令类型造出假的产物
                def run(arguments, **kwargs):
                    command = [str(value) for value in arguments]; commands.append(command)
                    # cargo：造出静态库
                    if command[0] == 'cargo':
                        target = command[command.index('--target') + 1]
                        library = root / 'target/widget-core' / target / 'release/libloki_widget_core.a'
                        library.parent.mkdir(parents=True); library.write_bytes(b'static core fixture')
                    # 其他编译命令：造出目标文件
                    else:
                        Path(command[command.index('-o') + 1]).write_bytes(b'native object fixture')
                env = {}
                with patch.object(package, 'ROOT', root), patch.object(package, 'run', side_effect=run):
                    library, report = package.build_widget_support(env, simulator=simulator,
                        sdk_path=Path('/fixture SDK'), developer=Path('/fixture Xcode'), deployment='15.0')
                # cargo 使用锁定的依赖
                self.assertIn('--locked', commands[0])
                self.assertEqual(report['target'], 'aarch64-apple-ios-sim' if simulator else 'aarch64-apple-ios')
                # Swift 使用整模块优化并包含桥接源码
                swift = next(command for command in commands if 'swiftc' in command)
                self.assertIn('-whole-module-optimization', swift)
                self.assertIn(str(bridge / 'WidgetHostBridge.swift'), swift)
                # 并编译 C 引导
                self.assertTrue(any('clang' in command and str(bridge / 'WidgetHostBootstrap.c') in command
                                    for command in commands))
                # 链接器通过按目标限定的环境变量指定
                key = 'CARGO_TARGET_' + report['target'].upper().replace('-', '_') + '_LINKER'
                self.assertEqual(Path(env[key]), Path(report['hostLinker']))
                # 响应文件
                response_file = Path(report['hostLinkResponse'])
                link = shlex.split(response_file.read_text())
                # 不把 Rust 静态库链接进手机
                self.assertNotIn(str(library), link)
                # 需要的系统框架
                self.assertIn('WidgetKit', link); self.assertIn('BackgroundTasks', link); self.assertIn('SwiftUI', link)
                # 环境里只新增这一个变量
                self.assertEqual(set(env), {key})

    # 生成的链接器包装不碰依赖的动态库，只把宿主目标文件加到最终运行器
    def test_generated_linker_keeps_dependency_cdylibs_clean_and_adds_host_only_to_final_runner(self):
        with tempfile.TemporaryDirectory(prefix='linker fixture ') as directory:
            root = Path(directory)
            # 声明唯一的可执行文件名
            manifest = root / 'Cargo.toml'
            manifest.write_text('[[bin]]\nname = "nsh-app-example-ios-app"\npath = "main.rs"\n')
            response = root / 'host objects.rsp'; response.write_text('WidgetHostBridge.o\n')
            capture = root / 'link.json'
            # 假的链接器：把收到的参数写入文件
            fake_linker = root / 'fake clang.py'
            fake_linker.write_text('import json, sys\nfrom pathlib import Path\n'
                                   + 'Path(' + repr(str(capture)) + ').write_text(json.dumps(sys.argv[1:]))\n')
            # 生成包装脚本
            wrapper = package.widget_linker(root, response, sdk='iphoneos', runner_manifest=manifest,
                                             linker=[sys.executable, str(fake_linker)])
            # 带哈希后缀的最终运行器名
            executable = root / 'deps/nsh_app_example_ios_app-0123456789abcdef'
            # Rust 自己的响应文件也要被展开识别
            rust_response = root / 'rust link.rsp'
            rust_response.write_text(shlex.join(['input.o', '-o', str(executable)]))
            # 各种链接场景及是否应追加宿主文件：动态库不追加，其他应用不追加，最终运行器追加
            cases = [(['input.o', '-dynamiclib', '-o', str(root / 'deps/liboxc_sourcemap-1234.dylib')], False),
                     (['input.o', '-dynamiclib', '-o', str(executable)], False),
                     (['input.o', '-o', str(root / 'deps/other_app-0123456789abcdef')], False),
                     (['input.o', '-o', str(executable)], True),
                     (['input.o', '-o', str(root / 'release/nsh-app-example-ios-app')], True),
                     (['@' + str(rust_response)], True)]
            env = dict(os.environ, LOKI_WIDGET_SIMULATOR_ENTITLEMENTS=str(root / 'simulator permissions.plist'))
            for arguments, applies in cases:
                # 每组参数一个子用例
                with self.subTest(arguments=arguments):
                    # 执行包装脚本
                    subprocess.run([sys.executable, str(wrapper), *arguments], env=env,
                                   check=True, capture_output=True)
                    # 读取捕获到的参数
                    actual = json.loads(capture.read_text())
                    # 原有参数不变
                    self.assertEqual(actual[:len(arguments)], arguments)
                    # 需要时追加响应文件与模拟器权限
                    if applies:
                        self.assertEqual(actual[len(arguments):], ['@' + str(response),
                            '-Wl,-sectcreate,__TEXT,__entitlements,' + env['LOKI_WIDGET_SIMULATOR_ENTITLEMENTS']])
                    # 否则参数完全不变
                    else:
                        self.assertEqual(actual, arguments)
            # 审计日志逐条记录是否追加
            audit = [json.loads(line) for line in (root / 'link-events.jsonl').read_text().splitlines()]
            self.assertEqual([entry['hostShimAdded'] for entry in audit], [applies for _, applies in cases])


# 作为脚本运行时
if __name__ == '__main__':
    # 运行全部用例
    unittest.main()
