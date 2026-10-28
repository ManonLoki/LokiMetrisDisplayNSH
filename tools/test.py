#!/usr/bin/env python3
"""Run current project tests with the existing Rust, NSH and platform toolchains."""
# 引入参数解析、环境变量、路径、进程等标准库
import argparse
import os
from pathlib import Path
import shlex
import subprocess
import sys

# 仓库根目录
ROOT = Path(__file__).resolve().parents[1]


# 返回指定测试套件要依次执行的命令
def commands(suite, output):
    # 用当前解释器运行 Python 脚本
    python = sys.executable
    # 默认套件：格式、静态检查、Rust 测试、打包测试、凭据测试与 NSH 界面回归
    if suite == 'unit':
        result = [
            # 格式检查
            ['cargo', 'fmt', '--all', '--check'],
            # 静态检查，警告视为错误
            ['cargo', 'clippy', '--workspace', '--all-targets', '--all-features', '--locked', '--', '-D', 'warnings'],
            # Rust 测试
            ['cargo', 'test', '--workspace', '--all-targets', '--locked'],
            # 打包相关的 Python 测试
            [python, '-m', 'unittest', 'discover', '-s', 'src/tests/packaging', '-p', 'test_*.py'],
        ]
        # 类 Unix 上增加 Linux 凭据的测试
        if os.name == 'posix':
            result.append([python, 'src/native/credentials/test_linux.py'])
        # macOS 上增加小组件宿主测试
        if sys.platform == 'darwin':
            result.append([python, 'platform/ios/Tests/run_tests.py', '--output', str(output / 'widget-host')])
        # NSH 类型检查
        result.append(['nsh', 'check'])
        # 依次运行三组 NSH 界面测试并输出 JUnit 报告
        for name in ['app.test.tsx', 'app-recovery.test.tsx', 'mobile-app.test.tsx', 'image-playback.test.ts']:
            result.append(['nsh', 'test', 'src/tests/desktop/' + name, '--junit',
                           str(output / (name + '.xml'))])
        return result
    # Android 入口测试
    if suite == 'android':
        return [[python, 'src/tests/android/test_entry.py']]
    # 手表缓存测试
    if suite == 'watch':
        return [[python, 'platform/watchos/Tests/run_tests.py', '--report', str(output / 'cache.json')]]
    # iOS 小组件测试
    if suite == 'ios':
        return [[python, 'platform/ios/Tests/run_tests.py', '--output', str(output)]]
    # 像素级视觉测试
    if suite == 'visual':
        return [[python, 'src/tests/visual/run.py', '--suite', 'all', '--output', str(output)]]
    # 其余为真实协议的联调测试
    return [[python, 'src/tests/integration/run.py', '--output', str(output)]]


# 入口：解析参数、准备输出目录并逐条执行命令
def main():
    parser = argparse.ArgumentParser(description=__doc__)
    # 选择套件
    parser.add_argument('--suite', choices=['unit', 'android', 'ios', 'watch', 'visual', 'integration'], default='unit')
    # 结果根目录，按套件分子目录
    parser.add_argument('--output', type=Path, default=ROOT / 'output/tests', help='Results root, grouped by suite')
    args = parser.parse_args()
    # 这些套件依赖 macOS 与 Xcode 工具
    if args.suite in {'ios', 'watch', 'visual', 'integration'} and sys.platform != 'darwin':
        parser.error('This suite requires the existing macOS/Xcode tools')
    output = args.output.expanduser().resolve() / args.suite
    output.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    # 避免生成 .pyc 文件
    environment['PYTHONDONTWRITEBYTECODE'] = '1'
    # 逐条执行，失败立即停止
    for command in commands(args.suite, output):
        # 先打印命令便于排查
        print('+ ' + shlex.join(command), flush=True)
        subprocess.run(command, cwd=ROOT, env=environment, check=True)


# 脚本入口
if __name__ == '__main__':
    try:
        main()
    # 子命令失败时沿用其退出码，且不输出多余的堆栈
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.returncode) from None
