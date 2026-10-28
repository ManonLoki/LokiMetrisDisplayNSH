#!/usr/bin/env python3
# 文档字符串：说明脚本用途
"""Test the Widget host's cancellation lifecycle using the existing Swift compiler."""
# 引入标准库
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

# 仓库根目录
ROOT = Path(__file__).resolve().parents[3]


# 入口：编译并运行小组件宿主的取消与完成生命周期测试
def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'output/tests/ios')
    args = parser.parse_args()
    # 使用已安装的 Swift 编译器，不会尝试安装
    compiler = shutil.which('swiftc')
    if not compiler:
        parser.error('The existing Swift compiler is required; no installation was attempted')
    output = args.output.expanduser().resolve()
    output.mkdir(parents=True, exist_ok=True)
    # 被测的宿主桥接、提交闸门与测试本身
    sources = [ROOT / 'platform/ios/bridge/WidgetHostBridge.swift',
               ROOT / 'platform/ios/shared/WidgetRefreshCommitGate.swift',
               ROOT / 'platform/ios/Tests/WidgetBackgroundExecutionTests.swift']
    executable = output / 'widget-background-execution-tests'
    # 编译
    subprocess.run([compiler, '-parse-as-library', *map(str, sources), '-o', str(executable)],
                   cwd=ROOT, check=True)
    # 运行
    subprocess.run([str(executable)], cwd=ROOT, check=True)
    # 报告：通过状态、用例数、范围声明与源码摘要
    report = {
        'passed': True,
        'testCount': 5,
        # 只覆盖不依赖 UIKit 的取消与完成生命周期，不证明系统调度、WidgetKit 渲染或真机刷新
        'scope': 'Foundation-only background execution cancellation and completion lifecycle; '
                 'does not prove OS scheduling, WidgetKit rendering or device refresh',
        'sourceHashes': {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
                         for path in sources},
    }
    (output / 'widget-background-execution.json').write_text(json.dumps(report, indent=2) + '\n')


# 作为脚本运行时
if __name__ == '__main__':
    # 调用入口
    main()
