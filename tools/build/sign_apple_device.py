#!/usr/bin/env python3
"""Sign an immutable NSH device build for an existing development identity; never install it."""
# 引入标准库
import argparse
# Base64 解码
import base64
# 日期时间与时区
from datetime import datetime, timezone
# 摘要算法
import hashlib
# JSON 读写
import json
# 路径对象
from pathlib import Path
# plist 读写
import plistlib
# 正则表达式
import re
# 文件工具
import shutil
# 结构打包
import struct
# 子进程
import subprocess

# 引入 Mach-O 平台判断与 NSH 包摘要
from package_apple_companion import macho_platforms, nsh_bundle_hash
from apple_entitlements import (development_entitlements, executable, instructions, permits,  # noqa: F401
                                profile_summary)


# 执行外部命令并返回标准输出，超时 90 秒
def command(arguments):
    # 执行命令并捕获输出，失败即抛错
    return subprocess.run([str(item) for item in arguments], check=True, capture_output=True,
                          # 命令超时 90 秒，取标准输出
                          timeout=90).stdout


# 解码描述文件（CMS）为字典
def profile(path):
    # 用 security 解开描述文件的 CMS 外壳并解析 plist
    return plistlib.loads(command(['security', 'cms', '-D', '-i', path]))




# 确认指纹对应本机有效的签名身份，并从其公开证书读出团队 ID
def certificate(fingerprint):
    # 列出本机可用于代码签名的身份
    identities = command(['security', 'find-identity', '-v', '-p', 'codesigning']).decode()
    # 指纹不在其中
    if fingerprint not in identities:
        # 报错：所选证书没有有效的本地签名身份
        raise RuntimeError('The selected certificate has no valid local code-signing identity')
    # 列出 Apple Development 证书的 PEM
    certificates = command(['security', 'find-certificate', '-a', '-c', 'Apple Development', '-p']).decode()
    # 逐个证书核对 SHA-1
    for pem in re.findall(r'-----BEGIN CERTIFICATE-----.*?-----END CERTIFICATE-----', certificates, re.S):
        # 去掉首尾行后解码出 DER
        der = base64.b64decode(''.join(pem.splitlines()[1:-1]))
        # SHA-1 与指纹不符
        if hashlib.sha1(der).hexdigest().upper() != fingerprint:
            # 跳过这一张证书
            continue
        # 解析主题
        result = subprocess.run(['openssl', 'x509', '-noout', '-subject', '-nameopt', 'sep_multiline'],
                                # 证书内容通过标准输入传入
                                input=pem.encode(), check=True, capture_output=True)
        # 取 OU 作为团队 ID
        team = re.search(r'^\s*OU\s*=\s*([A-Z0-9]{10})\s*$', result.stdout.decode(), re.M)
        # 解析出了团队标识
        if team:
            # 返回团队标识
            return team.group(1)
    # 无法确定证书所属团队
    raise RuntimeError('Cannot establish the Apple Development certificate team from its public certificate')










# 生成签名计划（不执行签名）：核对包结构、描述文件、证书与版本一致性
def plan(app, bundle_id, team, fingerprint, phone_profile, watch_profile, phone_device, watch_device=None,
         # 小组件描述文件与共享组
         widget_profile=None, app_group=None):
    # 解析为真实路径
    app = app.resolve(strict=True)
    # 证书所属团队与期望不符
    if certificate(fingerprint) != team:
        # 报错
        raise RuntimeError('Selected development certificate belongs to another team')
    # 读取应用 Info.plist
    info = plistlib.loads((app / 'Info.plist').read_bytes())
    # 必须是 iOS 真机可执行文件
    if macho_platforms(executable(app, info)) != {2}:
        # 报错：需要真机可执行文件而不是模拟器
        raise RuntimeError('Signing requires an iOS device executable, not a simulator')
    # 内嵌的 Watch 应用，最多一个，且必须以本应用为配套
    children = [(path, plistlib.loads((path / 'Info.plist').read_bytes())) for path in app.rglob('*.app')]
    # 手表应用多于一个或不属于本应用
    if len(children) > 1 or any(child.get('WKCompanionAppBundleIdentifier') != info['CFBundleIdentifier']
                                # （续）逐个检查伴生标识
                                for _, child in children):
        # 报错
        raise RuntimeError('Expected at most one matching embedded Watch application')
    # 内嵌的扩展，只允许匹配的 WidgetKit 小组件扩展，并位于 PlugIns
    extensions = [(path, plistlib.loads((path / 'Info.plist').read_bytes())) for path in app.rglob('*.appex')]
    # 小组件扩展多于一个或位置、标识、扩展点不符
    if len(extensions) > 1 or any(path.parent != app / 'PlugIns'
            # （续）标识须为应用标识加 .widgets
            or value.get('CFBundleIdentifier') != info['CFBundleIdentifier'] + '.widgets'
            # （续）扩展点须为 WidgetKit
            or value.get('NSExtension', {}).get('NSExtensionPointIdentifier') != 'com.apple.widgetkit-extension'
            # （续）逐个检查
            for path, value in extensions):
        # 报错
        raise RuntimeError('Expected only the matching WidgetKit extension in PlugIns')
    # 本签名流程要求 NSH 与小组件都是静态二进制
    if list(app.rglob('*.framework')) or list(app.rglob('*.dylib')):
        # 签名器要求静态的 NSH 与小组件包
        raise RuntimeError('This signer requires static NSH and Widget application binaries')
    # 描述文件与组件必须一一对应
    if extensions and widget_profile is None:
        # 小组件扩展必须有自己的描述文件
        raise RuntimeError('The WidgetKit extension requires its own development profile')
    # 没有扩展却给了小组件描述文件
    if not extensions and widget_profile is not None:
        # 报错
        raise RuntimeError('A Widget profile was supplied without a WidgetKit extension')
    # 有手表应用却没给描述文件
    if children and watch_profile is None:
        # 报错
        raise RuntimeError('The embedded Watch application requires its own development profile')
    # 没有手表应用却给了描述文件
    if not children and watch_profile is not None:
        # 报错
        raise RuntimeError('A Watch profile was supplied without a Watch application')
    # 内嵌产物的平台与版本必须和手机应用一致
    for child, child_info in children + extensions:
        # 手表平台编号为 4，其他为 2
        platform = {4} if child.suffix == '.app' else {2}
        # 平台与设备不符
        if macho_platforms(executable(child, child_info)) != platform:
            # 报错
            raise RuntimeError('Embedded application does not match its device platform')
        # 版本号与手机应用不一致
        if child_info['CFBundleVersion'] != info['CFBundleVersion'] or child_info['CFBundleShortVersionString'] != info['CFBundleShortVersionString']:
            # 报错
            raise RuntimeError('Phone and embedded application versions must match')
    # 解析手机描述文件
    phone = profile(phone_profile)
    # 手表应用标识
    watch_id = bundle_id + '.watchkitapp'
    # 共享组：优先用指定值，否则按包标识生成
    group = app_group or 'group.' + bundle_id
    # 手机与小组件必须声明同一个显式 App Group
    if extensions:
        # 手机声明的共享组
        source_group = info.get('LokiWidgetAppGroup')
        # 手机与小组件的共享组必须相同
        if not source_group or extensions[0][1].get('LokiWidgetAppGroup') != source_group:
            # 报错
            raise RuntimeError('Phone and Widget must declare the same explicit App Group')
    # 没有小组件却指定了共享组
    elif app_group:
        # 报错
        raise RuntimeError('An App Group was requested without a WidgetKit extension')
    # 手机应用的开发权限
    phone_entitlements = development_entitlements(phone, bundle_id, team, fingerprint, device=phone_device,
                                                  # 共享组列表：有扩展时才有
                                                  app_groups=[group] if extensions else [])
    # 报告：来源、身份、描述文件摘要与代码摘要；强调不会安装应用
    report = {'sourceApplication': str(app), 'sourceBundleId': info['CFBundleIdentifier'],
            # 包标识、手表标识、团队与证书
            'bundleId': bundle_id, 'watchBundleId': watch_id, 'team': team, 'certificateSha1': fingerprint,
            # 手机与手表设备编号
            'phoneDeviceUdid': phone_device, 'watchDeviceUdid': watch_device,
            # 手机描述文件摘要
            'phoneProfile': profile_summary(phone, phone_entitlements, phone_device),
            # 手机指令
            'phoneInstructions': instructions(executable(app, info)),
            # 应用包摘要，不安装
            'nshBundleSha256': nsh_bundle_hash(app), 'installsApplication': False}
    # 加入 Watch 部分
    if children:
        # 取出手表应用与信息
        watch, watch_info = children[0]
        # 解析手表描述文件
        child = profile(watch_profile)
        # 手表应用的开发权限
        watch_entitlements = development_entitlements(child, watch_id, team, fingerprint, device=watch_device)
        # 写入手表路径与标识
        report.update({'watchRelativePath': str(watch.relative_to(app)),
                       # 手表描述文件摘要
                       'watchProfile': profile_summary(child, watch_entitlements, watch_device),
                       # 手表指令
                       'watchInstructions': instructions(executable(watch, watch_info))})
    # 加入 Widget 部分
    if extensions:
        # 取出小组件扩展与信息
        widget, widget_info = extensions[0]
        # 小组件标识
        widget_id = bundle_id + '.widgets'
        # 解析小组件描述文件
        child = profile(widget_profile)
        # 小组件的开发权限
        widget_entitlements = development_entitlements(child, widget_id, team, fingerprint,
                                                       # 设备与共享组
                                                       device=phone_device, app_groups=[group])
        # 写入小组件路径与标识
        report.update({'widgetRelativePath': str(widget.relative_to(app)), 'widgetBundleId': widget_id,
                       # 小组件共享组
                       'widgetAppGroup': group,
                       # 小组件描述文件摘要
                       'widgetProfile': profile_summary(child, widget_entitlements, phone_device),
                       # 小组件指令
                       'widgetInstructions': instructions(executable(widget, widget_info))})
    # 返回报告
    return report


# 输出必须是源包之外的全新路径
def validate_output(app, output):
    # 输出已存在、是符号链接或落在源码目录内
    if output.exists() or output.is_symlink() or output.resolve().is_relative_to(app.resolve()):
        # 报错：输出必须是源码目录之外的新应用
        raise RuntimeError('Output must be a new application outside the source bundle')


# 复制并签名：先签嵌入的组件，最后签手机应用
def sign_copy(app, output, report, phone_profile, watch_profile=None, widget_profile=None):
    # 校验输出路径
    validate_output(app, output)
    # 创建输出目录
    output.parent.mkdir(parents=True, exist_ok=True)
    # 复制应用，保留符号链接
    shutil.copytree(app, output, symlinks=True)
    # 签名顺序：Widget、Watch、手机
    targets = []
    # 有小组件
    if report.get('widgetRelativePath'):
        # 加入签名目标
        targets.append(('widget', output / report['widgetRelativePath'], report['widgetBundleId'],
                        # 小组件描述文件与权限
                        widget_profile, report['widgetProfile']['entitlements']))
    # 有手表
    if report.get('watchRelativePath'):
        # 加入签名目标
        targets.append(('watch', output / report['watchRelativePath'], report['watchBundleId'],
                        # 手表描述文件与权限
                        watch_profile, report['watchProfile']['entitlements']))
    # 最后加入手机应用本身
    targets.append(('phone', output, report['bundleId'], phone_profile, report['phoneProfile']['entitlements']))
    # 逐个改标识、嵌入描述文件并签名
    for name, target, bundle_id, provision, entitlements in targets:
        # 读取目标 Info.plist
        info = plistlib.loads((target / 'Info.plist').read_bytes())
        # 写入新的包标识
        info['CFBundleIdentifier'] = bundle_id
        # 手表目标
        if name == 'watch':
            # 写入伴生应用标识
            info['WKCompanionAppBundleIdentifier'] = report['bundleId']
        # 有小组件共享组时
        elif report.get('widgetAppGroup'):
            # 写入共享组
            info['LokiWidgetAppGroup'] = report['widgetAppGroup']
        # 写回 Info.plist
        (target / 'Info.plist').write_bytes(plistlib.dumps(info))
        # 复制描述文件
        shutil.copy2(provision, target / 'embedded.mobileprovision')
        # 权限文件路径
        entitlement_file = output.parent / f'{name}-entitlements.plist'
        # 写入权限
        entitlement_file.write_bytes(plistlib.dumps(entitlements))
        # 调用 codesign 签名
        command(['codesign', '--force', '--sign', report['certificateSha1'], '--identifier', bundle_id,
                 # 指定权限文件与不加时间戳
                 '--entitlements', entitlement_file, '--generate-entitlement-der', '--timestamp=none', target])
        # 读回签名后的权限，必须与评审过的请求一致
        signed = plistlib.loads(command(['codesign', '--display', '--entitlements', ':-', target]))
        # 实际签得的权限与评审过的不一致
        if signed != entitlements:
            # 报错
            raise RuntimeError('Signed entitlements differ from the reviewed development request')
    # 整体校验
    command(['codesign', '--verify', '--deep', '--strict', output])
    # 读取手机 Info.plist
    phone_info = plistlib.loads((output / 'Info.plist').read_bytes())
    # 签名不得改动已链接好的手机代码
    if instructions(executable(output, phone_info)) != report['phoneInstructions']:
        # 报错：签名改变了预链接指令
        raise RuntimeError('Signing changed the prelinked phone instructions')
    # 内嵌组件同样不得被改动
    for name, target, _, _, _ in targets[:-1]:
        # 读取子包 Info.plist
        child_info = plistlib.loads((target / 'Info.plist').read_bytes())
        # 子包的指令也不得改变
        if instructions(executable(target, child_info)) != report[name + 'Instructions']:
            # 报错
            raise RuntimeError('Signing changed the prelinked embedded instructions')
        # 记录指令已保持
        report[name + 'InstructionsPreserved'] = True
    # NSH 应用包也必须保持不变
    if nsh_bundle_hash(output) != report['nshBundleSha256']:
        # 报错：签名改变了 NSH 包
        raise RuntimeError('Signing changed the NSH bundle')
    # 记录已签名应用与签名校验结果
    report.update({'signedApplication': str(output.resolve()), 'signatureVerified': True,
                   # 手机指令已保持
                   'phoneInstructionsPreserved': True,
                   # 应用包已保持
                   'nshBundlePreserved': True})
    # 写出签名报告
    (output.parent / 'signed-application.json').write_text(json.dumps(report, indent=2) + '\n')


# 命令行入口
def main():
    # 创建参数解析器
    parser = argparse.ArgumentParser(description=__doc__)
    # 待签名的应用
    parser.add_argument('--app', required=True, type=Path)
    # 输出路径
    parser.add_argument('--output', required=True, type=Path)
    # 包标识参数
    parser.add_argument('--bundle-id', required=True)
    # 团队参数
    parser.add_argument('--team', required=True)
    # 已有开发签名身份的完整 SHA-1
    parser.add_argument('--identity', required=True, help='Exact SHA-1 of the existing development signing identity')
    # 手机描述文件
    parser.add_argument('--phone-profile', required=True, type=Path)
    # 手表描述文件
    parser.add_argument('--watch-profile', type=Path, help='Required when the archive contains its Watch companion')
    # 小组件描述文件
    parser.add_argument('--widget-profile', type=Path, help='Required when the archive contains its WidgetKit extension')
    # 共享组
    parser.add_argument('--app-group', help='Explicit shared group; defaults to group.<signed phone bundle ID>')
    # 手机 UDID，必须已在描述文件里
    parser.add_argument('--device', required=True, help='Phone UDID already included in the development profile')
    # 已知的手表设备
    parser.add_argument('--watch-device', help='Optional known Watch UDID; its absence does not block iPhone signing')
    # 默认只生成计划；加此参数才创建并签名副本，仍然不会安装
    parser.add_argument('--execute', action='store_true', help='Create and development-sign a copy; still never installs')
    # 解析参数
    args = parser.parse_args()
    # 规范化证书指纹
    fingerprint = args.identity.replace(':', '').upper()
    # 指纹与团队 ID 格式检查
    if not re.fullmatch(r'[A-F0-9]{40}', fingerprint) or not re.fullmatch(r'[A-Z0-9]{10}', args.team):
        # 指纹或团队格式不对
        parser.error('Specify an exact certificate SHA-1 and ten-character development team')
    # 标识与输出路径检查
    if not re.fullmatch(r'[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+', args.bundle_id) or args.output.suffix != '.app':
        # 包标识或输出路径不对
        parser.error('Specify a bundle identifier and a new .app output path')
    # 校验输出路径
    validate_output(args.app, args.output)
    # 生成并保存签名计划
    report = plan(args.app, args.bundle_id, args.team, fingerprint, args.phone_profile,
                  # 手表描述文件、设备与小组件描述文件
                  args.watch_profile, args.device, args.watch_device, args.widget_profile, args.app_group)
    # 创建输出目录
    args.output.parent.mkdir(parents=True, exist_ok=True)
    # 写出签名计划
    (args.output.parent / 'signing-plan.json').write_text(json.dumps(report, indent=2) + '\n')
    # 带 --execute 才真正签名
    if args.execute:
        # 对副本签名
        sign_copy(args.app.resolve(), args.output, report, args.phone_profile, args.watch_profile, args.widget_profile)
    # 打印报告
    print(json.dumps(report, indent=2))


# 作为脚本运行时
if __name__ == '__main__':
    # 调用入口
    main()
