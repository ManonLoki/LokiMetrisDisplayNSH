"""Development signing keeps identity grants and prelinked instructions intact."""
# 引入标准库与 unittest.mock
from datetime import datetime, timedelta, timezone
import hashlib
import importlib.util
from pathlib import Path
import plistlib
import struct
import sys
import tempfile
# 测试框架
import unittest
# 补丁工具
from unittest.mock import patch

# 仓库根目录
ROOT = Path(__file__).resolve().parents[3]
# 让测试能导入构建脚本
sys.path.insert(0, str(ROOT / 'tools/build'))
# 按路径加载签名脚本
spec = importlib.util.spec_from_file_location('sign_device', ROOT / 'tools/build/sign_apple_device.py')
signing = importlib.util.module_from_spec(spec)
spec.loader.exec_module(signing)
# 测试用团队 ID、应用标识与证书
TEAM = 'ABCDEFGHIJ'
BUNDLE = 'org.example.phone'
CERT = b'public certificate fixture'
# 证书的 SHA-1 指纹
FINGERPRINT = hashlib.sha1(CERT).hexdigest().upper()


# 构造描述文件夹具：有效期一天，允许的标识与钥匙串组按前缀通配
def provision(prefix=TEAM, grant=None):
    return {'UUID': 'profile', 'Name': 'development', 'TeamIdentifier': [TEAM],
            'ApplicationIdentifierPrefix': [prefix], 'ProvisionedDevices': ['phone'],
            'DeveloperCertificates': [CERT],
            'ExpirationDate': datetime.now(timezone.utc) + timedelta(days=1),
            'Entitlements': {'application-identifier': grant or prefix + '.*',
                             'keychain-access-groups': [prefix + '.*'],
                             'get-task-allow': True,
                             # 旧的小组件共享组
                             'com.apple.security.application-groups': ['retired.widget.group']}}


# 构造最小的 Mach-O：一个 __text 节和一段“签名”，用于验证指令摘要不受签名影响
def macho(body=b'original instructions', signature=b'old signature'):
    header = struct.pack('<8I', 0xfeedfacf, 0x0100000c, 0, 2, 1, 152, 0, 0)
    segment = struct.pack('<II16s4Q4I', 0x19, 152, b'__TEXT', 0, 0, 0, 0, 7, 5, 1, 0)
    section = struct.pack('<16s16sQQ8I', b'__text', b'__TEXT', 0, len(body), 256, 0, 0, 0, 0, 0, 0, 0)
    return (header + segment + section).ljust(256, b'\0') + body + signature


# 设备签名的测试
class DeviceSigning(unittest.TestCase):
    # 只请求最小的私有钥匙串权限
    def test_only_minimal_private_keychain_entitlements_are_requested(self):
        request = signing.development_entitlements(provision(), BUNDLE, TEAM, FINGERPRINT, device='phone')
        self.assertEqual(request, {'application-identifier': TEAM + '.' + BUNDLE,
                                  'com.apple.developer.team-identifier': TEAM,
                                  'get-task-allow': True,
                                  'keychain-access-groups': [TEAM + '.' + BUNDLE]})

    # 沿用旧的应用前缀，与证书团队无关
    def test_legacy_app_prefix_is_kept_independently_of_certificate_team(self):
        request = signing.development_entitlements(provision(prefix='OLDPREFIX1'), BUNDLE,
                                                  TEAM, FINGERPRINT, device='phone')
        self.assertEqual(request['application-identifier'], 'OLDPREFIX1.' + BUNDLE)
        self.assertEqual(request['com.apple.developer.team-identifier'], TEAM)

    # 描述文件的设备、标识、团队、证书或有效期不对时必须拒绝
    def test_profile_rejects_wrong_device_bundle_team_certificate_or_expiry(self):
        # 逐项构造出错的描述文件
        cases = [({'ProvisionedDevices': ['other']}, {}),
                 ({'TeamIdentifier': ['OTHERTEAM1']}, {}),
                 ({'DeveloperCertificates': [b'other certificate']}, {}),
                 ({'ExpirationDate': datetime.now(timezone.utc) - timedelta(days=1)}, {}),
                 ({}, {'application-identifier': TEAM + '.another.bundle'}),
                 ({}, {'get-task-allow': False}),
                 ({}, {'keychain-access-groups': ['OTHERTEAM1.*']})]
        for top, entitlements in cases:
            # 构造被篡改的描述文件
            value = provision(); value.update(top); value['Entitlements'].update(entitlements)
            # 每个子用例都应报错
            with self.subTest(top=top, entitlements=entitlements), self.assertRaises(RuntimeError):
                # 生成开发权限
                signing.development_entitlements(value, BUNDLE, TEAM, FINGERPRINT, device='phone')

    # 指令摘要不受签名块影响，但指令变化会被发现
    def test_code_hash_ignores_signing_blob_but_detects_changed_instructions(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'App'; path.write_bytes(macho())
            before = signing.instructions(path)
            # 只换签名：摘要不变
            path.write_bytes(macho(signature=b'new DER signature'))
            self.assertEqual(signing.instructions(path), before)
            # 换指令：摘要改变
            path.write_bytes(macho(body=b'changed instructions'))
            self.assertNotEqual(signing.instructions(path), before)

    # 命令行在写任何文件之前就拒绝嵌套在源包里的输出，即使只是生成计划
    def test_cli_rejects_nested_output_before_writing_even_in_plan_only_mode(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); source = root / 'Source.app'; source.mkdir()
            marker = source / 'original'; marker.write_bytes(b'keep original')
            # 用符号链接别名也要拒绝
            alias = root / 'source-alias'; alias.symlink_to(source, target_is_directory=True)
            for parent in [source, alias]:
                for execute in [[], ['--execute']]:
                    arguments = ['sign_apple_device.py', '--app', str(source),
                                 '--output', str(parent / 'Nested.app'), '--bundle-id', BUNDLE,
                                 '--team', TEAM, '--identity', FINGERPRINT,
                                 '--phone-profile', 'phone.mobileprovision',
                                 '--watch-profile', 'watch.mobileprovision', '--device', 'phone', *execute]
                    # 逐个子用例：替换命令行与签名计划，并期望报错
                    with self.subTest(parent=parent, execute=execute), \
                            patch.object(sys, 'argv', arguments), patch.object(signing, 'plan') as plan, \
                            self.assertRaisesRegex(RuntimeError, 'outside the source bundle'):
                        # 执行入口
                        signing.main()
                    # 不应走到生成计划
                    plan.assert_not_called()
                    # 源包没有被改动
                    self.assertEqual(list(source.iterdir()), [marker])
                    self.assertEqual(marker.read_bytes(), b'keep original')

    # 签名副本会同时更新两个标识、先签子应用、并保留源包不变
    def test_signing_copy_updates_both_ids_signs_child_first_and_preserves_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); source = root / 'Source.app'; watch = source / 'Watch/Watch.app'
            watch.mkdir(parents=True); (source / 'nsh').mkdir()
            (source / 'nsh/bundle.nshc').write_bytes(b'unchanged NSH bytecode')
            # 给手机与手表各写入可执行文件和 Info.plist
            for app, bundle in [(source, 'old.phone'), (watch, 'old.phone.watchkitapp')]:
                (app / 'App').write_bytes(macho())
                info = {'CFBundleExecutable': 'App', 'CFBundleIdentifier': bundle}
                if app == watch: info['WKCompanionAppBundleIdentifier'] = 'old.phone'
                (app / 'Info.plist').write_bytes(plistlib.dumps(info))
            profile = root / 'profile.mobileprovision'; profile.write_bytes(b'public signed profile')
            phone_ent = signing.development_entitlements(provision(), BUNDLE, TEAM, FINGERPRINT)
            watch_ent = signing.development_entitlements(provision(), BUNDLE + '.watchkitapp', TEAM, FINGERPRINT)
            # 报告：包标识
            report = {'bundleId': BUNDLE, 'watchBundleId': BUNDLE + '.watchkitapp',
                      # 手表相对路径与证书指纹
                      'watchRelativePath': 'Watch/Watch.app', 'certificateSha1': FINGERPRINT,
                      # 手机与手表描述文件权限
                      'phoneProfile': {'entitlements': phone_ent}, 'watchProfile': {'entitlements': watch_ent},
                      # 手机指令
                      'phoneInstructions': signing.instructions(source / 'App'),
                      # 手表指令
                      'watchInstructions': signing.instructions(watch / 'App'),
                      # NSH 包摘要
                      'nshBundleSha256': signing.nsh_bundle_hash(source)}
            # 输出路径与已签名列表
            output = root / 'signed/Phone.app'; signed = []
            # 假的 codesign：记录签名顺序，并返回预期的权限
            def codesign(arguments):
                if '--force' in arguments:
                    signed.append(Path(arguments[-1])); return b''
                if '--display' in arguments:
                    return plistlib.dumps(watch_ent if Path(arguments[-1]) == output / 'Watch/Watch.app' else phone_ent)
                return b''
            with patch.object(signing, 'command', side_effect=codesign):
                signing.sign_copy(source, output, report, profile, profile)
            # 先签手表，再签手机
            self.assertEqual(signed, [output / 'Watch/Watch.app', output])
            # 源包的标识保持不变
            self.assertEqual(plistlib.loads((source / 'Info.plist').read_bytes())['CFBundleIdentifier'], 'old.phone')
            # 副本中手表的配套标识已更新
            actual = plistlib.loads((output / 'Watch/Watch.app/Info.plist').read_bytes())
            self.assertEqual(actual['WKCompanionAppBundleIdentifier'], BUNDLE)
            # 指令均被保留
            self.assertTrue(report['phoneInstructionsPreserved'] and report['watchInstructionsPreserved'])
            # 输出已存在时再次签名应被拒绝
            with self.assertRaisesRegex(RuntimeError, 'Output must be a new application'):
                signing.sign_copy(source, output, report, profile, profile)


# 作为脚本运行时
if __name__ == '__main__':
    # 运行全部用例
    unittest.main()
