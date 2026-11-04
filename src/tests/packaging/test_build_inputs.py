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
# 引入 XML 处理
import xml.etree.ElementTree as ET
# 重定向标准输出
from contextlib import redirect_stdout
# 补丁工具
from unittest.mock import patch

# 仓库根目录
ROOT = Path(__file__).resolve().parents[3]
# 让测试能导入构建脚本
sys.path.insert(0, str(ROOT / 'tools/build'))
# 引入小组件扩展管线与常量
from android_widget import (APPLICATION, ACTIVITY, PERMISSIONS, RUNNER_ACTIVITY, WIDGET_PROVIDER,
    WIDGET_META_NAME, WIDGET_UPDATE_ACTION, android_resource_names, extend_manifest)
from android_binary_xml import NO_INDEX, to_text
from android_arsc import entries as arsc_entries
# 引入 Java 桥接构建工具
import build_android_bridge as java_tools
# 引入 Android 打包脚本
import package_android as android_package

# 按路径加载总打包脚本
spec = importlib.util.spec_from_file_location('package_app', ROOT / 'tools/build/package_app.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


# 构造字符串池块，与既有打包流程的二进制编码一致
def string_pool(values, utf8):
    """Encode a manifest string pool chunk."""
    body = bytearray(); offsets = []
    for value in values:
        offsets.append(len(body))
        units = len(value.encode('utf-16-le')) // 2
        if utf8:
            encoded = value.encode('utf-8')
            body += bytes([units]) if units < 0x80 else bytes([(units >> 8) | 0x80, units & 0xff])
            body += bytes([len(encoded)]) if len(encoded) < 0x80 else bytes([(len(encoded) >> 8) | 0x80, len(encoded) & 0xff])
            body += encoded + b'\0'
        else:
            body += struct.pack('<H', units) + value.encode('utf-16-le') + b'\0\0'
    body += bytes((-len(body)) % 4)
    start = 28 + len(values) * 4
    return (struct.pack('<HHI5I', 1, 28, start + len(body), len(values), 0, 0x100 if utf8 else 0, start, 0)
            + struct.pack('<' + 'I' * len(offsets), *offsets) + body)


# 构造一份编译后风格的二进制清单：含启动器、导出标记、Rust 元数据和一个无关 Activity
def manifest_fixture(*, utf8, named_application, runner=RUNNER_ACTIVITY):
    """A compiled-style manifest with launcher/exported/Rust metadata and an unrelated activity."""
    namespace = 'http://schemas.android.com/apk/res/android'
    values = ['manifest', 'application', 'activity', 'uses-sdk', 'meta-data', 'intent-filter',
              'action', 'category', 'uses-permission', 'name', 'package', 'targetSdkVersion',
              'exported', 'value', 'theme', 'label', 'icon', 'android', namespace, runner,
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
    for name, resource in {'theme': 0x01010000, 'label': 0x01010001, 'name': 0x01010003, 'icon': 0x01010002,
                           'exported': 0x01010010, 'value': 0x01010024,
                           'targetSdkVersion': 0x01010270}.items():
        resources[index(name)] = resource
    body = string_pool(values, utf8)
    body += struct.pack('<HHI', 0x180, 8, 8 + 4 * len(resources)) + struct.pack('<' + 'I' * len(resources), *resources)
    body += struct.pack('<HHI4I', 0x100, 16, 24, 0, NO_INDEX, index('android'), index(namespace))
    body += start('manifest', attribute('package', 'org.example.product', android=False))
    body += start('uses-sdk', attribute('targetSdkVersion', 35, 0x10)) + end('uses-sdk')
    body += start('uses-permission', attribute('name', 'android.permission.INTERNET')) + end('uses-permission')
    # 主题指向系统资源，图标指向应用资源，与真实基线一致
    application = [attribute('theme', 0x0103022e, 1), attribute('icon', 0x7f010000, 1), attribute('label', '诡秘神谕')]
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


# 构造最小 arsc 夹具：一个包、一个 mipmap 类型、一个 ic_launcher 项
def arsc_fixture():
    """A minimal resources.arsc with one mipmap/ic_launcher entry at 0x7f010000."""
    package_id = 0x7f
    # 类型与键名字列表
    type_names = ['mipmap']
    key_names = ['ic_launcher']
    # 类型与键字符串池
    type_pool = string_pool(type_names, True)
    key_pool = string_pool(key_names, True)
    # 类型规格：一项，缺省配置
    type_spec = struct.pack('<HHI', 0x202, 16, 20) + struct.pack('<3I', 1, 1, 0xFFFFFFFF)
    # 类型块：包头（含 config）、项数与项区起点、项偏移与项数据
    type_header = 8 + 12 + 8
    type_chunk = struct.pack('<HHI', 0x201, type_header, type_header + 4 + 8)
    type_chunk += struct.pack('<BBHII', 1, 0, 0, 1, type_header + 4)
    type_chunk += struct.pack('<HHI', 8, 0, 0)
    type_chunk += struct.pack('<I', 0)
    type_chunk += struct.pack('<HHI', 8, 0, 0)
    # 包块：包头、包 ID、256 字节包名与四个偏移字段，池放在对齐后的包头之后
    name = 'com.example.product'.encode('utf-16-le')
    name += bytes(256 - len(name))
    type_strings = 8 + 4 + 256 + 16
    key_strings = type_strings + len(type_pool)
    package_header = key_strings + len(key_pool)
    package = struct.pack('<HHI', 0x200, package_header,
                          package_header + len(type_spec) + len(type_chunk))
    package += struct.pack('<I', package_id) + name
    package += struct.pack('<4I', type_strings, len(type_names), key_strings, len(key_names))
    package += type_pool + key_pool + type_spec + type_chunk
    # 资源表：表头、包数与包块
    table = struct.pack('<HHII', 0x2, 12, 12 + len(package), 1)
    return table + package


# 构造小型 ELF64 夹具：含链接好的动态字符串表、符号表和一个文本函数
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
    # 清单还原与扩展：类名替换、权限与小组件接收器一次到位，输出确定
    def test_widget_manifest_extension_preserves_launcher_contract(self):
        # 覆盖两种字符串编码与有无 Application 名称
        for utf8 in [False, True]:
            for named in [False, True]:
                with self.subTest(utf8=utf8, named_application=named):
                    manifest = manifest_fixture(utf8=utf8, named_application=named)
                    arsc = arsc_fixture()
                    android_names = {0x0103022e: 'style/Theme.Material.NoActionBar'}
                    text, target = extend_manifest(manifest, arsc, android_names)
                    # 目标 SDK 保持 35
                    self.assertEqual(target, '35')
                    # 相同输入输出确定
                    self.assertEqual(extend_manifest(manifest, arsc, android_names), (text, target))
                    # 解析为文本树逐项核对
                    root = ET.fromstring(text)
                    android = '{http://schemas.android.com/apk/res/android}'
                    # 应用类名被替换
                    applications = root.findall('application')
                    self.assertEqual(len(applications), 1)
                    self.assertEqual(applications[0].get(f'{android}name'), APPLICATION)
                    # 运行器类名被替换且启动器契约保留
                    runners = [element for element in applications[0].findall('activity')
                               if element.get(f'{android}name') == ACTIVITY]
                    self.assertEqual(len(runners), 1)
                    self.assertIn('android.intent.category.LAUNCHER', text)
                    # 无关 Activity 不被改动
                    self.assertIn('org.example.HelpActivity', text)
                    # 权限：原有的加新增的两个
                    permissions = [element.get(f'{android}name') for element in root.findall('uses-permission')]
                    self.assertCountEqual(permissions, ['android.permission.INTERNET'] + PERMISSIONS)
                    # 小组件接收器：不导出、产品名、只接收系统更新广播，元数据指向小组件信息资源
                    receivers = [element for element in applications[0].findall('receiver')
                                 if element.get(f'{android}name') == WIDGET_PROVIDER]
                    self.assertEqual(len(receivers), 1)
                    receiver = receivers[0]
                    self.assertEqual(receiver.get(f'{android}exported'), 'false')
                    self.assertEqual(receiver.get(f'{android}label'), '诡秘神谕')
                    actions = [element.get(f'{android}name')
                               for element in receiver.findall('intent-filter/action')]
                    self.assertEqual(actions, [WIDGET_UPDATE_ACTION])
                    meta = receiver.find('meta-data')
                    self.assertEqual(meta.get(f'{android}name'), WIDGET_META_NAME)
                    self.assertEqual(meta.get(f'{android}resource'), '@xml/widget_info')
                    # 系统主题与应用图标引用都被还原为可链接的文本
                    self.assertEqual(applications[0].get(f'{android}theme'), '@android:style/Theme.Material.NoActionBar')
                    self.assertEqual(applications[0].get(f'{android}icon'), '@mipmap/ic_launcher')
                    # Rust 应用标记保留
                    self.assertIn('com.longbridge.nsh.RUST', text)

    # 无法识别的运行器会被拒绝
    def test_widget_manifest_rejects_an_unrecognized_runner(self):
        manifest = manifest_fixture(utf8=True, named_application=False, runner='org.example.UnrelatedActivity')
        with self.assertRaisesRegex(RuntimeError, 'standard NSH application and runner activity'):
            extend_manifest(manifest, arsc_fixture(), {0x0103022e: 'style/Theme.Material.NoActionBar'})

    # 应用资源引用缺失映射时失败关闭，不输出坏引用
    def test_widget_manifest_fails_closed_without_arsc_mapping(self):
        manifest = manifest_fixture(utf8=True, named_application=False)
        with self.assertRaisesRegex(RuntimeError, 'Resource name mapping lacks 0x'):
            extend_manifest(manifest, arsc_fixture(), {})

    # arsc 解析给出精确的资源项名
    def test_arsc_entries_resolve_type_and_name(self):
        package, mapping = arsc_entries(arsc_fixture())
        self.assertEqual(package, 'com.example.product')
        self.assertEqual(mapping, {0x7f010000: ('mipmap', 'ic_launcher')})

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
            source = cargo / ('registry/src/index.crates.io-fixture/nsh-platform-android-' + java_tools.NSH_VERSION)
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
                'version = "' + java_tools.NSH_VERSION + '"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\n'
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
