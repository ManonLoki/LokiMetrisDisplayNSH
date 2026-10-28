"""Runner and Android package checks, and the embedded Apple identifier reader."""
from pathlib import Path
import plistlib
import shutil
import struct
import zipfile


# 让 CLI 只为项目支持的 64 位 ABI 构建，而不修改全局设置
def android_runtime_seed(cli, stage, env, root):
    # 文档说明：让 CLI 只构建受支持的 64 位 ABI
    """Let the CLI build the project's supported 64-bit ABIs without global edits."""
    # 官方运行时的位置
    original = Path(env.get('NSH_RUNTIME') or Path(cli).resolve().parent.parent / 'runtime') / 'android/release'
    # 种子目录
    seed = stage / 'runtime-seed/android/release'
    # 创建种子目录
    seed.mkdir(parents=True)
    # 复制 classes.dex
    shutil.copy2(original / 'classes.dex', seed / 'classes.dex')
    # 只保留两个受支持的 ABI
    for abi in ['arm64-v8a', 'x86_64']:
        # 各 ABI 的运行时库
        source = original / 'lib' / abi / 'libnsh.so'
        # 库缺失
        if not source.is_file():
            # 报错
            raise RuntimeError('Installed NSH runtime is missing the supported ABI: ' + abi)
        # 目标 ABI 目录
        directory = seed / 'lib' / abi
        # 创建目录
        directory.mkdir(parents=True)
        # 复制运行时库
        shutil.copy2(source, directory / 'libnsh.so')
    # Cargo 会重新构建这些库;种子只决定存在哪些 ABI。
    # Cargo 会重新编译这些库；种子只决定存在哪些 ABI
    env['NSH_RUNTIME'] = str(stage / 'runtime-seed')
    # CLI 生成的库目录
    generated = root / '.nsh/rust/android-release/lib'
    # 清掉生成目录里其余 ABI
    if generated.is_dir():
        # 遍历生成的 ABI 目录
        for path in generated.iterdir():
            # 只保留受支持的两种 ABI
            if path.name not in ['arm64-v8a', 'x86_64']:
                # 删除其他 ABI
                shutil.rmtree(path)


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


# 校验 APK：包含应用包与各 ABI 的运行时，且 ELF 架构正确
def verify_android(path):
    # 打开 APK 压缩包
    with zipfile.ZipFile(path) as archive:
        # 列出全部条目
        names = archive.namelist()
        # 缺少应用 bundle
        if not any(name.endswith('bundle.nshc') for name in names):
            # 报错
            raise RuntimeError('APK is missing the NSH application bundle')
        # 收集 NSH 运行时库
        libraries = [name for name in names if name.startswith('lib/') and name.endswith('/libnsh.so')]
        # 没有运行时库
        if not libraries:
            # 报错
            raise RuntimeError('APK is missing its NSH Rust runtime')
        # 架构列表
        architectures = []
        # 逐个库
        for name in libraries:
            # 取 ABI 目录名
            abi = name.split('/')[1]
            # 各 ABI 对应的 ELF 机器码
            expected = {'arm64-v8a': 183, 'x86_64': 62, 'armeabi-v7a': 40, 'x86': 3}.get(abi)
            # 打开该库
            with archive.open(name) as library:
                # 读取 ELF 头
                header = library.read(20)
            # 魔数和机器码都要对
            if expected is None or header[:4] != b'\x7fELF' or int.from_bytes(header[18:20], 'little') != expected:
                # 架构与目录名不符则报错
                raise RuntimeError('Invalid NSH runtime architecture: ' + abi)
            # 记录该架构
            architectures.append(abi)
    # 返回架构列表与 bundle 已校验
    return {'architectures': architectures, 'bundleVerified': True}


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
