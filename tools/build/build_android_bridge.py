#!/usr/bin/env python3
"""Compile the app-owned Android adapter against the unmodified official NSH API."""
# 引入标准库
import argparse
# 摘要算法
import hashlib
# 内存字节流
import io
# JSON 读写
import json
# 操作系统接口
import os
# 路径对象
from pathlib import Path
# 文件工具
import shutil
# 子进程
import subprocess
# 归档
import tarfile
# 临时目录
import tempfile
# TOML 解析
import tomllib
# 压缩包
import zipfile

# 仓库根目录
ROOT = Path(__file__).resolve().parents[2]
# 应用自有的 Java 适配层源码目录
SOURCE = ROOT / "src/native/android/java"
# 官方 NSH 版本
NSH_VERSION = '0.3.0'


# 定位 Android SDK：命令行参数、环境变量，然后是常见安装位置
def sdk_directory(explicit):
    # 命令行显式指定了 SDK
    if explicit:
        # 使用并校验其存在
        return explicit.expanduser().resolve(strict=True)
    # 其次读环境变量
    value = os.environ.get("ANDROID_HOME") or os.environ.get("ANDROID_SDK_ROOT")
    # 有则使用
    if value:
        # 使用并校验其存在
        return Path(value).expanduser().resolve(strict=True)
    # 再依次尝试常见安装位置
    candidates = [Path.home() / "Library/Android/sdk", Path.home() / "Android/Sdk"]
    # Windows 下追加 LOCALAPPDATA 位置
    if os.environ.get("LOCALAPPDATA"):
        # 加入 Windows 的默认 SDK 位置
        candidates.append(Path(os.environ["LOCALAPPDATA"]) / "Android/Sdk")
    # 逐个检查候选
    for path in candidates:
        # 是目录则采用
        if path.is_dir():
            # 返回绝对路径
            return path.resolve()
    # 找不到 SDK
    raise RuntimeError("Android SDK not found; set --sdk or ANDROID_HOME")


# 定位 javac：参数、JAVA_HOME，然后是 PATH；不会尝试安装
def java_compiler(explicit):
    # 显式指定了 javac
    if explicit:
        # 使用并校验其存在
        return explicit.expanduser().resolve(strict=True)
    # 其次读 JAVA_HOME
    if os.environ.get("JAVA_HOME"):
        # 拼出 javac 路径
        path = Path(os.environ["JAVA_HOME"]) / "bin" / ("javac.exe" if os.name == "nt" else "javac")
        # 存在
        if path.is_file():
            # 返回
            return path.resolve()
    # 最后在 PATH 中查找
    path = shutil.which("javac")
    # 已有路径
    if path:
        # 返回绝对路径
        return Path(path).resolve()
    # 找不到 JDK，不尝试安装
    raise RuntimeError("An existing JDK with javac is required; no installation attempted")


# 取已安装平台的 android.jar
def platform_jar(sdk, api):
    # 候选平台 jar 路径
    candidates = [sdk / 'platforms' / name / 'android.jar' for name in
                  # 两种命名：android-N 与 android-N.0
                  ['android-' + str(api), 'android-' + str(api) + '.0']]
    # 取第一个存在的
    path = next((candidate for candidate in candidates if candidate.is_file()), None)
    # 都不存在
    if path is None:
        # 报错
        raise RuntimeError('Installed Android platform jar missing for API ' + str(api))
    # 返回平台 jar
    return path


# 定位官方 NSH 的 AAR：参数或 nsh 可执行文件旁边；必须在应用源码树之外
def official_aar(explicit=None):
    # 显式指定了 AAR
    if explicit:
        # 使用并校验其存在
        path = explicit.expanduser().resolve(strict=True)
        # 位于仓库内
        if path.is_relative_to(ROOT):
            # 报错：必须来自仓库外的官方 SDK
            raise RuntimeError('NSH AAR must come from the official installed SDK outside the app source tree')
        # 返回路径
        return path
    # 查找已安装的 nsh
    cli = shutil.which('nsh')
    # 找到则据此推出 AAR 位置
    if cli:
        # 由命令行位置推出 AAR 路径
        path = Path(cli).resolve().parent.parent / 'embed/android/nsh.aar'
        # 存在则采用
        if path.is_file():
            # 返回路径
            return path
    # 否则无官方 AAR
    return None


# 从 Cargo.lock 读取官方 Android crate 的校验和，必须唯一且一致
def locked_android_checksum():
    # 校验和集合
    checksums = set()
    # 检查两份锁文件
    for path in [ROOT / 'Cargo.lock', ROOT / '.nsh/rust/Cargo.lock']:
        # 文件不存在则跳过
        if not path.is_file():
            # 跳过
            continue
        # 遍历锁文件里的包
        for package in tomllib.loads(path.read_text()).get('package', []):
            # 找到官方的 Android 平台包且版本相符
            if (package['name'] == 'nsh-platform-android' and package['version'] == NSH_VERSION
                    # 来源必须是 crates.io 注册表
                    and package.get('source') == 'registry+https://github.com/rust-lang/crates.io-index'):
                # 有校验和
                if package.get('checksum'):
                    # 加入集合
                    checksums.add(package['checksum'])
    # 校验和缺失或不一致
    if len(checksums) != 1:
        # 报错
        raise RuntimeError('Official NSH Android crate checksum is missing or inconsistent in Cargo.lock; generate the standard NSH runner first')
    # 返回唯一的校验和
    return checksums.pop()


# 对照由 Cargo.lock 认证的发布包，检查缓存里的官方 Java 源码
def official_sources():
    # 文档说明：用 Cargo 校验和验证缓存的 Java 源码与发布包一致
    """Compare cached Java against a release archive authenticated by the Cargo lockfile."""
    # 取锁定的校验和
    checksum = locked_android_checksum()
    # Cargo 主目录
    cargo = Path(os.environ.get('CARGO_HOME', Path.home() / '.cargo'))
    # 遍历缓存中的官方包源码目录
    for source in sorted((cargo / 'registry/src').glob('index.crates.io-*/nsh-platform-android-' + NSH_VERSION)):
        # 收集其中的 Java 源文件
        sources = sorted((source / 'java').rglob('*.java'))
        # 没有 Java 源码
        if not sources:
            # 跳过
            continue
        # 压缩包与旧式校验文件，两者择一核对
        archive = source.parents[2] / 'cache' / source.parent.name / (source.name + '.crate')
        # 旧式校验和文件
        legacy = source / '.cargo-checksum.json'
        # 压缩包存在：先核对其 SHA-256，再逐个文件算摘要
        if archive.is_file():
            # 发布包摘要与锁定校验和不符
            if hashlib.sha256(archive.read_bytes()).hexdigest() != checksum:
                # 报错
                raise RuntimeError('Official Cargo archive checksum differs: ' + source.name)
            # 打开发布包
            with tarfile.open(archive, 'r:gz') as release:
                # 包内路径前缀
                prefix = source.name + '/'
                # 期望的 Java 文件摘要表
                expected = {member.name[len(prefix):]: hashlib.sha256(release.extractfile(member).read()).hexdigest()
                            # 只取 java/ 下的 .java 文件
                            for member in release.getmembers() if member.isfile()
                            # （续）筛选条件
                            and member.name.startswith(prefix + 'java/') and member.name.endswith('.java')}
        # 旧式校验文件：核对包摘要并取出文件摘要
        elif legacy.is_file():
            # 读取旧式校验和
            checksums = json.loads(legacy.read_text())
            # 包校验和不符
            if checksums.get('package') != checksum:
                # 报错
                raise RuntimeError('Official Cargo package checksum differs: ' + source.name)
            # 期望的 Java 文件摘要表
            expected = {name: digest for name, digest in checksums.get('files', {}).items()
                        # （续）只取 java/ 下的 .java 文件
                        if name.startswith('java/') and name.endswith('.java')}
        # 两种来源都没有
        else:
            # 跳过该候选
            continue
        # 文件清单必须与官方完全一致
        if {path.relative_to(source).as_posix() for path in sources} != set(expected):
            # 报错：Java 源文件清单不一致
            raise RuntimeError('Official Cargo Java source inventory differs: ' + source.name)
        # 每个文件的摘要也要一致
        for path in sources:
            # 相对路径
            relative = path.relative_to(source).as_posix()
            # 文件摘要与期望不符
            if hashlib.sha256(path.read_bytes()).hexdigest() != expected[relative]:
                # 报错
                raise RuntimeError('Official Cargo Java source checksum differs: ' + relative)
        # 返回源码目录、源文件与校验和
        return source, sources, checksum
    # 找不到官方 AAR 也没有缓存的 crates.io 源码
    raise RuntimeError('Official NSH AAR or cached crates.io NSH ' + NSH_VERSION + ' Java sources are required')


# 读出 jar 中的全部文件
def jar_entries(data):
    # 把 jar 读成 文件名到内容 的映射
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        # （续）逐个条目
        return {entry.filename: archive.read(entry) for entry in archive.infolist() if not entry.is_dir()}


# 写出可复现的 jar：条目排序，时间戳与权限固定
def write_jar(path, entries):
    # 写出 jar
    with zipfile.ZipFile(path, 'w', zipfile.ZIP_DEFLATED) as archive:
        # 按名称排序，保证可复现
        for name, data in sorted(entries.items()):
            # 固定时间戳
            info = zipfile.ZipInfo(name, (2020, 1, 1, 0, 0, 0))
            # 使用压缩
            info.compress_type = zipfile.ZIP_DEFLATED
            # 固定文件权限
            info.external_attr = 0o100644 << 16
            # 写入条目
            archive.writestr(info, data)


# 把应用自有的 Java 适配层编译到官方 NSH 之上
def build(args):
    # 定位 SDK
    sdk = sdk_directory(args.sdk)
    # 定位平台 jar
    android = platform_jar(sdk, args.compile_sdk)
    # 定位 javac
    javac = java_compiler(args.javac)
    # 读取 javac 版本
    version = subprocess.check_output([str(javac), "-version"], stderr=subprocess.STDOUT, text=True).strip()
    # 输出目录
    out = args.out.expanduser().resolve()
    # 创建输出目录
    out.mkdir(parents=True, exist_ok=True)
    # 应用自己的 Java 源文件
    sources = sorted(SOURCE.rglob("*.java"))
    # 没有 Java 源码
    if not sources:
        # 报错
        raise RuntimeError("No app-owned Android Java adapter sources found")
    # 创建临时构建目录
    with tempfile.TemporaryDirectory(prefix="android-bridge-", dir=out.parent) as temporary:
        # 转为路径对象
        temporary = Path(temporary)
        # 类输出目录
        classes = Path(temporary) / "classes"
        # 创建目录
        classes.mkdir()
        # 编译依赖
        dependencies = [str(android)]
        # 官方 jar 的原有条目
        original = {}
        # 定位官方 AAR
        aar = official_aar(getattr(args, 'aar', None))
        # 官方 SDK 源码默认为空
        sdk_sources = []
        # 优先使用已安装的官方 AAR
        if aar:
            # 打开 AAR
            with zipfile.ZipFile(aar) as archive:
                # 读出其中的 classes.jar
                sdk_jar = archive.read('classes.jar')
            # 拆成条目映射
            original = jar_entries(sdk_jar)
            # 缺少 NshView 类
            if 'com/longbridge/nsh/NshView.class' not in original:
                # 报错
                raise RuntimeError('Official NSH AAR has no NshView API')
            # 依赖 jar 路径
            dependency = temporary / 'official-nsh.jar'
            # 写出依赖 jar
            dependency.write_bytes(sdk_jar)
            # 加入依赖列表
            dependencies.append(str(dependency))
            # 记录输入来源与摘要，证明未经修改
            sdk_input = {'kind': 'official-installed-aar', 'path': str(aar),
                         # AAR 摘要
                         'aarSha256': hashlib.sha256(aar.read_bytes()).hexdigest(),
                         # classes.jar 摘要与未修改标记
                         'classesSha256': hashlib.sha256(sdk_jar).hexdigest(), 'modified': False}
        # 否则使用经校验的官方 Java 源码
        else:
            # 改用 crates.io 缓存里的官方源码
            source, sdk_sources, checksum = official_sources()
            # 记录官方源码来源
            sdk_input = {'kind': 'official-crates-io-java', 'path': str(source), 'version': NSH_VERSION,
                         # crate 校验和与未修改标记
                         'crateChecksum': checksum, 'modified': False,
                         # 各源文件摘要
                         'sourceSha256': {p.relative_to(source).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                                          # （续）各源文件摘要
                                          for p in sdk_sources}}
        # 以 Java 8 级别编译
        command = [str(javac), '--release', '8', '-classpath', os.pathsep.join(dependencies), '-d', str(classes)]
        # 编译官方源码与应用源码
        subprocess.run(command + [str(path) for path in sdk_sources + sources], check=True)
        # 收集本应用编译出的类
        app_entries = {p.relative_to(classes).as_posix(): p.read_bytes() for p in sorted(classes.rglob('*.class'))}
        # 没有产出任何类
        if not app_entries:
            # 报错
            raise RuntimeError("javac produced no classes")
        # 应用的类不能替换官方 NSH 的类
        if original.keys() & app_entries.keys():
            # 报错：应用的类不得覆盖官方类
            raise RuntimeError('App Java adapter would replace an official NSH class')
        # 合并后写成 classes.jar
        entries = {**original, **app_entries}
        # 写出最终的 classes.jar
        write_jar(out / 'classes.jar', entries)
    # 构建报告：来源、版本、摘要与 VM 限额
    report = {
        # 源码目录与编译 SDK
        "source": "src/native/android/java", "compileSdk": args.compile_sdk, 'officialSdk': sdk_input,
        # javac、版本与 android.jar
        "javac": str(javac), "javacVersion": version, "androidJar": str(android),
        # 源文件数与类数
        "javaSources": len(sources), "classes": len([name for name in entries if name.endswith('.class')]),
        # 视图接口
        'viewApi': 'public NshView embedding API',
        # 虚拟机限制
        'vmLimits': {'heapMiB': 256, 'stackMiB': 4, 'loadMs': 10000, 'callMs': 2000, 'maxNodes': 200000},
        # 源文件摘要
        "sourceSha256": {p.relative_to(SOURCE).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources},
        # jar 摘要
        "jarSha256": hashlib.sha256((out / "classes.jar").read_bytes()).hexdigest(),
    }
    # 写入构建报告
    (out / "java-build.json").write_text(json.dumps(report, indent=2) + "\n")
    # 打印构建结果
    print("Built " + str(out / "classes.jar") + " with unmodified official NSH and " + str(len(sources)) + " app Java sources (" + version + ")")
    # 返回报告
    return report


# 命令行入口
def main():
    # 创建参数解析器
    parser = argparse.ArgumentParser(description=__doc__)
    # 输出目录
    parser.add_argument("--out", type=Path, default=ROOT / "output/build/android-bridge")
    # SDK 路径
    parser.add_argument("--sdk", type=Path)
    # 编译用的 SDK 版本
    parser.add_argument("--compile-sdk", type=int, default=37)
    # javac 路径
    parser.add_argument("--javac", type=Path)
    # 官方 AAR 路径
    parser.add_argument('--aar', type=Path, help='Official installed NSH release AAR (default: beside the nsh executable)')
    # 解析参数
    args = parser.parse_args()
    # Android 后端至少需要 API 28
    if args.compile_sdk < 28:
        # 版本太低
        parser.error("The Android backend needs API 28 or newer")
    # 开始构建
    build(args)


# 作为脚本运行时
if __name__ == "__main__":
    # 调用入口
    main()
