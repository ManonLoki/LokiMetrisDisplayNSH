"""Widget host shim: linker wrapper and the extension core build."""
import hashlib
import json
import os
from pathlib import Path
import platform
import shlex
import sys
import os
from pathlib import Path
import re
import shlex
import sys
import tomllib


# 生成链接器包装脚本：只把宿主垫片加到 NSH 可执行文件上，不加到依赖的动态库
def widget_linker(directory, response_file, *, sdk, runner_manifest, linker=None):
    # 文档说明：宿主垫片只作用于 NSH 可执行文件
    """Apply the host shim only to the NSH executable, never dependency dylibs."""
    # 脚本需要的配置：响应文件、其摘要、运行器清单与真实链接器
    configuration = {'response': str(response_file),
                     # 响应文件摘要
                     'responseSha256': hashlib.sha256(response_file.read_bytes()).hexdigest(),
                     # 运行器清单路径
                     'runnerManifest': str(runner_manifest),
                     # 链接器，缺省为 xcrun 的 clang
                     'linker': linker or ['xcrun', '--sdk', sdk, 'clang']}
    # 脚本内容（内嵌的 Python 源码）；其中判断是否为最终运行器、追加响应文件与模拟器权限，并记录审计日志后转交真实链接器
    script = '#!' + sys.executable + '\n' + '''import json
import os
from pathlib import Path
import re
import shlex
import sys
import tomllib

# (内嵌脚本) 配置：响应文件、运行器清单与真实链接器
CONFIGURATION = ''' + repr(configuration) + '''

# (内嵌脚本) 展开链接参数里的 @响应文件，嵌套不得超过 4 层
def expanded(arguments, depth=0):
    # (内嵌脚本)
    if depth > 4:
        raise RuntimeError('Linker response files are nested too deeply')
    # (内嵌脚本)
    result = []
    # (内嵌脚本)
    for argument in arguments:
        # (内嵌脚本) 响应文件：读出并递归展开
        if argument.startswith('@') and Path(argument[1:]).is_file():
            # (内嵌脚本)
            result.extend(expanded(shlex.split(Path(argument[1:]).read_text()), depth + 1))
        else:
            # (内嵌脚本) 普通参数原样保留
            result.append(argument)
    # (内嵌脚本)
    return result

# (内嵌脚本) 判断本次链接的是不是最终的 NSH 运行器，而不是依赖的动态库
def final_runner(arguments):
    # (内嵌脚本)
    values = expanded(arguments)
    # (内嵌脚本) 带动态库标志的不是运行器
    if any(flag in values for flag in ['-dynamiclib', '-shared', '-bundle']):
        return False
    # (内嵌脚本) 找出 -o 指定的输出
    outputs = [values[index + 1] for index, value in enumerate(values[:-1]) if value == '-o']
    # (内嵌脚本)
    if not outputs:
        return False
    # (内嵌脚本) 读取运行器清单，必须只声明一个可执行文件
    manifest = tomllib.loads(Path(CONFIGURATION['runnerManifest']).read_text())
    # (内嵌脚本)
    names = [entry['name'] for entry in manifest.get('bin', [])]
    # (内嵌脚本)
    if len(names) != 1:
        raise RuntimeError('Expected the generated NSH iPhone runner to declare one executable')
    # (内嵌脚本)
    name = names[0]
    # (内嵌脚本)
    output = Path(outputs[-1]).name
    # (内嵌脚本) 输出名可能是下划线形式或带哈希后缀
    normalized = name.replace('-', '_')
    # (内嵌脚本)
    return output in {name, normalized} or re.fullmatch(re.escape(normalized) + '-[0-9a-f]{8,64}', output) is not None

# (内嵌脚本)
if __name__ == '__main__':
    # (内嵌脚本)
    arguments = sys.argv[1:]
    # (内嵌脚本)
    applies = final_runner(arguments)
    # (内嵌脚本) 最终运行器：追加宿主目标文件的响应文件，以及模拟器的权限段
    if applies:
        # (内嵌脚本)
        arguments.append('@' + CONFIGURATION['response'])
        # (内嵌脚本)
        entitlements = os.environ.get('LOKI_WIDGET_SIMULATOR_ENTITLEMENTS')
        # (内嵌脚本)
        if entitlements:
            # (内嵌脚本)
            arguments.append('-Wl,-sectcreate,__TEXT,__entitlements,' + entitlements)
    # (内嵌脚本) 审计日志：每次链接都记一笔是否加了垫片
    audit = Path(__file__).with_name('link-events.jsonl')
    # (内嵌脚本)
    values = expanded(sys.argv[1:])
    # (内嵌脚本)
    outputs = [values[index + 1] for index, value in enumerate(values[:-1]) if value == '-o']
    # (内嵌脚本)
    with audit.open('a') as output:
        # (内嵌脚本)
        output.write(json.dumps({'output': outputs[-1] if outputs else None, 'hostShimAdded': applies}) + '\\n')
    # (内嵌脚本) 转交真实链接器
    command = CONFIGURATION['linker'] + arguments
    # (内嵌脚本)
    os.execvp(command[0], command)
'''
    # 文件名带内容摘要，内容变化就换新文件
    path = directory / ('WidgetHostLinker-' + hashlib.sha256(script.encode()).hexdigest() + '.py')
    # 脚本内容有变化才重写
    if not path.exists() or path.read_text() != script:
        # 写入脚本
        path.write_text(script)
    # 设为可执行
    path.chmod(0o755)
    # 返回脚本路径
    return path


# 编译小组件核心，并把一个很小的原生宿主垫片链接进 NSH 运行器
def build_widget_support(env, *, simulator, sdk_path, developer, deployment, root, run, log=None):
    # 文档说明：编译扩展核心并链接宿主垫片
    """Compile the extension core and link a small native host shim into the NSH runner."""
    # 目标三元组与 SDK 名称
    target = 'aarch64-apple-ios-sim' if simulator else 'aarch64-apple-ios'
    # 按是否模拟器选择 SDK
    sdk = 'iphonesimulator' if simulator else 'iphoneos'
    # 共享 Swift 源码目录
    shared = root / 'platform/ios/shared'
    # 需要编译的 Swift 源文件
    sources = [shared / (name + '.swift') for name in
               # 共享源文件列表
               ['LokiCore', 'PlatformRules', 'SharedSupport', 'WidgetContainer', 'WidgetRefresh', 'WidgetRefreshCommitGate']]
    # 加入宿主桥接源文件
    sources.append(root / 'platform/ios/bridge/WidgetHostBridge.swift')
    # C 引导文件
    bootstrap = root / 'platform/ios/bridge/WidgetHostBootstrap.c'
    # 对所有输入求摘要，输出目录以摘要命名，实现可复用的缓存
    digest = hashlib.sha256()
    # 遍历全部源文件与引导文件
    for source in sources + [bootstrap]:
        # 把相对路径与内容计入摘要
        digest.update(str(source.relative_to(root)).encode()); digest.update(source.read_bytes())
    # 再计入 SDK 路径与部署版本
    digest.update((str(sdk_path) + deployment).encode())
    # 输出目录以摘要命名，保证缓存可复用
    output = root / '.nsh/widget-support' / target / digest.hexdigest()
    # 创建输出目录
    output.mkdir(parents=True, exist_ok=True)
    # Rust 核心库的输出目录
    rust_output = root / 'target/widget-core'
    # 编译小组件核心静态库
    run(['cargo', 'build', '--manifest-path', root / 'Cargo.toml', '-p', 'loki_widget_core',
         # 发布构建、锁定依赖、目标与目标目录
         '--release', '--locked', '--target', target, '--target-dir', rust_output], env=env, log=log)
    # 静态库路径
    library = rust_output / target / 'release/libloki_widget_core.a'
    # 库不存在
    if not library.is_file():
        # 核心库构建失败
        raise RuntimeError('Widget core build did not produce its static library')
    # Swift 目标文件路径
    swift_object = output / 'WidgetHostBridge.o'
    # 引导目标文件路径
    bootstrap_object = output / 'WidgetHostBootstrap.o'
    # 目标三元组：设备或模拟器
    triple = 'arm64-apple-ios' + deployment + ('-simulator' if simulator else '')
    # 缺失才重新编译 Swift 目标文件
    if not swift_object.is_file():
        # 编译 Swift 桥接源码
        run(['xcrun', '--sdk', sdk, 'swiftc', '-swift-version', '5', '-parse-as-library', '-whole-module-optimization',
             # 仅输出目标文件、优化、模块名与目标三元组
             '-emit-object', '-O', '-module-name', 'LokiWidgetHost', '-target', triple,
             # SDK 路径、源文件与输出文件
             '-sdk', sdk_path, *sources, '-o', swift_object], env=env, log=log)
    # 缺失才重新编译 C 引导
    if not bootstrap_object.is_file():
        # 编译 C 引导文件
        run(['xcrun', '--sdk', sdk, 'clang', '-target', triple, '-isysroot', sdk_path,
             # 仅编译、引导文件与输出文件
             '-c', bootstrap, '-o', bootstrap_object], env=env, log=log)
    # Swift 运行时库目录
    toolchain = Path(developer) / 'Toolchains/XcodeDefault.xctoolchain/usr/lib/swift' / sdk
    # 链接参数：目标文件、库搜索路径与运行时路径
    arguments = [str(swift_object), str(bootstrap_object), '-L', str(toolchain),
                 # 库路径与运行时库搜索路径
                 '-L', str(Path(sdk_path) / 'usr/lib/swift'), '-Wl,-rpath,/usr/lib/swift']
    # 需要的系统框架
    for framework in ['Foundation', 'UIKit', 'SwiftUI', 'WidgetKit', 'BackgroundTasks']:
        # 追加需要链接的系统框架
        arguments.extend(['-framework', framework])
    # Cargo 会把一些依赖链接为动态库。目标链接器包装器只在
    # 链接生成的 NSH 应用二进制时才加入 Swift 宿主对象。
    # Cargo 会把部分依赖链接为动态库；这里用链接器包装脚本，只在链接 NSH 应用二进制时加入 Swift 宿主目标文件
    response = ''.join(shlex.quote(value) + '\n' for value in arguments)
    # 链接响应文件按内容摘要命名
    response_file = output / ('WidgetHostLink-' + hashlib.sha256(response.encode()).hexdigest() + '.rsp')
    # 已有同名文件但内容不同
    if response_file.exists() and response_file.read_text() != response:
        # 报错
        raise RuntimeError('Widget link response path contains unexpected data')
    # 写入响应文件
    response_file.write_text(response)
    # 生成链接器包装脚本
    wrapper = widget_linker(output, response_file, sdk=sdk,
                            # 运行器清单路径
                            runner_manifest=root / '.nsh/rust/ios-app/Cargo.toml')
    # 通过环境变量让 Cargo 使用包装后的链接器
    key = 'CARGO_TARGET_' + target.upper().replace('-', '_') + '_LINKER'
    # 把真实链接器替换为包装脚本
    env[key] = str(wrapper)
    # 返回静态库与构建记录
    return library, {'target': target, 'hostSourceSha256': digest.hexdigest(),
                     # 宿主目标文件摘要
                     'hostObjectSha256': hashlib.sha256(swift_object.read_bytes()).hexdigest(),
                     # 引导目标文件摘要
                     'bootstrapObjectSha256': hashlib.sha256(bootstrap_object.read_bytes()).hexdigest(),
                     # 宿主链接器与响应文件
                     'hostLinker': str(wrapper), 'hostLinkResponse': str(response_file),
                     # 小组件核心库摘要
                     'widgetCoreSha256': hashlib.sha256(library.read_bytes()).hexdigest()}
