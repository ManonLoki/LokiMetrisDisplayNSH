#!/usr/bin/env python3
"""Package five NSH applications with the official SDK and app-owned system adapters."""
# 引入标准库
import argparse
# 摘要算法
import hashlib
# (内嵌脚本) 标准库
import json
# 操作系统接口
import os
# 路径对象
from pathlib import Path
# 平台信息
import platform
# plist 读写
import plistlib
# 文件复制与删除
import shutil
# 命令行转义
import shlex
# 子进程
import subprocess
# 结构打包
import struct
# 标准错误
import sys
# 临时目录
import tempfile
# 压缩包
import zipfile

# 仓库根目录
ROOT = Path(__file__).resolve().parents[2]
# Android 应用标识
ANDROID_ID = 'com.manonloki.lokimetrisdisplay'
# Apple 应用标识
APPLE_ID = 'com.manonloki.lokimetrisdisplay.LokiMetrisDisplay'


# 在仓库根目录执行命令，失败即抛异常；日志可重定向
def run(command, *, env=None, log=None):
    # 把命令各项转为字符串后执行，失败即抛错
    subprocess.run([str(item) for item in command], cwd=ROOT, env=env, check=True,
                   # 输出重定向到日志，没有日志则不重定向
                   stdout=log, stderr=subprocess.STDOUT if log else None)


# 读取 Android 签名配置；密码通过子进程环境变量传递，不出现在命令文本里
def android_signing(directory=None):
    # 文档字符串：口令只通过子进程环境变量传递
    """Passwords travel through the subprocess environment, never command text."""
    # 目录默认为仓库内的 Android 签名目录
    directory = directory or ROOT / 'signing/android'
    # 密钥库路径
    store = directory / 'lokimetris.keystore'
    # 签名属性文件路径
    properties = directory / 'signing.properties'
    # 缺少密钥库时只提示恢复，绝不自动生成新密钥
    if not store.is_file() or not properties.is_file():
        # 缺少签名材料时报错
        raise RuntimeError('Restore signing/android/lokimetris.keystore and signing.properties; no new key was generated')
    # 解析 properties
    values = dict(line.split('=', 1) for line in properties.read_text().splitlines()
                  # 解析 key=value 行，忽略注释
                  if '=' in line and not line.lstrip().startswith('#'))
    # 别名或库口令缺失
    if not values.get('keyAlias') or not values.get('storePassword'):
        # 属性不完整
        raise RuntimeError('Android signing properties are incomplete')
    # 复制当前环境变量
    env = os.environ.copy()
    # 库口令只通过环境变量传递
    env['NSH_KEYSTORE_PASSWORD'] = values['storePassword']
    # 密钥口令缺省与库口令相同
    env['LOKI_SIGNING_KEY_PASSWORD'] = values.get('keyPassword', values['storePassword'])
    # 返回命令行参数和环境变量
    return ['--keystore', store, '--key-alias', values['keyAlias']], env


# Android 的版本与构建号跟随 nsh.json，构建号即 versionCode
def android_identity():
    # 文档字符串：安卓版本与构建号取自 nsh.json
    """Android follows the release version and build in nsh.json; the build is its versionCode."""
    # 读取 nsh.json 里的版本信息
    app = json.loads((ROOT / 'nsh.json').read_text())
    # 返回包名、版本与构建号
    return {'bundleId': ANDROID_ID, 'version': app['version'], 'build': app['build']}


# Apple 身份：优先读取已保存的 identity.json，否则用默认值
def apple_identity():
    # 苹果身份文件路径
    path = ROOT / 'signing/apple/identity.json'
    # 存在则读取，否则用默认包名与版本
    return json.loads(path.read_text()) if path.is_file() else {'bundleId': APPLE_ID, 'version': '1.0.11', 'build': 20}


# 小组件使用的 App Group；只有主应用才读取配置，否则按 group.<应用标识> 推导
def widget_group(application_id):
    # 读取苹果身份
    identity = apple_identity()
    # 只有主应用才使用身份里配置的共享组
    group = identity.get('widgetAppGroup') if application_id == identity['bundleId'] else None
    # 没有配置则按应用标识生成
    return group or 'group.' + application_id






# 小组件宿主垫片与扩展核心的构建在 widget_support.py；这里按调用时的 ROOT 和 run 转交，测试可替换它们
sys.path.insert(0, str(Path(__file__).resolve().parent))
import package_checks  # noqa: E402
import widget_support  # noqa: E402
from package_checks import embedded_apple_identifier, verify_android  # noqa: E402,F401


def widget_linker(directory, response_file, *, sdk, runner_manifest, linker=None):
    """Apply the host shim only to the NSH executable, never dependency dylibs."""
    return widget_support.widget_linker(directory, response_file, sdk=sdk, runner_manifest=runner_manifest, linker=linker)


def build_widget_support(env, *, simulator, sdk_path, developer, deployment, log=None):
    """Compile the extension core and link a small native host shim into the NSH runner."""
    return widget_support.build_widget_support(env, simulator=simulator, sdk_path=sdk_path, developer=developer,
                                               deployment=deployment, root=ROOT, run=run, log=log)


# 拼出 nsh 打包移动端的命令：目标、应用标识、版本与构建号
def android_runtime_seed(cli, stage, env):
    """Let the CLI build the project's supported 64-bit ABIs without global edits."""
    return package_checks.android_runtime_seed(cli, stage, env, ROOT)


def verify_runner(target):
    """Generated runners must include the local application's Rust module."""
    return package_checks.verify_runner(target, ROOT)


def mobile_command(cli, target, stage, *, ios_device=False, application_id=None):
    # 目标 SDK：真机或模拟器
    sdk_target = 'ios' if ios_device else 'ios-simulator'
    # 按平台取苹果或安卓身份
    identity = apple_identity() if target == 'ios' else android_identity()
    # 组装 nsh package 命令
    return [cli, 'package', '--target', 'android' if target == 'android' else sdk_target,
            # 应用标识、应用版本与构建号
            '--id', application_id or identity['bundleId'], '--app-version', identity['version'],
            # 构建号与输出目录
            '--build', str(identity['build']), '--out', stage]









# 模拟器的权限放在 __TEXT 里，而不是受限的代码签名块
def simulator_link_permissions(env, application_id):
    # 文档说明：模拟器权限放在 __TEXT 而不是受限签名块
    """Simulator permissions belong in __TEXT, not restricted code-sign blobs."""
    # 读取配置的应用标识
    identifier = apple_identity().get('applicationIdentifier')
    # 标识缺失或与应用不匹配
    if not identifier or not identifier.endswith('.' + application_id):
        # 报错：请恢复身份文件
        raise RuntimeError('Restore the matching applicationIdentifier in signing/apple/identity.json')
    # 权限内容：应用标识与 App Group
    data = plistlib.dumps({'application-identifier': identifier,
                          # 应用组权限
                          'com.apple.security.application-groups': [widget_group(application_id)]})
    # 权限文件目录
    directory = ROOT / '.nsh/signing'
    # 创建目录
    directory.mkdir(parents=True, exist_ok=True)
    # 文件名取内容摘要
    path = directory / (hashlib.sha256(data).hexdigest() + '.plist')
    # 不存在才写入
    if not path.exists():
        # 写入权限数据
        path.write_bytes(data)
    # 通过环境变量告诉链接器包装脚本
    env['LOKI_WIDGET_SIMULATOR_ENTITLEMENTS'] = str(path)




# 在最终签名之前补上发现相关的声明
def configure_apple(app, *, simulator, application_id, minimum_os=None):
    # 文档说明：最终签名前补上发现声明
    """Supply discovery declarations before final signing of the NSH runner."""
    # Info.plist 路径
    info = app / 'Info.plist'
    # 解析 plist
    value = plistlib.loads(info.read_bytes())
    # 包标识与配置不一致
    if value.get('CFBundleIdentifier') != application_id:
        # 报错
        raise RuntimeError('iOS package identifier differs from its configured identity')
    # 指定了最低系统版本
    if minimum_os:
        # 写入最低系统版本
        value['MinimumOSVersion'] = minimum_os
    # 本地网络用途说明
    value['NSLocalNetworkUsageDescription'] = '连接同一局域网内已配对的电脑，接收 Token 用量、账户额度与 Agent 活动。'
    # 允许发现的 Bonjour 服务
    value['NSBonjourServices'] = ['_loki-host._tcp']
    # 小组件 App Group
    value['LokiWidgetAppGroup'] = widget_group(application_id)
    # 后台刷新任务标识
    value['BGTaskSchedulerPermittedIdentifiers'] = ['com.manonloki.lokimetrisdisplay.widget-refresh']
    # 后台模式：获取
    value['UIBackgroundModes'] = sorted(set(value.get('UIBackgroundModes', []) + ['fetch']))
    # 允许访问本地网络的 HTTP
    value['NSAppTransportSecurity'] = dict(value.get('NSAppTransportSecurity', {}), NSAllowsLocalNetworking=True)
    # 写回二进制 plist
    info.write_bytes(plistlib.dumps(value, fmt=plistlib.FMT_BINARY))
    # 模拟器：核对内嵌标识并做临时签名
    if simulator:
        # 期望的应用标识
        expected = apple_identity().get('applicationIdentifier')
        # 可执行文件路径
        executable = app / value['CFBundleExecutable']
        # 嵌入的钥匙串标识与期望不符
        if embedded_apple_identifier(executable) != expected:
            # 报错
            raise RuntimeError('Embedded simulator Keychain identity differs from the preserved identity')
        # AMFI 会拒绝临时签名中的 application-identifier (-424)。
        # Xcode 模拟器运行器使用签名为空的 __TEXT 段。
        # AMFI 会拒绝临时签名里的 application-identifier（-424）；Xcode 模拟器运行器用 __TEXT 节加空签名
        run(['codesign', '--force', '--sign', '-', app])
        # 校验签名
        run(['codesign', '--verify', '--deep', '--strict', app])
    # 返回模拟器标记与签名校验结果
    return {'simulatorOnly': simulator, 'signatureVerified': simulator,
            # 钥匙串应用标识是否已声明
            'keychainApplicationIdentifierDeclared': simulator,
            # 本地网络权限与 Bonjour 服务
            'localNetworkDeclared': True, 'bonjourServices': value['NSBonjourServices'],
            # 最低系统版本
            'minimumOsVersion': value.get('MinimumOSVersion')}


# 打包移动端：iOS 或 Android
def package_mobile(target, output, *, ios_device=False, application_id=None, with_watch=False):
    # 查找已安装的 nsh
    cli = shutil.which('nsh')
    # 没找到
    if not cli:
        # 报错：需要已有的 NSH 安装
        raise RuntimeError('An existing NSH installation is required; no installation was attempted')
    # 目标目录
    destination = output / ('ios-device' if ios_device else target)
    # 目录已存在
    if destination.exists():
        # 报错：请换一个输出目录
        raise RuntimeError('Choose a new --out; package already exists: ' + str(destination))
    # 创建输出目录
    output.mkdir(parents=True, exist_ok=True)
    # 日志目录
    log_dir = ROOT / 'output/build' / destination.name
    # 创建目录
    log_dir.mkdir(parents=True, exist_ok=True)
    # 在临时目录里打包
    with tempfile.TemporaryDirectory(prefix='nsh-package-', dir=output) as temporary:
        # 暂存目录
        stage = Path(temporary)
        # 构造打包命令
        command = mobile_command(cli, target, stage, ios_device=ios_device, application_id=application_id)
        # 环境变量默认继承
        env = None
        # 小组件库与支持信息初始为空
        widget_library = None; widget_support = None
        # iOS：准备 SDK 路径、部署目标与 libclang
        if target == 'ios':
            # 导入部署版本函数
            from package_apple_companion import deployment_target
            # 复制环境变量
            env = os.environ.copy()
            # SDK 名称
            sdk_name = 'iphoneos' if ios_device else 'iphonesimulator'
            # 查询 SDK 路径
            sdk_path = subprocess.check_output(['xcrun', '--sdk', sdk_name, '--show-sdk-path'], text=True).strip()
            # 查询开发者目录
            developer = subprocess.check_output(['xcode-select', '-p'], text=True).strip()
            # 设置 SDK 根目录
            env['SDKROOT'] = sdk_path
            # 否则 Rust 默认把真机链接设为 iOS 10,并生成
            # 旧式的最低版本命令,而不是 LC_BUILD_VERSION。
            # Rust 默认把真机链接目标设为 iOS 10，并发出旧式的最低版本命令，而不是 LC_BUILD_VERSION
            env['IPHONEOS_DEPLOYMENT_TARGET'] = deployment_target(not ios_device, env.get('IPHONEOS_DEPLOYMENT_TARGET', '15.0'))
            # 找不到 libclang
            if not list(Path(env.get('LIBCLANG_PATH', '')).glob('libclang*.dylib')):
                # 指向 Xcode 工具链里的 libclang
                env['LIBCLANG_PATH'] = str(Path(developer) / 'Toolchains/XcodeDefault.xctoolchain/usr/lib')
            # 给 bindgen 追加 sysroot 参数
            env['BINDGEN_EXTRA_CLANG_ARGS'] = env.get('BINDGEN_EXTRA_CLANG_ARGS', '') + ' -isysroot ' + shlex.quote(sdk_path)
        # Android：读取签名，必要时定位 libclang，并限定 ABI
        if target == 'android':
            # 取得 Android 签名参数与环境
            signing, env = android_signing()
            # macOS 上缺 libclang 时
            if platform.system() == 'Darwin' and not list(Path(env.get('LIBCLANG_PATH', '')).glob('libclang*.dylib')):
                # 查询开发者目录
                developer = subprocess.check_output(['xcode-select', '-p'], text=True).strip()
                # 指向 Xcode 工具链里的 libclang
                env['LIBCLANG_PATH'] = str(Path(developer) / 'Toolchains/XcodeDefault.xctoolchain/usr/lib')
            # 把签名参数加入命令
            command.extend(signing)
            # 准备 Android 运行时种子
            android_runtime_seed(cli, stage, env)
        # 所有输出写入日志
        with (log_dir / 'package.log').open('wb') as log:
            # iOS：先构建小组件支持，模拟器还要准备权限
            if target == 'ios':
                # 构建小组件支持库
                widget_library, widget_support = build_widget_support(env, simulator=not ios_device,
                    # SDK 路径、开发者目录与部署版本
                    sdk_path=sdk_path, developer=developer, deployment=env['IPHONEOS_DEPLOYMENT_TARGET'], log=log)
                # 模拟器构建
                if not ios_device:
                    # 补充模拟器权限
                    simulator_link_permissions(env, command[command.index('--id') + 1])
            # 调用 nsh 打包
            run(command, env=env, log=log)
            # 校验运行器
            verify_runner(target)
            # 安卓包
            if target == 'android':
                # 删除运行时种子
                shutil.rmtree(stage / 'runtime-seed')
            # 按目标选择包文件后缀
            suffix = '*.apk' if target == 'android' else '*.app'
            # 必须恰好产出一个标准包
            packages = list(stage.glob(suffix))
            # 产物必须恰好一个
            if len(packages) != 1:
                # 否则报错并提示查看日志
                raise RuntimeError('Expected exactly one standard NSH package; inspect ' + str(log_dir / 'package.log'))
            # Android：替换 Java 桥接、重新签名，并校验与记录
            if target == 'android':
                # 导入签名函数
                from package_android import update_java_and_sign
                # 更新 Java 并签名
                target_sdk = update_java_and_sign(packages[0], signing, env, log)
                # 校验 APK
                metadata = verify_android(packages[0])
                # 记录 SHA-256
                metadata['sha256'] = hashlib.sha256(packages[0].read_bytes()).hexdigest()
                # 标记官方 SDK 未被改动
                metadata['officialSdkUnmodified'] = True
                # 标记应用 Java 桥已编译
                metadata['appJavaBridgeCompiled'] = True
                # 记录目标 SDK
                metadata['targetSdk'] = target_sdk
                # 标记已声明局域网权限
                metadata['lanPermissionsDeclared'] = True
            # iOS：写入声明、组装 Widgets（及可选的 Watch）的归档，并换成归档后的应用
            else:
                # 配置苹果包
                metadata = configure_apple(packages[0], simulator=not ios_device,
                                           # 应用标识取自命令行
                                           application_id=command[command.index('--id') + 1],
                                           # 最低系统版本取自环境变量
                                           minimum_os=env['IPHONEOS_DEPLOYMENT_TARGET'])
                # 导入组装函数
                from package_apple_companion import assemble
                # 组装配套归档
                archived, companion = assemble(packages[0], stage / 'NSHCompanion.xcarchive',
                                                # 模拟器、日志
                                                simulator=not ios_device, log=log,
                                                # 是否带手表、小组件库
                                                with_watch=with_watch, widget_library=widget_library)
                # 删除旧包
                shutil.rmtree(packages[0])
                # 复制归档，保留符号链接
                shutil.copytree(archived, packages[0], symlinks=True)
                # 记录苹果配套
                metadata['appleCompanions'] = companion
                # 记录小组件支持
                metadata['widgetSupport'] = widget_support
                # 带手表时
                if with_watch:
                    # 记录手表配套
                    metadata['watchCompanion'] = companion
        # 记录构建信息：仅做构建与打包检查，运行时验收另行记录
        metadata.update({'target': target, 'applicationId': command[command.index('--id') + 1],
                         # 包名与运行器包含本地 Rust
                         'package': packages[0].name, 'runnerIncludesLocalRust': True, 'runtimeVerified': False,
                         # 说明：仅为构建与打包检查，运行验收另行记录
                         'note': 'Build and package checks only; runtime acceptance is recorded separately.'})
        # 写入平台校验记录
        (stage / 'platform-verification.json').write_text(json.dumps(metadata, ensure_ascii=False, indent=2) + '\n')
        # 输出目录已出现
        if destination.exists():
            # 报错
            raise RuntimeError('Output appeared during packaging: ' + str(destination))
        # 暂存目录改名为输出目录
        stage.rename(destination)
    # 打印输出路径
    print(destination)


# 命令行入口
def main():
    # 创建参数解析器
    parser = argparse.ArgumentParser(description=__doc__)
    # 目标平台
    parser.add_argument('--target', choices=['macos', 'windows', 'linux', 'android', 'ios'], required=True)
    # 输出目录
    parser.add_argument('--out', type=Path, default=ROOT / 'output/packages')
    # 移动端应用标识覆盖
    parser.add_argument('--id', help='Explicit mobile application identifier; defaults preserve this project\'s existing identities')
    # 构建未签名的真机 .app，默认面向模拟器
    parser.add_argument('--ios-device', action='store_true', help='Build an unsigned physical-device .app; iOS otherwise targets the simulator')
    # 附带独立的 Watch 配套应用
    parser.add_argument('--with-watch', action='store_true', help='Include the independent Watch companion; iOS always includes Widgets')
    # 解析参数
    args = parser.parse_args()
    # 参数组合检查
    if args.ios_device and args.target != 'ios':
        # 真机只适用于 iOS
        parser.error('--ios-device requires --target ios')
    # 手表只适用于 iOS
    if args.with_watch and args.target != 'ios':
        # 指定了应用标识
        parser.error('--with-watch requires --target ios')
    # 仅移动端可覆盖标识
    if args.id and args.target not in ['ios', 'android']:
        # 桌面标识来自 nsh.json
        parser.error('--id is a mobile override; desktop identity comes from nsh.json')
    # 规范化输出路径
    output = args.out.expanduser().resolve()
    # 输出目录不能落在受维护的源码目录里
    if output == ROOT or any(output.is_relative_to(ROOT / part) for part in ['src', 'platform', 'tools', 'signing', 'vendor', 'docs']):
        # 输出目录不得与受维护源码混用
        parser.error('Output must be separate from maintained source directories')
    # 桌面目标交给对应的桌面打包脚本
    if args.target in ['macos', 'windows', 'linux']:
        # macOS 上打 Windows 包走交叉构建脚本
        cross = args.target == 'windows' and platform.system() == 'Darwin'
        # 选择桌面打包脚本
        script = 'package_windows_cross.py' if cross else 'package_desktop.py'
        # 构造命令
        command = [sys.executable, Path(__file__).with_name(script), '--out', output]
        # 非交叉构建时
        if not cross:
            # 追加目标平台
            command.extend(['--target', args.target])
        # 执行
        run(command)
    # 移动端
    else:
        # 打包移动端
        package_mobile(args.target, output, ios_device=args.ios_device, application_id=args.id,
                       # 是否带手表
                       with_watch=args.with_watch)


# 作为脚本运行时
if __name__ == '__main__':
    # 捕获异常
    try:
        # 调用入口
        main()
    # 把预期的失败转为简短的退出信息
    except (OSError, RuntimeError, subprocess.CalledProcessError, zipfile.BadZipFile) as error:
        # 以错误信息退出
        raise SystemExit(str(error))
