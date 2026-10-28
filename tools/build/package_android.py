# 文档字符串：打包官方 Java 与应用自有适配器并签名
"""Package unmodified official NSH Java with app-owned Android adapters, then sign it."""
# 引入标准库
import argparse
# 路径对象
from pathlib import Path
# 文件工具
import shutil
# 子进程
import subprocess
# 临时目录
import tempfile
# 压缩包
import zipfile

# 引入桥接构建与 SDK 定位函数
from build_android_bridge import build, platform_jar, sdk_directory
# 引入清单改写函数
from android_manifest import component_manifest

# 运行时必须导出的 JNI 初始化函数名
NATIVE_INIT = b'Java_com_manonloki_lokimetrisdisplay_bridge_DisplayApplication_nativeInit'


# 检查 libnsh.so 的 .dynsym 中是否定义了可见的 JNI 函数，而不是在调试字符串里碰巧出现
def has_native_init(data):
    # 文档字符串：检查 ELF 动态符号表里的 JNI 函数
    """Check a defined, visible JNI function in ELF .dynsym, never a debug-string occurrence."""
    # 结构解析
    import struct
    # 必须是小端 ELF，且为 32 位或 64 位
    if len(data) < 64 or data[:4] != b'\x7fELF' or data[5] != 1 or data[4] not in {1, 2}:
        # 数据不足则返回 False
        return False
    # 是否 64 位
    wide = data[4] == 2
    # 解析节头表
    try:
        # 节头表偏移
        section_offset = struct.unpack_from('<Q' if wide else '<I', data, 40 if wide else 32)[0]
        # 每项大小与数量
        stride, count = struct.unpack_from('<HH', data, 58 if wide else 46)
        # 节头最小长度
        minimum = 64 if wide else 40
        # 表头不合理则拒绝
        if stride < minimum or count == 0 or section_offset + stride * count > len(data):
            # 不合法则返回 False
            return False
        # 读取第 index 个节：类型、位置、大小、链接与表项大小
        def section(index):
            # 下标越界
            if index >= count:
                # 报错
                raise ValueError('ELF string-table section outside header table')
            # 节头偏移
            offset = section_offset + index * stride
            # 节类型
            kind = struct.unpack_from('<I', data, offset + 4)[0]
            # 节起点与大小
            start, size = struct.unpack_from('<QQ' if wide else '<II', data, offset + (24 if wide else 16))
            # 关联节下标
            link = struct.unpack_from('<I', data, offset + (40 if wide else 24))[0]
            # 条目大小
            entry = struct.unpack_from('<Q' if wide else '<I', data, offset + (56 if wide else 36))[0]
            # 节区越界
            if start + size > len(data):
                # 报错
                raise ValueError('ELF section outside library')
            # 返回各字段
            return kind, start, size, link, entry
        # 逐节查找动态符号表（类型 11）
        for index in range(count):
            # 读取该节信息
            kind, start, size, link, entry = section(index)
            # 不是动态符号表或条目太小
            if kind != 11 or entry < (24 if wide else 16):
                # 跳过
                continue
            # 取其关联的字符串表（类型 3）
            string_kind, string_start, string_size, _, _ = section(link)
            # 关联节不是字符串表
            if string_kind != 3:
                # 跳过
                continue
            # 字符串表内容
            names = data[string_start:string_start + string_size]
            # 逐个符号检查
            for offset in range(start, start + size - entry + 1, entry):
                # 符号名偏移
                name = struct.unpack_from('<I', data, offset)[0]
                # 绑定、可见性与是否已定义
                info, visibility, defined = struct.unpack_from('<BBH', data, offset + (4 if wide else 12))
                # 必须已定义、是全局/弱符号、是函数，且可见
                if defined == 0 or info >> 4 not in {1, 2} or info & 15 != 2 or visibility & 3 not in {0, 3}:
                    # 跳过
                    continue
                # 找到名称结尾
                end = names.find(b'\0', name)
                # 名字相符即找到
                if name < len(names) and end >= 0 and names[name:end] == NATIVE_INIT:
                    # 找到已定义的导出函数
                    return True
    # 解析出错一律视为不存在
    except (IndexError, ValueError, struct.error):
        # 没找到
        return False
    # 返回 False
    return False


# 校验 APK 中每个 ABI 的运行时都导出了 JNI 初始化函数
def verify_jni_exports(apk):
    # ABI 列表
    abis = []
    # 打开 APK
    with zipfile.ZipFile(apk) as archive:
        # 找出所有 libnsh.so
        libraries = [name for name in archive.namelist() if name.startswith('lib/') and name.endswith('/libnsh.so')]
        # 没有运行时库
        if not libraries:
            # 报错
            raise RuntimeError('APK has no app-owned libnsh.so runtime')
        # 逐个库
        for name in libraries:
            # 缺少导出则要求先重建运行时
            if not has_native_init(archive.read(name)):
                # 缺少导出的 JNI 初始化函数
                raise RuntimeError('Android runtime lacks exported ' + NATIVE_INIT.decode() + ': ' + name
                                   # 提示先重建应用运行时
                                   + '; rebuild the app runtime before packaging its Java bridge')
            # 记录 ABI
            abis.append(name.split('/')[1])
    # 返回排序后的 ABI 列表
    return sorted(abis)


# 替换 APK 的 Java 桥接与清单，对齐并重新签名
def update_java_and_sign(apk, signing, env, log):
    # 过时的官方/先前运行时无法初始化本应用新的 Application。
    # 旧的或官方运行时无法初始化本应用新的 Application
    verify_jni_exports(apk)
    # 定位 SDK
    sdk = sdk_directory(None)
    # 找出带 D8 的 build-tools
    versions = [path for path in (sdk / 'build-tools').iterdir() if (path / 'lib/d8.jar').is_file()]
    # 没有带 D8 的 build-tools
    if not versions:
        # 报错
        raise RuntimeError('An existing Android build-tools installation with D8 is required')
    # 取最新版本
    tools = max(versions, key=lambda path: tuple(int(part) for part in path.name.split('.') if part.isdigit()))
    # 定位 java
    java = Path(env['JAVA_HOME']) / 'bin/java' if env.get('JAVA_HOME') else Path(shutil.which('java') or '')
    # 找不到 java
    if not java.is_file():
        # 报错
        raise RuntimeError('An existing JDK is required for D8 and APK signing')
    # 平台 jar
    platform = platform_jar(sdk, 37)
    # 在临时目录里完成所有改动
    with tempfile.TemporaryDirectory(prefix='nsh-apk-', dir=apk.parent) as temporary:
        # 暂存目录
        stage = Path(temporary)
        # 编译桥接 Java 源码
        build(argparse.Namespace(out=stage / 'java', sdk=sdk, compile_sdk=37, javac=None))
        # dex 输出目录
        dex = stage / 'dex'; dex.mkdir()
        # 用 D8 转成 DEX
        subprocess.run([str(java), '-cp', str(tools / 'lib/d8.jar'), 'com.android.tools.r8.D8',
                        # 库路径、最低 API 与输出
                        '--lib', str(platform), '--min-api', '28', '--output', str(dex),
                        # 输入的类文件，输出重定向到日志
                        str(stage / 'java/classes.jar')], check=True, stdout=log, stderr=subprocess.STDOUT)
        # 未签名 APK 路径
        unsigned = stage / 'unsigned.apk'
        # 复制原 APK，去掉旧签名与旧 DEX，改写清单
        with zipfile.ZipFile(apk) as original, zipfile.ZipFile(unsigned, 'w') as output:
            # 遍历原 APK 条目
            for entry in original.infolist():
                # 跳过签名信息与旧 dex
                if entry.filename.startswith('META-INF/') or (entry.filename.startswith('classes') and entry.filename.endswith('.dex')):
                    # 跳过
                    continue
                # 清单文件
                if entry.filename == 'AndroidManifest.xml':
                    # 改写清单并取目标 SDK
                    manifest, target_sdk = component_manifest(original.read(entry.filename))
                    # 目标 SDK 37 起需要显式的本地网络运行时权限支持
                    if target_sdk is not None and target_sdk >= 37:
                        # 报错：目标 SDK 37 起需显式声明本地网络权限
                        raise RuntimeError('Target SDK 37+ requires explicit local-network runtime permission support')
                    # 写入新清单
                    output.writestr(entry, manifest)
                # 其他条目
                else:
                    # 原样写入
                    output.writestr(entry, original.read(entry.filename))
            # 加入新的 DEX
            for path in sorted(dex.glob('*.dex')):
                # 加入新生成的 dex
                output.write(path, path.name, compress_type=zipfile.ZIP_DEFLATED)
        # 对齐后的 APK 路径
        aligned = stage / 'aligned.apk'
        # 16 KB 页对齐
        subprocess.run([str(tools / 'zipalign'), '-f', '-P', '16', '4', str(unsigned), str(aligned)],
                       # 失败即抛错，输出重定向到日志
                       check=True, stdout=log, stderr=subprocess.STDOUT)
        # 从签名参数取出密钥库与别名
        keystore = signing[signing.index('--keystore') + 1]
        # 取密钥别名
        alias = signing[signing.index('--key-alias') + 1]
        # apksigner 命令
        signer = [str(java), '-jar', str(tools / 'lib/apksigner.jar')]
        # 签名，密码从环境变量读取
        subprocess.run(signer + ['sign', '--ks', str(keystore), '--ks-key-alias', str(alias),
                                # 密钥库口令与密钥口令来自环境变量
                                '--ks-pass', 'env:NSH_KEYSTORE_PASSWORD', '--key-pass', 'env:LOKI_SIGNING_KEY_PASSWORD',
                                # 输出与输入
                                '--out', str(apk), str(aligned)], env=env, check=True,
                       # 输出重定向到日志
                       stdout=log, stderr=subprocess.STDOUT)
        # 校验签名
        subprocess.run(signer + ['verify', '--verbose', str(apk)], check=True,
                       # 输出重定向到日志
                       stdout=log, stderr=subprocess.STDOUT)
        # CLI 的 v4 附属文件描述的是 DEX/清单替换之前的 APK。
        # CLI 生成的 v4 签名附件描述的是替换前的 APK，需删除
        apk.with_suffix(apk.suffix + '.idsig').unlink(missing_ok=True)
        # 保存桥接构建记录
        shutil.copy2(stage / 'java/java-build.json', apk.parent / 'java-build.json')
        # 返回目标 SDK
        return target_sdk
