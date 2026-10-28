#!/usr/bin/env python3
"""Archive a prelinked NSH iPhone app with its independent Watch target."""
# 引入标准库
import argparse
# 摘要算法
import hashlib
# JSON 读写
import json
# 路径对象
from pathlib import Path
# plist 读写
import plistlib
# 文件复制与删除
import shutil
# 二进制结构解析
import struct
# 子进程
import subprocess
import sys
# 临时目录
import tempfile
# XML 转义
from xml.sax.saxutils import escape

# 仓库根目录
sys.path.insert(0, str(Path(__file__).resolve().parent))
from apple_binary import (archived_application, machine_code_hash, macho_platforms, nsh_bundle_hash,  # noqa: E402,F401
                          prepare_app, verify_versions, watch_source_hashes)
from apple_project import (deployment_target, identifier, native_target, project_value,  # noqa: E402,F401
                           watch_target, widget_target)

ROOT = Path(__file__).resolve().parents[2]
# 手表应用的 Xcode 工程
WATCH_PROJECT = ROOT / 'platform/watchos/LokiMetrisWatch.xcodeproj'
# 小组件扩展的 Xcode 工程
WIDGET_PROJECT = ROOT / 'platform/ios/widgets/LokiMetrisWidgets.xcodeproj'


# 执行命令，失败即抛异常
def run(command, *, log=None):
    # 把命令各项转为字符串后执行
    subprocess.run([str(value) for value in command], check=True, cwd=ROOT,
                   # 日志重定向到文件，没有日志则不重定向
                   stdout=log, stderr=subprocess.STDOUT if log else None)














# 生成宿主工程：iPhone 目标保持预链接，由 Xcode 构建并嵌入手表与小组件
def generate_project(directory, app, watch_project=WATCH_PROJECT, *, simulator, team,
                     # 默认的小组件工程、手表与小组件开关
                     widget_project=WIDGET_PROJECT, with_watch=True, with_widgets=True):
    # 文档说明：iPhone 目标保持预链接，子目标由 Xcode 构建并嵌入
    """The iPhone target stays prelinked; Xcode builds and embeds native children."""
    # 创建工程目录
    directory.mkdir(parents=True, exist_ok=True)
    # 工程包路径
    project = directory / 'NSHCompanion.xcodeproj'
    # 创建工程包目录
    project.mkdir()
    # 复制应用的 Info.plist，去掉设备族并抬高最低系统版本
    info = plistlib.loads((app / 'Info.plist').read_bytes())
    # 去掉设备族声明
    info.pop('UIDeviceFamily', None)
    # 计算最低系统版本
    minimum = deployment_target(simulator, info.get('MinimumOSVersion', '15.0'))
    # 写入最低系统版本
    info['MinimumOSVersion'] = minimum
    # 带小组件时补上默认的 App Group
    if with_widgets:
        # 缺省的小组件共享组
        info.setdefault('LokiWidgetAppGroup', 'group.' + info['CFBundleIdentifier'])
    # 写出 Info.plist
    (directory / 'Info.plist').write_bytes(plistlib.dumps(info))
    # 工程对象表
    objects = {}

    # 添加一个工程对象，返回其键
    def add(label, kind, **fields):
        # 由标签生成对象标识
        key = identifier(label)
        # 登记对象
        objects[key] = {'isa': kind, **fields}
        # 返回标识
        return key

    # 宿主应用的产物引用
    product = add('hostProduct', 'PBXFileReference', explicitFileType='wrapper.application',
                  # 产物引用：不加入索引、路径与来源组
                  includeInIndex=0, path=app.name, sourceTree='BUILT_PRODUCTS_DIR')
    # 产物分组
    products = add('hostProducts', 'PBXGroup', children=[product], name='Products', sourceTree='<group>')
    # 构建阶段脚本：把预链接的 NSH 应用原样复制到产物目录
    stage = add('stageNSH', 'PBXShellScriptBuildPhase', buildActionMask=2147483647, files=[],
                # 输入路径：预链接的可执行文件
                inputPaths=[str(app / info['CFBundleExecutable'])],
                # 输出路径与仅部署时运行
                outputPaths=['$(TARGET_BUILD_DIR)/$(EXECUTABLE_PATH)'], runOnlyForDeploymentPostprocessing=0,
                # 使用 /bin/sh，阶段名为“暂存预链接应用”
                shellPath='/bin/sh', name='Stage prelinked NSH application',
                # 脚本：用 ditto 复制预链接应用
                shellScript='set -eu\n/usr/bin/ditto --norsrc --noqtn "$NSH_PRELINKED_APP" "$TARGET_BUILD_DIR/$WRAPPER_NAME"\n')
    # 子项、依赖、嵌入与引用列表
    children = [products]; dependencies = []; embeds = []; references = []

    # 嵌入一个外部目标（手表应用或小组件扩展）的全套对象
    def embed_target(label, external_project, target_name, target, *, extension):
        # 拆出目标标识、产物标识与产物名
        target_id, product_id, product_name = target
        # 外部工程引用
        external = add(label + 'Project', 'PBXFileReference', lastKnownFileType='wrapper.pb-project',
                       # 外部工程路径，使用绝对路径
                       path=str(external_project), sourceTree='<absolute>')
        # 目标代理与产物代理
        target_proxy = add(label + 'TargetProxy', 'PBXContainerItemProxy', containerPortal=external,
                           # 目标代理：目标标识与名称
                           proxyType=1, remoteGlobalIDString=target_id, remoteInfo=target_name)
        # 产物代理对象
        product_proxy = add(label + 'ProductProxy', 'PBXContainerItemProxy', containerPortal=external,
                            # 产物代理：产物标识与名称
                            proxyType=2, remoteGlobalIDString=product_id, remoteInfo=product_name)
        # 产物引用
        reference = add(label + 'Reference', 'PBXReferenceProxy',
                        # 按是否扩展选择文件类型
                        fileType='wrapper.app-extension' if extension else 'wrapper.application',
                        # 路径、产物代理与来源组
                        path=product_name, remoteRef=product_proxy, sourceTree='BUILT_PRODUCTS_DIR')
        # 外部产物分组
        external_products = add(label + 'Products', 'PBXGroup', children=[reference],
                                # 分组名与来源
                                name=target_name + ' Products', sourceTree='<group>')
        # 宿主依赖该目标
        dependencies.append(add(label + 'Dependency', 'PBXTargetDependency', name=target_name,
                                # 目标代理
                                targetProxy=target_proxy))
        # 扩展嵌入时需要代码签名
        attributes = ['RemoveHeadersOnCopy'] + (['CodeSignOnCopy'] if extension else [])
        # 嵌入用的构建文件
        embedded = add(label + 'BuildFile', 'PBXBuildFile', fileRef=reference, settings={'ATTRIBUTES': attributes})
        # 拷贝文件阶段：手表放进 Watch 目录，扩展放进 PlugIns
        embeds.append(add(label + 'Embed', 'PBXCopyFilesBuildPhase', buildActionMask=2147483647,
                          # 目标路径：扩展为空，手表放到 Watch 目录
                          dstPath='' if extension else '$(CONTENTS_FOLDER_PATH)/Watch',
                          # 目标子目录规格与文件
                          dstSubfolderSpec=13 if extension else 16, files=[embedded],
                          # 阶段名：嵌入扩展或手表内容
                          name='Embed App Extensions' if extension else 'Embed Watch Content',
                          # 仅部署后处理时运行：否
                          runOnlyForDeploymentPostprocessing=0))
        # 把外部工程加入子项
        children.append(external)
        # 登记工程引用
        references.append({'ProductGroup': external_products, 'ProjectRef': external})

    # 带手表时
    if with_watch:
        # 嵌入手表目标
        embed_target('watch', watch_project, 'LokiMetrisWatch', watch_target(watch_project), extension=False)
    # 带小组件时
    if with_widgets:
        # 嵌入小组件目标
        embed_target('widget', widget_project, 'LokiMetrisWidgets', widget_target(widget_project), extension=True)
    # 主分组
    group = add('mainGroup', 'PBXGroup', children=children, sourceTree='<group>')
    # 宿主目标的构建设置：手动签名、不剥离符号、保持预链接的应用
    settings = {
        # 包标识与产品名
        'PRODUCT_BUNDLE_IDENTIFIER': info['CFBundleIdentifier'], 'PRODUCT_NAME': app.stem,
        # 可执行文件名与 Info.plist 路径
        'EXECUTABLE_NAME': info['CFBundleExecutable'], 'INFOPLIST_FILE': str(directory / 'Info.plist'),
        # 不生成 Info.plist，SDK 为 iphoneos
        'GENERATE_INFOPLIST_FILE': 'NO', 'SDKROOT': 'iphoneos',
        # 支持的平台与部署版本
        'SUPPORTED_PLATFORMS': 'iphoneos iphonesimulator', 'IPHONEOS_DEPLOYMENT_TARGET': minimum,
        # 设备族、手动签名与签名身份
        'TARGETED_DEVICE_FAMILY': '1,2', 'CODE_SIGN_STYLE': 'Manual', 'CODE_SIGN_IDENTITY': '-',
        # 开发团队、不安装与安装路径
        'DEVELOPMENT_TEAM': team, 'SKIP_INSTALL': 'NO', 'INSTALL_PATH': '$(LOCAL_APPS_DIR)',
        # 不剥离符号、不启用用户脚本沙箱
        'COPY_PHASE_STRIP': 'NO', 'STRIP_INSTALLED_PRODUCT': 'NO', 'ENABLE_USER_SCRIPT_SANDBOXING': 'NO',
        # 预链接应用等构建设置
        'NSH_PRELINKED_APP': str(app), 'ENABLE_DEBUG_DYLIB': 'NO', 'ALWAYS_SEARCH_USER_PATHS': 'NO',
    }
    # 配置列表
    configs = []; project_configs = []
    # 两种配置共用同一套设置
    for name in ['Debug', 'Release']:
        # 宿主目标的构建配置
        configs.append(add('host' + name, 'XCBuildConfiguration', buildSettings=settings, name=name))
        # 工程级构建配置
        project_configs.append(add('project' + name, 'XCBuildConfiguration', buildSettings={}, name=name))
    # 宿主配置列表
    host_configs = add('hostConfigurations', 'XCConfigurationList', buildConfigurations=configs,
                       # 默认配置不可见，名称为 Release
                       defaultConfigurationIsVisible=0, defaultConfigurationName='Release')
    # 工程配置列表
    pconfigs = add('projectConfigurations', 'XCConfigurationList', buildConfigurations=project_configs,
                   # 默认配置不可见，名称为 Release
                   defaultConfigurationIsVisible=0, defaultConfigurationName='Release')
    # 宿主应用目标
    host = add('host', 'PBXNativeTarget', buildConfigurationList=host_configs, buildPhases=[stage] + embeds,
               # 构建规则、依赖与目标名
               buildRules=[], dependencies=dependencies, name='NSHCompanion', productName=app.stem,
               # 产物引用与类型：应用
               productReference=product, productType='com.apple.product-type.application')
    # 工程根对象
    root = add('project', 'PBXProject', attributes={'LastUpgradeCheck': '2710'}, buildConfigurationList=pconfigs,
               # 兼容版本与开发区域
               compatibilityVersion='Xcode 14.0', developmentRegion='en', hasScannedForEncodings=0,
               # 已知区域、主分组与产物分组
               knownRegions=['en', 'Base'], mainGroup=group, productRefGroup=products,
               # 工程引用、目录与目标
               projectReferences=references, projectDirPath='', projectRoot='', targets=[host])
    # 写出 pbxproj
    (project / 'project.pbxproj').write_text('// !$*UTF8*$!\n' + project_value({
        # 归档版本、类、对象版本与对象表
        'archiveVersion': 1, 'classes': {}, 'objectVersion': 56, 'objects': objects, 'rootObject': root}) + '\n')
    # 创建共享 scheme 目录
    schemes = project / 'xcshareddata/xcschemes'; schemes.mkdir(parents=True)
    # 写出共享方案
    (schemes / 'NSHCompanion.xcscheme').write_text(f'''<?xml version="1.0" encoding="UTF-8"?>
<Scheme LastUpgradeVersion="2710" version="1.3"><BuildAction parallelizeBuildables="YES" buildImplicitDependencies="YES">
<BuildActionEntries><BuildActionEntry buildForTesting="YES" buildForRunning="YES" buildForProfiling="YES" buildForArchiving="YES" buildForAnalyzing="YES">
<BuildableReference BuildableIdentifier="primary" BlueprintIdentifier="{host}" BuildableName="{escape(app.name, {chr(34): '&quot;'})}" BlueprintName="NSHCompanion" ReferencedContainer="container:NSHCompanion.xcodeproj"/>
</BuildActionEntry></BuildActionEntries></BuildAction><ArchiveAction buildConfiguration="Release" revealArchiveInOrganizer="NO"/></Scheme>''')
    # 返回工程路径
    return project















# 组装：用 Xcode 归档预链接的 iPhone 应用及其手表与小组件，并核对产物
def assemble(app, archive, *, simulator=True, log=None, watch_project=WATCH_PROJECT,
             # 默认小组件工程、库与开关
             widget_project=WIDGET_PROJECT, widget_library=None, with_watch=True, with_widgets=True):
    # 解析路径
    app = app.resolve(strict=True); archive = archive.resolve()
    # 归档已存在
    if archive.exists():
        # 拒绝覆盖
        raise RuntimeError('Refusing to replace an existing companion archive')
    # 读取应用 Info.plist
    info = plistlib.loads((app / 'Info.plist').read_bytes())
    # 平台必须与请求相符（模拟器 7，真机 2）
    phone_platforms = macho_platforms(app / info['CFBundleExecutable'])
    # 手机可执行文件平台不符
    if phone_platforms != {7 if simulator else 2}:
        # 报错
        raise RuntimeError('The prelinked NSH executable does not match the requested iOS platform')
    # 记录归档前的包摘要与源码摘要
    bundle_hash = nsh_bundle_hash(app)
    # 记录手表源码摘要
    source_hashes = watch_source_hashes(watch_project) if with_watch else None
    # 记录小组件源码摘要
    widget_hashes = watch_source_hashes(widget_project) if with_widgets else None
    # 带小组件需要匹配的 Rust 静态库
    if with_widgets and (widget_library is None or not Path(widget_library).is_file()):
        # 缺少 Rust 静态库
        raise RuntimeError('Widget assembly requires the matching Rust static library')
    # 读取身份文件
    identity = json.loads((ROOT / 'signing/apple/identity.json').read_text())
    # 构建根目录
    build_root = ROOT / 'output/build/ios-companion'
    # 创建目录
    build_root.mkdir(parents=True, exist_ok=True)
    # 创建组装用临时目录
    work = Path(tempfile.mkdtemp(prefix='assembly-', dir=build_root))
    # 预链接应用路径
    prepared = work / 'Prelinked' / app.name
    # 准备预链接应用
    prepare_app(app, prepared, info['CFBundleIdentifier'])
    # 生成工程
    project = generate_project(work, prepared, watch_project.resolve(), simulator=simulator,
                               # 团队与小组件工程
                               team=identity.get('teamId', ''), widget_project=widget_project.resolve(),
                               # 是否带手表与小组件
                               with_watch=with_watch, with_widgets=with_widgets)
    # 目标设备：模拟器或真机
    destination = 'generic/platform=iOS Simulator' if simulator else 'generic/platform=iOS'
    # 构建归档命令
    command = ['xcodebuild', '-project', project, '-scheme', 'NSHCompanion', '-configuration', 'Release',
               # 目标设备、归档路径与派生数据路径
               '-destination', destination, '-archivePath', archive, '-derivedDataPath', work / 'DerivedData',
               # 配套应用的包标识
               'LOKI_COMPANION_BUNDLE_IDENTIFIER=' + info['CFBundleIdentifier'],
               # 构建号
               'CURRENT_PROJECT_VERSION=' + str(info['CFBundleVersion']),
               # 营销版本号
               'MARKETING_VERSION=' + info['CFBundleShortVersionString'],
               # 开发团队与动作：归档
               'DEVELOPMENT_TEAM=' + identity.get('teamId', ''), 'archive']
    # 带小组件时传入核心库与 App Group
    if with_widgets:
        # 传入 Rust 核心库路径
        command[-1:-1] = ['LOKI_WIDGET_CORE_LIBRARY=' + str(Path(widget_library).resolve()),
                          # 小组件共享组
                          'LOKI_WIDGET_APP_GROUP=' + info['LokiWidgetAppGroup']]
    # 真机归档不签名，之后另行开发签名
    if not simulator:
        # 禁用 Xcode 自己的签名
        command.insert(-1, 'CODE_SIGNING_ALLOWED=NO')
    # 模拟器只构建 arm64
    else:
        # 只构建 arm64
        command[-1:-1] = ['ARCHS=arm64', 'ONLY_ACTIVE_ARCH=YES']
    # 执行 xcodebuild
    run(command, log=log)
    # 编译期间源码不能变化
    if with_watch and watch_source_hashes(watch_project) != source_hashes:
        # 手表源码在编译期间被改动
        raise RuntimeError('Watch sources changed during compilation; rebuild after they are frozen')
    # 小组件源码在编译期间被改动
    if with_widgets and watch_source_hashes(widget_project) != widget_hashes:
        # 报错
        raise RuntimeError('Widget sources changed during compilation; rebuild after they are frozen')
    # 取得归档里的应用
    result = archived_application(archive)
    # 读取其 Info.plist
    result_info = plistlib.loads((result / 'Info.plist').read_bytes())
    # 标识被改动
    if result_info['CFBundleIdentifier'] != info['CFBundleIdentifier']:
        # 报错
        raise RuntimeError('Xcode assembly changed the NSH application identifier')
    # 归档不得改变预链接的机器码与 NSH 包
    before = machine_code_hash(app / info['CFBundleExecutable'])
    # 计算归档后的机器码摘要
    after = machine_code_hash(result / result_info['CFBundleExecutable'])
    # 与之前不一致
    if before != after:
        # 报错
        raise RuntimeError('The archive did not preserve the prelinked NSH machine code')
    # bundle 摘要不一致
    if nsh_bundle_hash(result) != bundle_hash:
        # 报错
        raise RuntimeError('The archive did not preserve the NSH application bundle')
    # 找出嵌入的手表应用
    watches = []
    # 遍历归档里所有 .app
    for candidate in result.rglob('*.app'):
        # 读取其 Info.plist
        metadata = plistlib.loads((candidate / 'Info.plist').read_bytes())
        # 属于本手机应用的手表
        if metadata.get('WKCompanionAppBundleIdentifier') == info['CFBundleIdentifier']:
            # 记录
            watches.append((candidate, metadata))
    # 数量必须与请求一致
    if len(watches) != (1 if with_watch else 0):
        # 手表数量与请求不符
        raise RuntimeError('Embedded Watch application count differs from the requested build')
    # 校验报告
    report = {'archive': archive.name, 'applicationPath': str(result.relative_to(archive)),
              # 配套包标识与机器码保持不变
              'companionBundleId': info['CFBundleIdentifier'], 'machineCodePreserved': True,
              # 机器码摘要与应用包摘要
              'machineCodeSha256': after, 'nshBundlePreserved': True, 'nshBundleSha256': bundle_hash,
              # 手机构建平台
              'phoneBuildPlatforms': sorted(phone_platforms),
              # 仅模拟器与签名校验
              'simulatorOnly': simulator, 'signatureVerified': simulator,
              # 需要设备签名；运行时未验证
              'deviceSigningRequired': not simulator, 'runtimeVerified': False}
    # 校验手表的平台、版本并记录摘要
    if watches:
        # 取出手表应用与信息
        watch, watch_info = watches[0]
        # 手表可执行文件的平台
        watch_platforms = macho_platforms(watch / watch_info['CFBundleExecutable'])
        # 手表模拟器 9，真机 4
        if watch_platforms != {9 if simulator else 4}:
            # 手表平台不符
            raise RuntimeError('The embedded Watch executable does not match the requested watchOS platform')
        # 校验版本一致
        verify_versions(info, watch_info)
        # 把手表信息写入报告
        report.update({'watchPath': str(watch.relative_to(result)), 'watchBundleId': watch_info['CFBundleIdentifier'],
                       # 手表源码摘要
                       'watchSourceHashes': source_hashes,
                       # 手表可执行文件摘要
                       'watchExecutableSha256': hashlib.sha256((watch / watch_info['CFBundleExecutable']).read_bytes()).hexdigest(),
                       # 手表构建平台
                       'watchBuildPlatforms': sorted(watch_platforms)})
    # 找出嵌入的小组件扩展
    extensions = list(result.rglob('*.appex'))
    # 小组件扩展数量不符
    if len(extensions) != (1 if with_widgets else 0):
        # 报错
        raise RuntimeError('Embedded WidgetKit extension count differs from the requested build')
    # 校验小组件的位置、标识、扩展点、App Group、平台与版本
    if extensions:
        # 取出小组件扩展
        widget = extensions[0]
        # 读取其 Info.plist
        widget_info = plistlib.loads((widget / 'Info.plist').read_bytes())
        # 期望的共享组
        expected_group = info['LokiWidgetAppGroup']
        # 位置或标识不符
        if widget.parent != result / 'PlugIns' or widget_info.get('CFBundleIdentifier') != info['CFBundleIdentifier'] + '.widgets':
            # 报错
            raise RuntimeError('The WidgetKit extension has an unexpected identifier or embedding path')
        # 扩展点不是 WidgetKit
        if widget_info.get('NSExtension', {}).get('NSExtensionPointIdentifier') != 'com.apple.widgetkit-extension':
            # 报错
            raise RuntimeError('The embedded extension is not a WidgetKit extension')
        # 校验小组件共享组一致
        if result_info.get('LokiWidgetAppGroup') != expected_group or widget_info.get('LokiWidgetAppGroup') != expected_group:
            # 手机与小组件的共享组不一致
            raise RuntimeError('Phone and Widget App Group declarations differ')
        # 小组件可执行文件的平台
        widget_platforms = macho_platforms(widget / widget_info['CFBundleExecutable'])
        # 平台不符
        if widget_platforms != {7 if simulator else 2}:
            # 报错
            raise RuntimeError('The embedded Widget executable does not match the requested iOS platform')
        # 校验版本一致
        verify_versions(info, widget_info)
        # 把小组件信息写入报告
        report.update({'widgetPath': str(widget.relative_to(result)), 'widgetBundleId': widget_info['CFBundleIdentifier'],
                       # 小组件共享组与源码摘要
                       'widgetAppGroup': expected_group, 'widgetSourceHashes': widget_hashes,
                       # 小组件可执行文件摘要
                       'widgetExecutableSha256': hashlib.sha256((widget / widget_info['CFBundleExecutable']).read_bytes()).hexdigest(),
                       # 小组件核心库摘要
                       'widgetCoreSha256': hashlib.sha256(Path(widget_library).read_bytes()).hexdigest(),
                       # 小组件构建平台
                       'widgetBuildPlatforms': sorted(widget_platforms)})
    # 模拟器产物需通过签名校验
    if simulator:
        # 校验签名
        run(['codesign', '--verify', '--deep', '--strict', result], log=log)
    # 写出归档报告
    (archive.parent / 'companion-archive.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
    # 返回应用与报告
    return result, report




# 命令行入口
def main():
    # 创建参数解析器
    parser = argparse.ArgumentParser(description=__doc__)
    # 应用路径
    parser.add_argument('--app', required=True, type=Path)
    # 归档路径
    parser.add_argument('--archive', required=True, type=Path)
    # 真机选项
    parser.add_argument('--device', action='store_true', help='Archive unsigned device binaries; no device installation or distribution')
    # 不带手表选项
    parser.add_argument('--without-watch', action='store_true', help='Embed Widgets without a Watch companion')
    # 小组件核心库路径
    parser.add_argument('--widget-core-library', required=True, type=Path)
    # 解析参数
    args = parser.parse_args()
    # 组装应用与归档
    app, _ = assemble(args.app, args.archive, simulator=not args.device,
                      # 是否带手表与小组件库
                      with_watch=not args.without_watch, widget_library=args.widget_core_library)
    # 打印应用路径
    print(app)


# 作为脚本运行时
if __name__ == '__main__':
    # 调用入口
    main()
