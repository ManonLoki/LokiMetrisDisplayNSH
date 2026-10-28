"""Official NSH runners preserve application identity and app-owned runtime adapters."""
# 引入标准库与 unittest.mock
import argparse
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import plistlib
import struct
# 系统模块
import sys
# 归档
import tarfile
# 临时目录
import tempfile
# 测试框架
import unittest
# 压缩包
import zipfile
# 重定向标准输出
from contextlib import redirect_stdout
# 补丁工具
from unittest.mock import patch

# 仓库根目录
ROOT = Path(__file__).resolve().parents[3]
# 让测试能导入构建脚本
sys.path.insert(0, str(ROOT / 'tools/build'))
# 引入清单改写函数与常量
from android_manifest import APPLICATION, ACTIVITY, PERMISSIONS, RUNNER_ACTIVITY, component_manifest, lan_permissions, pool, NO_INDEX, strings
# 引入 Java 桥接构建工具
import build_android_bridge as java_tools
# 引入 Android 打包脚本
import package_android as android_package

# 按路径加载总打包脚本
spec = importlib.util.spec_from_file_location('package_app', ROOT / 'tools/build/package_app.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


# 构造一份编译后风格的二进制清单：含启动器、导出标记、Rust 元数据和一个无关 Activity
def manifest_fixture(*, utf8, named_application, runner=RUNNER_ACTIVITY):
    """A compiled-style manifest with launcher/exported/Rust metadata and an unrelated activity."""
    namespace = 'http://schemas.android.com/apk/res/android'
    # 字符串池内容
    values = ['manifest', 'application', 'activity', 'uses-sdk', 'meta-data', 'intent-filter',
              'action', 'category', 'uses-permission', 'name', 'package', 'targetSdkVersion',
              'exported', 'value', 'theme', 'label', 'android', namespace, runner,
              'org.example.product', 'org.example.OriginalApplication', 'org.example.HelpActivity',
              'com.longbridge.nsh.RUST', 'android.intent.action.MAIN',
              'android.intent.category.LAUNCHER', 'android.permission.INTERNET', '诡秘神谕']
    index = values.index
    # 构造一个属性；kind 为 3 表示字符串，其余为原始值
    def attribute(name, value, kind=3, android=True):
        value = index(value) if kind == 3 else value
        return struct.pack('<IIIHBBI', index(namespace) if android else NO_INDEX,
                           index(name), value if kind == 3 else NO_INDEX, 8, 0, kind, value)
    # 构造起始元素
    def start(tag, *attributes):
        body = b''.join(attributes)
        return struct.pack('<HHI4I6H', 0x102, 16, 36 + len(body), 0, NO_INDEX, NO_INDEX,
                           index(tag), 20, 20, len(attributes), 0, 0, 0) + body
    # 构造结束元素
    def end(tag):
        return struct.pack('<HHI4I', 0x103, 16, 24, 0, NO_INDEX, NO_INDEX, index(tag))
    # 属性名对应的资源 ID
    resources = [0] * len(values)
    for name, resource in {'theme': 0x01010000, 'label': 0x01010001, 'name': 0x01010003,
                           'exported': 0x01010010, 'value': 0x01010024,
                           'targetSdkVersion': 0x01010270}.items():
        resources[index(name)] = resource
    # 字符串池，随后是资源映射块与命名空间
    body = pool(values, utf8)
    body += struct.pack('<HHI', 0x180, 8, 8 + 4 * len(resources)) + struct.pack('<' + 'I' * len(resources), *resources)
    body += struct.pack('<HHI4I', 0x100, 16, 24, 0, NO_INDEX, index('android'), index(namespace))
    # 清单根、目标 SDK 与已有的网络权限
    body += start('manifest', attribute('package', 'org.example.product', android=False))
    body += start('uses-sdk', attribute('targetSdkVersion', 35, 0x10)) + end('uses-sdk')
    body += start('uses-permission', attribute('name', 'android.permission.INTERNET')) + end('uses-permission')
    # 应用元素的属性；是否带有 name 由参数决定
    application = [attribute('theme', 0x7f010001, 1), attribute('label', '诡秘神谕')]
    if named_application:
        application.append(attribute('name', 'org.example.OriginalApplication'))
    body += start('application', *application)
    # Rust 应用标记
    body += start('meta-data', attribute('name', 'com.longbridge.nsh.RUST'), attribute('value', 0xffffffff, 0x12)) + end('meta-data')
    # 运行器 Activity：导出，带启动器 intent 过滤器
    body += start('activity', attribute('name', runner), attribute('exported', 0xffffffff, 0x12))
    body += start('intent-filter')
    body += start('action', attribute('name', 'android.intent.action.MAIN')) + end('action')
    body += start('category', attribute('name', 'android.intent.category.LAUNCHER')) + end('category')
    body += end('intent-filter') + end('activity')
    # 一个无关的、不导出的 Activity，不应被改动
    body += start('activity', attribute('name', 'org.example.HelpActivity'), attribute('exported', 0, 0x12)) + end('activity')
    body += end('application') + end('manifest')
    # 命名空间结束
    body += struct.pack('<HHI4I', 0x101, 16, 24, 0, NO_INDEX, index('android'), index(namespace))
    return struct.pack('<HHI', 3, 8, 8 + len(body)) + body


# 把二进制清单解析为（类型、标签、属性）列表，便于逐项比较
def manifest_nodes(data):
    values = []; result = []; offset = 8
    while offset < len(data):
        kind, _, size = struct.unpack_from('<HHI', data, offset)
        chunk = data[offset:offset + size]; offset += size
        # 字符串池
        if kind == 1:
            values, _ = strings(chunk)
        # 只看元素的起止
        if kind not in {0x102, 0x103}:
            continue
        tag = values[struct.unpack_from('<I', chunk, 20)[0]]
        attributes = {}
        # 起始元素才有属性
        if kind == 0x102:
            start, stride, count = struct.unpack_from('<3H', chunk, 24)
            for position in range(count):
                namespace, name, _, _, _, value_type, value = struct.unpack_from('<IIIHBBI', chunk, 16 + start + stride * position)
                key = (values[namespace] if namespace != NO_INDEX else None, values[name])
                attributes[key] = (value_type, values[value] if value_type == 3 else value)
        result.append((kind, tag, attributes))
    return result


# 构造小型 ELF64 夹具：含链接好的动态字符串表、符号表和一个文本函数
def elf_runtime(*, defined=True, visibility=0, function=True, named=True):
    """Small ELF64 fixture with a linked dynamic string/symbol table and one text function."""
    # 符号名可选正确或错误
    name = android_package.NATIVE_INIT if named else b'OtherNativeFunction'
    names = b'\0' + name + b'\0'
    name_offset = 64 + 4 * 64
    symbol_offset = (name_offset + len(names) + 7) // 8 * 8
    text_offset = symbol_offset + 48
    output = bytearray(text_offset + 4)
    identity = b'\x7fELF\x02\x01\x01' + bytes(9)
    # ELF 头
    output[:64] = struct.pack('<16sHHIQQQIHHHHHH', identity, 3, 183, 1, 0, 0, 64, 0, 64, 0, 0, 64, 4, 0)
    # 写入一个节头
    def section(index, kind, offset, size, link=0, entry=0):
        output[64 + index * 64:64 + (index + 1) * 64] = struct.pack('<IIQQQQIIQQ',
            0, kind, 0, 0, offset, size, link, 0, 1, entry)
    # 字符串表
    section(1, 3, name_offset, len(names))
    # 动态符号表
    section(2, 11, symbol_offset, 48, 1, 24)
    # 文本节
    section(3, 1, text_offset, 4)
    output[name_offset:name_offset + len(names)] = names
    # 写入符号：是否定义、可见性、是否为函数都可调
    output[symbol_offset + 24:symbol_offset + 48] = struct.pack('<IBBHQQ',
        1, 0x12 if function else 0x11, visibility, 3 if defined else 0, 0x1000, 4)
    return bytes(output)


# 标准构建输入的测试
class StandardBuildInputs(unittest.TestCase):
    # 打包使用标准的运行器目标，并保持移动端标识不变
    def test_packaging_uses_standard_runner_targets_and_preserves_mobile_ids(self):
        android = package.mobile_command('nsh', 'android', Path('/tmp/output'))
        ios = package.mobile_command('nsh', 'ios', Path('/tmp/output'))
        # Android 应用标识
        self.assertEqual(android[android.index('--id') + 1], 'com.manonloki.lokimetrisdisplay')
        # 版本与构建号取自 nsh.json
        app = json.loads((package.ROOT / 'nsh.json').read_text())
        self.assertEqual(android[android.index('--app-version') + 1], app['version'])
        self.assertEqual(android[android.index('--build') + 1], str(app['build']))
        # iOS 默认面向模拟器
        self.assertEqual(ios[ios.index('--target') + 1], 'ios-simulator')
        self.assertEqual(ios[ios.index('--id') + 1], package.apple_identity()['bundleId'])
        # 真机目标
        device = package.mobile_command('nsh', 'ios', Path('/tmp/output'), ios_device=True)
        self.assertEqual(device[device.index('--target') + 1], 'ios')

    # Android 运行时种子只选出受支持的 ABI，不改动已安装的运行时
    def test_android_seed_selects_supported_abis_without_editing_installed_runtime(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'installed/android/release'
            source.mkdir(parents=True)
            (source / 'classes.dex').write_bytes(b'dex')
            # 造出四种 ABI 的假运行时库
            for abi in ['arm64-v8a', 'x86_64', 'armeabi-v7a', 'x86']:
                leaf = source / 'lib' / abi
                leaf.mkdir(parents=True)
                (leaf / 'libnsh.so').write_bytes(abi.encode())
            stage = root / 'stage'; stage.mkdir()
            env = {'NSH_RUNTIME': str(root / 'installed')}
            with patch.object(package, 'ROOT', root):
                package.android_runtime_seed('/unused/nsh', stage, env)
            # 种子库目录
            seed = Path(env['NSH_RUNTIME']) / 'android/release/lib'
            # 种子里只有 arm64-v8a 与 x86_64
            self.assertEqual(sorted(path.name for path in seed.iterdir()), ['arm64-v8a', 'x86_64'])
            # 已安装的运行时原封不动
            self.assertEqual(len(list((source / 'lib').iterdir())), 4)

    # 签名密码不会出现在进程参数里
    def test_signing_credentials_stay_out_of_process_arguments(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'lokimetris.keystore').write_bytes(b'fixture key')
            (root / 'signing.properties').write_text('keyAlias=fixture\nstorePassword=fixture-secret\nkeyPassword=fixture-key-secret\n')
            args, env = package.android_signing(root)
            # 参数里不能出现密码
            self.assertNotIn('fixture-secret', str(args))
            self.assertNotIn('fixture-key-secret', str(args))
            # 密码只通过环境变量传递
            self.assertEqual(env['NSH_KEYSTORE_PASSWORD'], 'fixture-secret')
            self.assertEqual(env['LOKI_SIGNING_KEY_PASSWORD'], 'fixture-key-secret')

    # iOS 的发现相关声明在签名之前写入
    def test_ios_discovery_metadata_is_added_before_signing(self):
        with tempfile.TemporaryDirectory() as directory:
            app = Path(directory) / 'Display.app'; app.mkdir()
            info = app / 'Info.plist'
            # 带一个应当被保留的无关字段
            info.write_bytes(plistlib.dumps({'CFBundleIdentifier': package.APPLE_ID, 'CFBundleExecutable': 'App', 'Other': 'preserved'}))
            signed = []
            # 记录签名命令
            def inspect_signing(command):
                signed.append(command)
            # 替换命令执行与内嵌标识读取
            with patch.object(package, 'run', side_effect=inspect_signing) as run, \
                 patch.object(package, 'embedded_apple_identifier', return_value=package.apple_identity()['applicationIdentifier']):
                package.configure_apple(app, simulator=True, application_id=package.APPLE_ID, minimum_os='15.0')
            value = plistlib.loads(info.read_bytes())
            # 无关字段保留
            self.assertEqual(value['Other'], 'preserved')
            # 写入了 Bonjour 服务
            self.assertEqual(value['NSBonjourServices'], ['_loki-host._tcp'])
            self.assertEqual(value['MinimumOSVersion'], '15.0')
            self.assertTrue(value['NSLocalNetworkUsageDescription'])
            # 签名与校验各一次
            self.assertEqual(run.call_count, 2)
            # 临时签名不带权限文件
            self.assertTrue(all('--entitlements' not in command for command in signed))

    # 清单权限的补充是幂等的，并保留目标 SDK 版本
    def test_manifest_permissions_are_idempotent_and_keep_target_sdk(self):
        values = ['manifest', 'application', 'uses-sdk', 'targetSdkVersion', 'name',
                  'http://schemas.android.com/apk/res/android']
        def start(tag, attrs=b''):
            return struct.pack('<HHI4I6H', 0x102, 16, 36 + len(attrs), 0, NO_INDEX, NO_INDEX,
                               values.index(tag), 20, 20, len(attrs) // 20, 0, 0, 0) + attrs
        def end(tag):
            return struct.pack('<HHI4I', 0x103, 16, 24, 0, NO_INDEX, NO_INDEX, values.index(tag))
        # 目标 SDK 版本 35
        attribute = struct.pack('<IIIHBBI', 5, 3, NO_INDEX, 8, 0, 0x10, 35)
        body = pool(values, True) + start('manifest') + start('uses-sdk', attribute) + end('uses-sdk')
        body += start('application') + end('application') + end('manifest')
        manifest = struct.pack('<HHI', 3, 8, len(body) + 8) + body
        # 第一次补充权限
        output, target = lan_permissions(manifest)
        # 再次补充应保持不变
        repeated, _ = lan_permissions(output)
        self.assertEqual(output, repeated)
        self.assertEqual(target, 35)
        size = struct.unpack_from('<I', output, 12)[0]
        names, _ = strings(output[8:8 + size])
        # 两个局域网权限都已加入
        self.assertIn('android.permission.CHANGE_WIFI_MULTICAST_STATE', names)
        self.assertIn('android.permission.ACCESS_NETWORK_STATE', names)

    # 组件清单只改应用自有的类名，并保留启动器契约
    def test_component_manifest_only_changes_app_classes_and_preserves_launcher_contract(self):
        namespace = 'http://schemas.android.com/apk/res/android'
        name = (namespace, 'name')
        # 覆盖两种字符串编码与有无 Application 名称
        for utf8 in [False, True]:
            for named in [False, True]:
                with self.subTest(utf8=utf8, named_application=named):
                    original = manifest_fixture(utf8=utf8, named_application=named)
                    output, target = component_manifest(original)
                    # 幂等
                    repeated, repeated_target = component_manifest(output)
                    self.assertEqual(output, repeated)
                    self.assertEqual((target, repeated_target), (35, 35))
                    # 构造预期：只有 application 与运行器 activity 的名称被替换
                    expected = manifest_nodes(original)
                    for kind, tag, attributes in expected:
                        if kind == 0x102 and tag == 'application':
                            attributes[name] = (3, APPLICATION)
                        if kind == 0x102 and tag == 'activity' and attributes.get(name) == (3, RUNNER_ACTIVITY):
                            attributes[name] = (3, ACTIVITY)
                    actual = manifest_nodes(output)
                    # 除权限元素外，其余节点必须与预期完全一致
                    self.assertEqual([node for node in actual if node[1] != 'uses-permission'],
                                     [node for node in expected if node[1] != 'uses-permission'])
                    # 权限集合：原有的加上新增的两个
                    permissions = [attributes[name][1] for kind, tag, attributes in actual
                                   if kind == 0x102 and tag == 'uses-permission']
                    self.assertCountEqual(permissions, ['android.permission.INTERNET'] + PERMISSIONS)

    # 无法识别的运行器会被拒绝
    def test_component_manifest_rejects_an_unrecognized_runner(self):
        original = manifest_fixture(utf8=True, named_application=False, runner='org.example.UnrelatedActivity')
        with self.assertRaisesRegex(RuntimeError, 'standard NSH application and runner activity'):
            component_manifest(original)

    # Java 合并保留官方类的字节，并拒绝覆盖官方类
    def test_java_merge_keeps_official_class_bytes_and_rejects_sdk_class_overrides(self):
        # 分别测试无冲突与有冲突
        for collision in [False, True]:
            with self.subTest(collision=collision), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                # 假的平台 jar
                platform = root / 'sdk/platforms/android-37/android.jar'
                platform.parent.mkdir(parents=True); platform.write_bytes(b'fixture API')
                sources = root / 'app-java'; sources.mkdir()
                (sources / 'DisplayActivity.java').write_text('class DisplayActivity {}')
                # 假的官方类字节
                original = {'com/longbridge/nsh/NshView.class': b'official view bytecode',
                            'com/longbridge/nsh/Native.class': b'official native bytecode'}
                sdk_jar = root / 'official.jar'; java_tools.write_jar(sdk_jar, original)
                aar = root / 'nsh.aar'
                with zipfile.ZipFile(aar, 'w') as archive:
                    archive.write(sdk_jar, 'classes.jar')
                # 假的 javac：产出应用类；冲突时产出与官方同名的类
                def javac(command, **_):
                    destination = Path(command[command.index('-d') + 1])
                    name = 'com/longbridge/nsh/NshView.class' if collision else 'com/manonloki/lokimetrisdisplay/bridge/DisplayActivity.class'
                    path = destination / name; path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(b'new application bytecode')
                args = argparse.Namespace(out=root / 'out', sdk=root / 'sdk', compile_sdk=37, javac=None, aar=aar)
                # 替换 Java 工具链相关调用，避免真正编译
                with patch.object(java_tools, 'SOURCE', sources), \
                     patch.object(java_tools, 'java_compiler', return_value=Path('/fixture/javac')), \
                     patch.object(java_tools.subprocess, 'check_output', return_value='javac fixture'), \
                     patch.object(java_tools.subprocess, 'run', side_effect=javac), redirect_stdout(io.StringIO()):
                    # 冲突时必须报错且不产出 jar
                    if collision:
                        with self.assertRaisesRegex(RuntimeError, 'replace an official NSH class'):
                            java_tools.build(args)
                        self.assertFalse((args.out / 'classes.jar').exists())
                    # 无冲突：官方类字节原样保留，只多出一个应用类
                    else:
                        report = java_tools.build(args)
                        self.assertFalse(report['officialSdk']['modified'])
                        merged = java_tools.jar_entries((args.out / 'classes.jar').read_bytes())
                        for name, bytecode in original.items():
                            self.assertEqual(merged[name], bytecode)
                        self.assertEqual(len(merged), len(original) + 1)

    # Java 源码回退路径：不依赖旧式校验文件，直接核对锁定的发布包
    def test_java_source_fallback_checks_locked_archive_without_legacy_checksum_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); cargo = root / 'cargo'
            # 假的 Cargo 注册表目录
            source = cargo / 'registry/src/index.crates.io-fixture/nsh-platform-android-0.3.0'
            relative = 'java/com/longbridge/nsh/NshView.java'
            original = b'package com.longbridge.nsh; public class NshView {}'
            path = source / relative; path.parent.mkdir(parents=True); path.write_bytes(original)
            # 对应的 .crate 压缩包
            archive = source.parents[2] / 'cache' / source.parent.name / (source.name + '.crate')
            archive.parent.mkdir(parents=True)
            with tarfile.open(archive, 'w:gz') as release:
                member = tarfile.TarInfo(source.name + '/' + relative); member.size = len(original)
                release.addfile(member, io.BytesIO(original))
            # 压缩包摘要写进 Cargo.lock
            checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
            (root / 'Cargo.lock').write_text('version = 4\n[[package]]\nname = "nsh-platform-android"\n'
                'version = "0.3.0"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\n'
                'checksum = "' + checksum + '"\n')
            platform = root / 'sdk/platforms/android-37/android.jar'
            platform.parent.mkdir(parents=True); platform.write_bytes(b'fixture API')
            app = root / 'app-java'; app.mkdir(); (app / 'Bridge.java').write_text('class Bridge {}')
            # 假的 javac：确认官方源码被纳入编译，并产出两个类
            def javac(command, **_):
                self.assertIn(str(path), command)
                destination = Path(command[command.index('-d') + 1])
                for name in ['com/longbridge/nsh/NshView.class', 'com/example/Bridge.class']:
                    compiled = destination / name; compiled.parent.mkdir(parents=True, exist_ok=True)
                    compiled.write_bytes(b'compiled ' + name.encode())
            args = argparse.Namespace(out=root / 'out', sdk=root / 'sdk', compile_sdk=37, javac=None)
            # 替换 Java 工具链相关调用，避免真正编译
            with patch.object(java_tools, 'ROOT', root), patch.object(java_tools, 'SOURCE', app), \
                 patch.dict(java_tools.os.environ, {'CARGO_HOME': str(cargo)}), \
                 patch.object(java_tools, 'official_aar', return_value=None), \
                 patch.object(java_tools, 'java_compiler', return_value=Path('/fixture/javac')), \
                 patch.object(java_tools.subprocess, 'check_output', return_value='javac fixture'), \
                 patch.object(java_tools.subprocess, 'run', side_effect=javac) as compile_java, redirect_stdout(io.StringIO()):
                # 确认没有旧式校验文件
                self.assertFalse((source / '.cargo-checksum.json').exists())
                report = java_tools.build(args)
                self.assertEqual(report['officialSdk']['kind'], 'official-crates-io-java')
                self.assertEqual(report['officialSdk']['crateChecksum'], checksum)
                # 官方源码未被改动
                self.assertEqual(path.read_bytes(), original)
                self.assertEqual(compile_java.call_count, 1)
                path.write_bytes(original + b' // locally modified')
                compile_java.reset_mock()
                with self.assertRaisesRegex(RuntimeError, 'Java source .*differs|Java source checksum differs'):
                    java_tools.build(args)
                compile_java.assert_not_called()
                # 恢复源码后，再破坏压缩包缓存，同样必须失败
                path.write_bytes(original)
                archive.write_bytes(archive.read_bytes() + b'corrupt cache')
                with self.assertRaisesRegex(RuntimeError, 'archive checksum differs'):
                    java_tools.build(args)
                compile_java.assert_not_called()

    # JNI 导出校验要求动态符号表里有已定义、可见的函数
    def test_jni_export_verifier_requires_a_defined_visible_dynamic_function(self):
        # 正常情况通过
        self.assertTrue(android_package.has_native_init(elf_runtime()))
        # 未定义不行
        self.assertFalse(android_package.has_native_init(elf_runtime(defined=False)))
        # 隐藏可见性不行
        self.assertFalse(android_package.has_native_init(elf_runtime(visibility=2)))
        # 不是函数不行
        self.assertFalse(android_package.has_native_init(elf_runtime(function=False)))
        # 只是字符串里碰巧出现也不行
        self.assertFalse(android_package.has_native_init(elf_runtime(named=False) + android_package.NATIVE_INIT))

    # 打包在处理 Java 或签名之前就拒绝过期的 ABI，并保持输入不变
    def test_android_packaging_rejects_stale_abi_before_java_or_signing_and_preserves_input(self):
        with tempfile.TemporaryDirectory() as directory:
            apk = Path(directory) / 'App.apk'
            # 一个 ABI 正常，另一个缺少导出
            with zipfile.ZipFile(apk, 'w') as archive:
                archive.writestr('lib/arm64-v8a/libnsh.so', elf_runtime())
                archive.writestr('lib/x86_64/libnsh.so', elf_runtime(named=False) + android_package.NATIVE_INIT)
                archive.writestr('classes.dex', b'original dex')
            before = apk.read_bytes()
            with patch.object(android_package, 'sdk_directory') as sdk, \
                 patch.object(android_package.subprocess, 'run') as run:
                with self.assertRaisesRegex(RuntimeError, 'lacks exported .*lib/x86_64/libnsh.so'):
                    # 更新 Java 并签名
                    android_package.update_java_and_sign(apk, [], {}, None)
                # 失败发生在读取 SDK 与签名之前
                sdk.assert_not_called()
                run.assert_not_called()
            # 输入的 APK 未被改动
            self.assertEqual(apk.read_bytes(), before)
            # 两个 ABI 都正确时通过校验，并按名称排序返回
            with zipfile.ZipFile(apk, 'w') as archive:
                for abi in ['x86_64', 'arm64-v8a']:
                    archive.writestr('lib/' + abi + '/libnsh.so', elf_runtime())
            self.assertEqual(android_package.verify_jni_exports(apk), ['arm64-v8a', 'x86_64'])


# 作为脚本运行时
if __name__ == '__main__':
    # 运行全部用例
    unittest.main()
