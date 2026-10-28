"""Entitlement construction and signing instructions for the Apple device plan."""
from datetime import datetime, timezone
import hashlib
from pathlib import Path
import re
import struct


# 描述文件里的标识模式是否允许某个标识：精确匹配，或形如 前缀.* 的通配
def permits(pattern, identifier):
    # 没有通配符则要求完全相等
    if '*' not in pattern:
        # 完全匹配
        return pattern == identifier
    # 只允许结尾 .* 的单个通配符，按前缀匹配
    return pattern.endswith('.*') and pattern.count('*') == 1 and identifier.startswith(pattern[:-1])


# 校验开发描述文件的授权，并生成要请求的权限；同时校验显式请求的共享容器
def development_entitlements(value, bundle_id, team, fingerprint, *, device=None, app_groups=()):
    # 文档说明：校验开发授权与显式请求的共享容器
    """Validate the development grant and explicitly requested shared containers."""
    # 必须有且未过期
    expires = value.get('ExpirationDate')
    # 到期时间不是日期类型
    if not isinstance(expires, datetime):
        # 描述文件没有到期时间
        raise RuntimeError('Provisioning profile has no expiry')
    # 已过期
    if expires.replace(tzinfo=timezone.utc) <= datetime.now(timezone.utc):
        # 描述文件已过期
        raise RuntimeError('Provisioning profile has expired')
    # 团队必须一致
    if value.get('TeamIdentifier') != [team]:
        # 描述文件属于其他开发团队
        raise RuntimeError('Provisioning profile belongs to another development team')
    # 描述文件里的权限声明
    entitlements = value.get('Entitlements', {})
    # 必须是面向已注册设备的开发描述文件
    if entitlements.get('get-task-allow') is not True or not value.get('ProvisionedDevices'):
        # 应是面向已注册设备的开发描述文件
        raise RuntimeError('Expected a development profile for registered devices')
    # 目标设备必须在描述文件里
    if device and device not in value['ProvisionedDevices']:
        # 目标设备不在描述文件内
        raise RuntimeError('The target device is not included in the provisioning profile')
    # 描述文件必须包含所选的签名证书
    certificates = {hashlib.sha1(item).hexdigest().upper() for item in value.get('DeveloperCertificates', [])}
    # 所选证书不在描述文件的证书列表
    if fingerprint not in certificates:
        # 报错
        raise RuntimeError('Provisioning profile does not include the selected signing certificate')
    # 应用标识必须被描述文件允许
    grant = entitlements.get('application-identifier', '')
    # 应用标识前缀
    prefix = grant.split('.', 1)[0]
    # 完整的应用标识
    requested_id = prefix + '.' + bundle_id
    # 前缀不在描述文件里，或不允许该应用标识
    if prefix not in value.get('ApplicationIdentifierPrefix', []) or not permits(grant, requested_id):
        # 报错
        raise RuntimeError('Provisioning profile does not permit the requested application identifier')
    # 私有钥匙串访问组也必须被允许
    if not any(permits(pattern, requested_id) for pattern in entitlements.get('keychain-access-groups', [])):
        # 描述文件不允许私有钥匙串组
        raise RuntimeError('Provisioning profile does not permit the private Keychain access group')
    # 要请求的权限
    request = {'application-identifier': requested_id, 'com.apple.developer.team-identifier': team,
               # 允许调试，钥匙串组为请求的标识
               'get-task-allow': True, 'keychain-access-groups': [requested_id]}
    # 去重并校验 App Group 的格式
    groups = list(dict.fromkeys(app_groups))
    # 请求的共享组格式必须规范
    if any(not re.fullmatch(r'group\.[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+', group) for group in groups):
        # 报错：共享组标识无效
        raise RuntimeError('Invalid requested App Group identifier')
    # 且必须全部被描述文件允许
    if not set(groups).issubset(entitlements.get('com.apple.security.application-groups', [])):
        # 描述文件不允许请求的共享组
        raise RuntimeError('Provisioning profile does not permit the requested App Group')
    # 有共享组
    if groups:
        # 写入应用组权限
        request['com.apple.security.application-groups'] = groups
    # 返回最终请求的权限
    return request


# 取包的可执行文件路径：名字不能含路径成分，且不能逃出应用目录
def executable(app, info):
    # 可执行文件名
    name = info['CFBundleExecutable']
    # 名称不能含路径或为空
    if Path(name).name != name or name in ['', '.', '..']:
        # 报错
        raise RuntimeError('Invalid bundle executable path')
    # 拼出路径
    result = app / name
    # 路径不能逃出应用目录
    if not result.resolve().is_relative_to(app.resolve()):
        # 报错
        raise RuntimeError('Bundle executable escapes its application')
    # 返回路径
    return result


# 对每个 Mach-O 切片的 __TEXT,__text 求摘要，与签名块无关，用来证明签名没有改动代码
def instructions(path):
    # 文档说明：对每个 Mach-O 切片取 __TEXT,__text 摘要，与签名块无关
    """Hash __TEXT,__text in every Mach-O slice, independent of signing blobs."""
    # 读取文件
    data = path.read_bytes()
    # 通用二进制头的魔数与字节序
    fat = {b'\xca\xfe\xba\xbe': ('>', False), b'\xca\xfe\xba\xbf': ('>', True),
           # 胖二进制魔数：字节序与是否 64 位
           b'\xbe\xba\xfe\xca': ('<', False), b'\xbf\xba\xfe\xca': ('<', True)}
    # 通用二进制：读出各切片的起始偏移
    if data[:4] in fat:
        # 字节序与是否 64 位
        endian, wide = fat[data[:4]]
        # 切片数量
        count = struct.unpack_from(endian + 'I', data, 4)[0]
        # 各切片起点
        bases = [struct.unpack_from(endian + ('Q' if wide else 'I'), data,
                                    # 各切片起点
                                    8 + index * (32 if wide else 20) + 8)[0] for index in range(count)]
    # 否则只有一个切片
    else:
        # 单架构文件
        bases = [0]
    # 单个 Mach-O 的魔数、字节序与头大小
    formats = {b'\xcf\xfa\xed\xfe': ('<', 32), b'\xce\xfa\xed\xfe': ('<', 28),
               # Mach-O 魔数：字节序与头大小
               b'\xfe\xed\xfa\xcf': ('>', 32), b'\xfe\xed\xfa\xce': ('>', 28)}
    # 结果表
    result = {}
    # 逐个切片处理
    for base in bases:
        # 字节序与头大小
        endian, header_size = formats[data[base:base + 4]]
        # CPU 类型与子类型
        cpu, subtype = struct.unpack_from(endian + 'II', data, base + 4)
        # 加载命令数量
        count = struct.unpack_from(endian + 'I', data, base + 16)[0]
        # 加载命令起点
        position = base + header_size
        # 尚未找到 __text
        found = False
        # 逐个加载命令
        for _ in range(count):
            # 命令类型与大小
            kind, size = struct.unpack_from(endian + 'II', data, position)
            # 段命令（32 位或 64 位）
            if kind in [1, 0x19]:
                # 是否 64 位段命令
                wide = kind == 0x19
                # 段内节的数量
                section_count = struct.unpack_from(endian + 'I', data, position + (64 if wide else 48))[0]
                # 找 __TEXT,__text 节
                for index in range(section_count):
                    # 节头起点
                    section = position + (72 if wide else 56) + index * (80 if wide else 68)
                    # 不是 __text 节则跳过
                    if data[section:section + 16].split(b'\0')[0] != b'__text':
                        # 跳过
                        continue
                    # 不在 __TEXT 段则跳过
                    if data[section + 16:section + 32].split(b'\0')[0] != b'__TEXT':
                        # 跳过
                        continue
                    # 节长度
                    length = struct.unpack_from(endian + ('Q' if wide else 'I'), data, section + (40 if wide else 36))[0]
                    # 节偏移
                    offset = struct.unpack_from(endian + 'I', data, section + (48 if wide else 40))[0]
                    # 截取文本节区
                    body = data[base + offset:base + offset + length]
                    # 长度不符
                    if len(body) != length:
                        # 文本节区被截断
                        raise RuntimeError('Truncated Mach-O text section')
                    # 以 CPU 类型和子类型为键保存摘要
                    result[f'{cpu:08x}.{subtype:08x}'] = hashlib.sha256(body).hexdigest()
                    # 标记已找到
                    found = True
            # 移到下一条命令
            position += size
        # 该切片没有 __text
        if not found:
            # 报错
            raise RuntimeError('Mach-O slice has no __TEXT,__text')
    # 返回各切片的摘要
    return result


# 描述文件摘要：用于写入报告
def profile_summary(value, entitlements, device):
    # 把描述文件整理成摘要信息
    return {'uuid': value['UUID'], 'name': value['Name'], 'team': value['TeamIdentifier'],
            # 到期时间与权限
            'expires': value['ExpirationDate'].isoformat(), 'entitlements': entitlements,
            # 已登记设备数
            'provisionedDeviceCount': len(value.get('ProvisionedDevices', [])),
            # 指定设备是否在其中
            'specifiedDeviceCovered': device in value.get('ProvisionedDevices', []) if device else None}
