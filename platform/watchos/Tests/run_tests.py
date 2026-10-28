#!/usr/bin/env python3
# 文档字符串：脚本用途
"""Run Watch cache, presentation and image regressions on macOS."""
# 引入标准库
import argparse
from pathlib import Path
import subprocess
import tempfile

# 仓库根目录
ROOT = Path(__file__).resolve().parents[3]


# 入口：在 macOS 上运行手表缓存、展示和图片的回归测试
def main():
    parser = argparse.ArgumentParser(description=__doc__)
    # Rust 生成的清单、清空状态和按摘要命名的资源，用于互操作测试
    parser.add_argument('--interop', type=Path, help='Rust-produced manifest.json, clear.json and hash-named assets')
    # 缓存测试报告位置
    parser.add_argument('--report', type=Path, default=ROOT / 'output/tests/watch/cache.json')
    # 图片测试报告位置，默认放在缓存报告旁
    parser.add_argument('--image-report', type=Path, help='Image test report; defaults beside the cache report')
    args = parser.parse_args()
    report = args.report.expanduser().resolve()
    report.parent.mkdir(parents=True, exist_ok=True)
    image_report = (args.image_report.expanduser().resolve() if args.image_report
                    else report.with_name(report.stem + '-images.json'))
    sources = ROOT / 'platform/watchos'
    # 在临时目录里编译并运行
    with tempfile.TemporaryDirectory(prefix='loki-watch-cache-tests-') as directory:
        # 缓存测试
        executable = Path(directory) / 'cache-tests'
        subprocess.run(['xcrun', 'swiftc', str(sources / 'Sources/Contract.swift'),
                        str(sources / 'Sources/SnapshotCache.swift'), str(sources / 'Sources/BackgroundDelivery.swift'),
                        str(sources / 'Tests/CacheTests.swift'),
                        '-o', str(executable)], check=True, cwd=ROOT)
        command = [str(executable), str(ROOT / 'src/assets/branding/app-icon.png'), str(report)]
        if args.interop:
            command.extend(['--interop', str(args.interop.expanduser().resolve())])
        # 在仓库根目录执行编译或运行命令
        subprocess.run(command, check=True, cwd=ROOT)
        # 展示测试：用与 Rust 共用的夹具核对名称和文案
        presentation = Path(directory) / 'presentation-tests'
        subprocess.run(['xcrun', 'swiftc', str(sources / 'Sources/Contract.swift'),
                        str(sources / 'Sources/AmountText.swift'), str(sources / 'Sources/DisplayNames.swift'), str(sources / 'Tests/PresentationTests.swift'),
                        '-o', str(presentation)], check=True, cwd=ROOT)
        subprocess.run([str(presentation), str(ROOT / 'src/tests/fixtures/display-names.json')], check=True, cwd=ROOT)
        # 图片测试
        images = Path(directory) / 'image-tests'
        subprocess.run(['xcrun', 'swiftc', str(sources / 'Sources/WatchImageAsset.swift'),
                        str(sources / 'Tests/ImageTests.swift'), '-o', str(images)], check=True, cwd=ROOT)
        subprocess.run([str(images), str(ROOT / 'src/tests/fixtures/assets/hook.png'),
                        str(Path(directory) / 'image-assets'), str(image_report)], check=True, cwd=ROOT)
    print(report)
    print(image_report)


# 作为脚本运行时
if __name__ == '__main__':
    # 调用入口
    main()
