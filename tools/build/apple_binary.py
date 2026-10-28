"""Mach-O and bundle inspection for the companion archive."""
import hashlib
from pathlib import Path
import plistlib
import shutil
import struct


# 代码签名可以变，但预链接的 __text 指令不能变：对其求摘要
def machine_code_hash(executable):
    # 文档说明：签名可以变，预链接的指令不能变
    """Code signatures may change; the prelinked __text instructions must not."""
    # 以二进制方式打开
    with executable.open('rb') as source:
        # 读取 Mach-O 头
        header = source.read(32)
        # 必须是单架构 64 位 Mach-O
        if header[:4] != b'\xcf\xfa\xed\xfe':
            # 不是单架构 64 位可执行文件
            raise RuntimeError('Expected a thin 64-bit NSH Mach-O executable')
        # 遍历加载命令
        for _ in range(struct.unpack_from('<I', header, 16)[0]):
            # 记下位置
            position = source.tell()
            # 读取命令大小
            _, size = struct.unpack('<II', source.read(8))
            # 回到开头并读出整条命令
            source.seek(position); command = source.read(size)
            # 只看 64 位段命令
            if struct.unpack_from('<I', command)[0] != 0x19:
                # 不是段命令则跳过
                continue
            # 找 __text 节
            for index in range(struct.unpack_from('<I', command, 64)[0]):
                # 节头偏移
                section = 72 + index * 80
                # 找到 __text 节
                if command[section:section + 16].split(b'\0')[0] == b'__text':
                    # 节长度
                    length = struct.unpack_from('<Q', command, section + 40)[0]
                    # 节偏移
                    offset = struct.unpack_from('<I', command, section + 48)[0]
                    # 跳到该节
                    source.seek(offset)
                    # 返回其 SHA-256
                    return hashlib.sha256(source.read(length)).hexdigest()
    # 没有 __text 节
    raise RuntimeError('NSH executable has no __text section')


# 读取每个切片的 LC_BUILD_VERSION 平台；仅凭 ARM64 无法区分平台
def macho_platforms(executable):
    # 文档说明：检查每个切片的 LC_BUILD_VERSION
    """Inspect LC_BUILD_VERSION in every slice; ARM64 alone is ambiguous."""
    # 以二进制方式打开
    with executable.open('rb') as source:
        # 读取魔数
        magic = source.read(4)
        # 通用二进制头：魔数对应字节序与是否 64 位偏移
        fat = {b'\xca\xfe\xba\xbe': ('>', False), b'\xca\xfe\xba\xbf': ('>', True),
               # 胖二进制魔数：字节序与是否 64 位
               b'\xbe\xba\xfe\xca': ('<', False), b'\xbf\xba\xfe\xca': ('<', True)}
        # 通用二进制：读出各切片偏移
        if magic in fat:
            # 字节序与是否 64 位
            endian, wide = fat[magic]
            # 切片数量
            count = struct.unpack(endian + 'I', source.read(4))[0]
            # 切片起点列表
            bases = []
            # 逐个切片
            for _ in range(count):
                # 读取一条记录
                entry = source.read(32 if wide else 20)
                # 取出该切片的偏移
                bases.append(struct.unpack_from(endian + ('Q' if wide else 'I'), entry, 8)[0])
        # 单架构
        else:
            # 非胖二进制只有一个切片
            bases = [0]
        # 平台集合
        result = set()
        # 单个 Mach-O 头的魔数
        formats = {b'\xcf\xfa\xed\xfe': ('<', 32), b'\xce\xfa\xed\xfe': ('<', 28),
                   # Mach-O 魔数：字节序与头大小
                   b'\xfe\xed\xfa\xcf': ('>', 32), b'\xfe\xed\xfa\xce': ('>', 28)}
        # 逐个切片
        for base in bases:
            # 跳到切片并读取头
            source.seek(base); header = source.read(32)
            # 魔数不认识
            if header[:4] not in formats:
                # 报错
                raise RuntimeError('Unsupported Mach-O header')
            # 字节序与头大小
            endian, header_size = formats[header[:4]]
            # 加载命令数量
            count = struct.unpack_from(endian + 'I', header, 16)[0]
            # 跳到加载命令区
            source.seek(base + header_size)
            # 平台暂未知
            platform = None
            # 找 LC_BUILD_VERSION（0x32）
            for _ in range(count):
                # 记下位置
                position = source.tell()
                # 读取命令类型与大小
                kind, size = struct.unpack(endian + 'II', source.read(8))
                # 是构建版本命令
                if kind == 0x32:
                    # 读取平台编号
                    platform = struct.unpack(endian + 'I', source.read(4))[0]
                    # 找到即停止
                    break
                # 跳到下一条命令
                source.seek(position + size)
            # 该切片没有平台信息
            if platform is None:
                # 缺少平台标记
                raise RuntimeError('Mach-O lacks an explicit LC_BUILD_VERSION platform')
            # 加入平台集合
            result.add(platform)
        # 返回集合
        return result


# 从归档元数据取应用路径，拒绝绝对路径和含 .. 的路径
def archived_application(archive):
    # 读取归档的 Info.plist
    info = plistlib.loads((archive / 'Info.plist').read_bytes())
    # 应用相对路径
    relative = Path(info['ApplicationProperties']['ApplicationPath'])
    # 路径为绝对路径或含上级目录
    if relative.is_absolute() or '..' in relative.parts:
        # 报错
        raise RuntimeError('Invalid application path in Xcode archive metadata')
    # 返回归档内的应用路径
    return archive / 'Products' / relative


# NSH 应用包（bundle.nshc）的摘要，必须恰好有一个
def nsh_bundle_hash(app):
    # 查找应用 bundle
    bundles = list(app.rglob('bundle.nshc'))
    # 必须恰好一个
    if len(bundles) != 1:
        # 报错
        raise RuntimeError('Expected one NSH application bundle in the iPhone app')
    # 返回 bundle 的摘要
    return hashlib.sha256(bundles[0].read_bytes()).hexdigest()


# 对工程源码求摘要，把归档绑定到稳定的源码快照；忽略 Xcode 用户状态
def watch_source_hashes(project):
    # 文档说明：绑定到稳定的源码快照，排除 Xcode 用户状态
    """Bind the archive to a stable source snapshot, excluding Xcode user state."""
    # 工程所在目录
    root = project.parent
    # 忽略的文件名
    ignored = {'xcuserdata', 'project.xcworkspace', '.DS_Store', '__pycache__', '.build', 'build', 'target', '.git'}
    # 相对路径到摘要的映射
    return {str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
            # 遍历所有文件
            for path in sorted(root.rglob('*'))
            # 只收普通文件且不在忽略名单
            if path.is_file() and not ignored.intersection(path.relative_to(root).parts)}


# 复制应用，并移除之前组合时留下的子应用，只保留本次 Xcode 构建的产物
def prepare_app(app, prepared, companion_id):
    # 复制应用，保留符号链接
    shutil.copytree(app, prepared, symlinks=True)
    # 重新组装可能复用先前合并好的手机包。按伴侣应用元数据移除其旧的子包,
    # 这样只会暂存当前由 Xcode 构建的手表产物。
    # 按配套标识移除旧的手表应用
    for child in list(prepared.rglob('*.app')):
        # 读取子包 Info.plist
        metadata = plistlib.loads((child / 'Info.plist').read_bytes())
        # 是手表配套应用
        if metadata.get('WKCompanionAppBundleIdentifier') == companion_id:
            # 删除之
            shutil.rmtree(child)
    # 移除旧的小组件扩展
    for child in list(prepared.rglob('*.appex')):
        # 读取子包 Info.plist
        metadata = plistlib.loads((child / 'Info.plist').read_bytes())
        # 是旧的小组件扩展
        if metadata.get('CFBundleIdentifier') == companion_id + '.widgets':
            # 删除之
            shutil.rmtree(child)


# 手机与内嵌应用的构建号和营销版本必须一致
def verify_versions(phone, child):
    # 构建号不一致
    if str(child['CFBundleVersion']) != str(phone['CFBundleVersion']):
        # 报错
        raise RuntimeError('Phone and embedded application build versions differ')
    # 营销版本号不一致
    if child['CFBundleShortVersionString'] != phone['CFBundleShortVersionString']:
        # 报错
        raise RuntimeError('Phone and embedded application marketing versions differ')
