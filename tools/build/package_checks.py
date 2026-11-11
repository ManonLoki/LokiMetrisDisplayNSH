"""Runner and Android package checks, and the embedded Apple identifier reader."""
from pathlib import Path
import plistlib
import struct
import zipfile


# 生成的运行器必须依赖本应用的 Rust 模块
def verify_runner(target, root):
    # 文档说明：生成的运行器必须依赖本应用的 Rust 模块
    """Generated runners must include the local application's Rust module."""
    # TOML 解析器
    import tomllib
    # 按平台选择运行器的 Cargo.toml
    path = root / '.nsh/rust' / ('android/Cargo.toml' if target == 'android' else 'ios-app/Cargo.toml')
    # 读取依赖表
    dependencies = tomllib.loads(path.read_text()).get('dependencies', {}).values()
    # 没有任何依赖指向本仓库
    if not any(isinstance(value, dict) and value.get('path')
               # （续）路径解析后等于仓库根目录
               and Path(value['path']).resolve() == root for value in dependencies):
        # 报错：运行器漏掉本应用模块
        raise RuntimeError('NSH runner omitted the local application Rust module')


ANDROID_ABI = 'arm64-v8a'


def verify_android(path):
    """Require the app bundle and an ARM64-only native runtime."""
    runtime = f'lib/{ANDROID_ABI}/libnsh.so'
    with zipfile.ZipFile(path) as archive:
        names = archive.namelist()
        if not any(name.endswith('bundle.nshc') for name in names):
            raise RuntimeError('APK is missing the NSH application bundle')
        abis = {name.split('/')[1] for name in names
                if name.startswith('lib/') and name.endswith('.so')}
        if abis != {ANDROID_ABI}:
            raise RuntimeError(f'Expected only {ANDROID_ABI} libraries in APK, found {sorted(abis)}')
        if names.count(runtime) != 1:
            raise RuntimeError(f'APK must contain exactly one {runtime}')
        with archive.open(runtime) as library:
            header = library.read(20)
        # ELF64, little-endian, EM_AARCH64.
        if header[:6] != b'\x7fELF\x02\x01' or int.from_bytes(header[18:20], 'little') != 183:
            raise RuntimeError('Invalid NSH runtime architecture: ' + ANDROID_ABI)
    return {'architectures': [ANDROID_ABI], 'bundleVerified': True}


# 从模拟器 Mach-O 的 __TEXT,__entitlements 节读出内嵌的应用标识
def embedded_apple_identifier(executable):
    # 以二进制方式打开可执行文件
    with executable.open('rb') as source:
        # 读取 Mach-O 头
        header = source.read(32)
        # 必须是 64 位小端 Mach-O
        if header[:4] != b'\xcf\xfa\xed\xfe':
            # 不是 64 位模拟器可执行文件
            raise RuntimeError('Expected a 64-bit simulator Mach-O runner')
        # 加载命令数量
        commands = struct.unpack_from('<I', header, 16)[0]
        # 遍历加载命令
        for _ in range(commands):
            # 记下当前位置
            position = source.tell()
            # 读取命令类型与大小
            kind, size = struct.unpack('<II', source.read(8))
            # 回到命令开头
            source.seek(position)
            # 读出整条命令
            command = source.read(size)
            # 只看 64 位段命令
            if kind != 0x19:
                # 不是段命令则跳过
                continue
            # 逐个节查找 __TEXT,__entitlements
            for index in range(struct.unpack_from('<I', command, 64)[0]):
                # 节头偏移
                section = 72 + index * 80
                # 节名
                name = command[section:section + 16].split(b'\0')[0]
                # 段名
                segment = command[section + 16:section + 32].split(b'\0')[0]
                # 找到 __TEXT 里的 __entitlements 节
                if name == b'__entitlements' and segment == b'__TEXT':
                    # 节长度
                    length = struct.unpack_from('<Q', command, section + 40)[0]
                    # 节偏移
                    offset = struct.unpack_from('<I', command, section + 48)[0]
                    # 跳到该节
                    source.seek(offset)
                    # 返回应用标识
                    return plistlib.loads(source.read(length)).get('application-identifier')
    # 缺少钥匙串标识权限
    raise RuntimeError('Simulator runtime is missing its embedded Keychain identity entitlement')
