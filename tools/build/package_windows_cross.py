#!/usr/bin/env python3
"""Build the Windows MSVC runner on macOS, preserving an explicit runtime gap.

Requires existing nsh, cargo-xwin and Xcode installations. NSH 0.2.0's
cross-package runner is replaced by the verified PE built for Windows;
its generated bundle and metadata remain the normal NSH package layout.
"""
# 引入标准库
import argparse
# 摘要算法
import hashlib
# JSON 读写
import json
# 操作系统接口
import os
# 路径对象
from pathlib import Path
# 平台信息
import platform
# 文件工具
import shutil
# 二进制结构
import struct
# 子进程
import subprocess
# 临时目录
import tempfile
# 压缩包
import zipfile

# 复用根目录与可执行文件平台判断
from package_desktop import ROOT, executable_platform

# 包内附带的说明文件模板
PACKAGE_README = """诡秘神谕 · Windows 展示端

解压整个目录，双击 {name}.exe。请保留旁边的 nsh 文件夹。
运行环境需要 Microsoft VC++ x64 Runtime（提供 VCRUNTIME140.dll 和
VCRUNTIME140_1.dll），以及 Microsoft Edge WebView2 Runtime。

电脑采集客户端需要包含桌面终端平台支持的 0.10.0 源码版本。
在展示端设置页选择电脑，再在采集客户端中输入展示端的四位配对码。

本包为 Windows x86-64 MSVC 交叉构建，已核对真实 PE 与多尺寸应用图标。
Windows 实机启动、凭据存储和配对验收仍为待办。
"""


# 入口：在 macOS 上交叉构建 Windows 运行器，并明确标出尚未做实机验证
def main():
    # 创建参数解析器
    parser = argparse.ArgumentParser(description=__doc__)
    # 输出目录
    parser.add_argument("--out", type=Path, default=ROOT / "output/packages")
    # 解析参数
    args = parser.parse_args()
    # 仅支持使用本机已有的 Xcode/libclang
    if platform.system() != "Darwin":
        # 只支持 macOS 主机
        raise SystemExit("This entry point uses the existing macOS Xcode/libclang installation")
    # 依赖的工具必须已安装，不会尝试安装
    for command in ("nsh", "cargo", "cargo-xwin", "xcode-select", "sips", "x86_64-w64-mingw32-windres"):
        # 必需工具缺失
        if not shutil.which(command):
            # 退出，不尝试安装
            raise SystemExit(f"Required tool is missing: {command}; no installation was attempted")
    # 规范化输出目录
    output = args.out.resolve()
    # 目标子目录
    destination = output / "windows-cross"
    # 拒绝覆盖已有产物
    if destination.exists():
        # 拒绝覆盖已有输出
        raise SystemExit(f"Refusing to replace existing output: {destination}")
    # 创建输出目录
    output.mkdir(parents=True, exist_ok=True)
    # 读取应用元数据
    app = json.loads((ROOT / "nsh.json").read_text())
    # 应用名
    name = app["name"]
    # 在临时目录里构建
    with tempfile.TemporaryDirectory(prefix="loki-windows-cross-", dir=output) as temporary:
        # 暂存目录
        stage = Path(temporary)
        # 先让 nsh 生成标准的包布局
        subprocess.run(["nsh", "package", "--target", "windows", "--out", str(stage)], cwd=ROOT, check=True)
        # 生成的运行器清单路径
        manifest = ROOT / ".nsh/rust/Cargo.toml"
        # 运行器清单
        runner_manifest = ROOT / ".nsh/rust/windows-app/Cargo.toml"
        # 读取生成的包名,而不依赖检出目录的
        # 目录名。此入口已经要求 Python 3.11+。
        # 读取生成的包名，而不依赖检出目录的名字
        import tomllib
        # 运行器包名
        runner_name = tomllib.loads(runner_manifest.read_text())["package"]["name"]
        # 复制环境变量
        env = os.environ.copy()
        # 定位 Xcode 的 libclang
        developer = subprocess.check_output(["xcode-select", "-p"], text=True).strip()
        # Xcode 工具链里的 libclang 目录
        clang = Path(developer) / "Toolchains/XcodeDefault.xctoolchain/usr/lib"
        # 库不存在
        if not (clang / "libclang.dylib").is_file():
            # 退出
            raise SystemExit(f"Xcode libclang was not found at {clang}")
        # 设置 LIBCLANG_PATH
        env["LIBCLANG_PATH"] = str(clang)
        # 图标资源的临时目录
        resources = stage / "resources"
        # 创建资源目录
        resources.mkdir()
        # 图标路径
        icon = (ROOT / app["icon"]).resolve()
        # 资源管理器、任务栏和标题栏分别取不同尺寸，只有 256 像素一档时小尺寸会显示为空
        sizes = (16, 24, 32, 48, 64, 128, 256)
        # 各尺寸的 PNG 载荷
        payloads = []
        # 逐个尺寸缩放
        for size in sizes:
            # 目标文件
            target = resources / f"icon-{size}.png"
            # 缩放为方形 PNG
            subprocess.run(["sips", "-z", str(size), str(size), str(icon), "--out", str(target)], check=True)
            # 读取转换后的 PNG
            png = target.read_bytes()
            # 确认尺寸正确
            if png[:8] != b"\x89PNG\r\n\x1a\n" or struct.unpack(">II", png[16:24]) != (size, size):
                # 图标必须是对应像素的 PNG
                raise SystemExit(f"Expected a {size}-pixel PNG icon")
            # 记录载荷
            payloads.append(png)
        # 图标目录头：保留位、类型 1、条目数
        header = struct.pack("<HHH", 0, 1, len(sizes))
        # 载荷起始偏移
        offset = len(header) + 16 * len(sizes)
        # 目录条目
        entries = b""
        # 逐个生成条目，256 像素在 ICO 里记为 0
        for size, png in zip(sizes, payloads):
            # 宽高、色数、保留、平面数、位深、长度、偏移
            entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(png), offset)
            # 下一个载荷的偏移
            offset += len(png)
        # 现代 Windows 的 ICO 支持 PNG 载荷；用标准库拼容器，保留原始图案
        (resources / "icon.ico").write_bytes(header + entries + b"".join(payloads))
        # 资源脚本
        (resources / "icon.rc").write_text('1 ICON "icon.ico"\n')
        # 图标资源目标文件
        resource_object = resources / "icon.o"
        # 编译为 COFF 资源对象
        subprocess.run(["x86_64-w64-mingw32-windres", "-i", "icon.rc", "-O", "coff", "-o", str(resource_object)],
                       # 在资源目录执行
                       cwd=resources, check=True)
        # 交叉编译 Windows 运行器，并链接图标资源
        subprocess.run(["cargo", "xwin", "rustc", "--manifest-path", str(manifest), "-p", runner_name,
                        # 目标为 Windows x86-64，发布构建
                        "--target", "x86_64-pc-windows-msvc", "--release", "--",
                        # 链接参数：图标资源文件
                        "-C", f"link-arg={resource_object}"], cwd=ROOT, env=env, check=True)
        # 查询构建输出目录
        cargo = json.loads(subprocess.check_output(["cargo", "metadata", "--manifest-path", str(manifest),
                                                  # 不取依赖，格式版本 1
                                                  "--no-deps", "--format-version", "1"], cwd=ROOT, text=True))
        # 交叉构建出的可执行文件路径
        executable = Path(cargo["target_directory"]) / "x86_64-pc-windows-msvc/release" / (runner_name + ".exe")
        # 产物必须真的是 PE
        if executable_platform(executable) != "windows":
            # 不是 PE 格式则拒绝
            raise SystemExit("Cross-built runner is not PE; package rejected")
        # 并且必须是 x86-64 架构
        with executable.open("rb") as source:
            # 跳到 PE 头偏移位置
            source.seek(60)
            # 读取 PE 头偏移
            offset = int.from_bytes(source.read(4), "little")
            # 跳到机器类型字段
            source.seek(offset + 4)
            # 不是 x86-64
            if struct.unpack("<H", source.read(2))[0] != 0x8664:
                # 退出
                raise SystemExit("Expected an x86-64 Windows PE runner")
        # 包目录
        package = stage / name
        # 用真正的 Windows 可执行文件替换宿主运行器
        shutil.copy2(executable, package / f"{name}.exe")
        # 写入 README
        (package / "README.txt").write_text(PACKAGE_README.format(name=name), encoding="utf-8")
        # 压缩包路径
        archive = stage / f"{name}.zip"
        # 最初的 CLI 压缩包含有其不可信的宿主运行器。仅在实际的
        # Windows 可执行文件替换完成之后再重建它。
        # 初始压缩包含不可信的宿主运行器；等换上真正的 Windows 可执行文件后才重新打包
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
            # 遍历包内全部文件
            for path in sorted(package.rglob("*")):
                # 只处理普通文件
                if path.is_file():
                    # 以相对路径写入压缩包
                    bundle.write(path, path.relative_to(stage))
        # 清理临时资源
        shutil.rmtree(resources)
        # 记录校验证据：格式已验证，运行时尚未验证
        evidence = {
            # 目标平台、架构与构建主机
            "target": "windows", "architecture": "x86_64", "buildHost": "macos",
            # 目标三元组与交叉编译标记
            "targetTriple": "x86_64-pc-windows-msvc", "crossCompiled": True,
            # 运行器格式已验证，运行时未验证
            "runnerFormatVerified": True, "runtimeVerified": False,
            # 运行器摘要
            "runnerSha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
            # 图标来源与已嵌入资源
            "iconSource": app["icon"], "iconResourceEmbedded": True,
            # 说明：Windows 启动等待运行时验证
            "note": "Windows launch, Credential Manager, WebView2 and production pairing require Windows runtime acceptance."
        }
        # 写入平台验证证据
        (stage / "platform-verification.json").write_text(json.dumps(evidence, indent=2) + "\n")
        # 输出目录已存在
        if destination.exists():
            # 拒绝覆盖
            raise SystemExit(f"Output already exists: {destination}")
        # 暂存目录改名为输出目录
        stage.rename(destination)
    # 提示交叉构建完成，运行时待验证
    print(f"Cross-built Windows PE package (runtime pending): {destination}")


# 作为脚本运行时
if __name__ == "__main__":
    # 调用入口
    main()
