#!/usr/bin/env python3
"""Package on the target OS and reject executables for another platform.

NSH 0.2.0 can report success for --target windows/linux on macOS while
packaging a Mach-O runner. This entry point prevents that false success.
"""
# 引入标准库
import argparse
# JSON 读写
import json
# 路径对象
from pathlib import Path
# 平台信息
import platform
# 文件工具
import shutil
# 子进程
import subprocess
# 标准输入输出
import sys
# 临时目录
import tempfile

# 仓库根目录
ROOT = Path(__file__).resolve().parents[2]


# 根据可执行文件头判断它属于哪个平台
def executable_platform(path):
    # 读取可执行文件头
    with Path(path).open("rb") as source:
        # 取前 64 字节
        magic = source.read(64)
        # ELF：Linux
        if magic[:4] == b"\x7fELF":
            # ELF：Linux
            return "linux"
        # Mach-O 各种魔数（含通用二进制）：macOS
        if magic[:4] in (b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xfe\xed\xfa\xcf", b"\xca\xfe\xba\xbe", b"\xbe\xba\xfe\xca", b"\xca\xfe\xba\xbf", b"\xbf\xba\xfe\xca"):
            # Mach-O：macOS
            return "macos"
        # PE：先看 MZ，再跳到偏移处核对 PE 签名
        if len(magic) >= 64 and magic[:2] == b"MZ":
            # PE 头偏移
            pe = int.from_bytes(magic[60:64], "little")
            # 跳到 PE 头
            source.seek(pe)
            # 签名为 PE
            if source.read(4) == b"PE\0\0":
                # Windows
                return "windows"
    # 无法识别
    return None


# 入口：打包并校验产物格式与目标平台一致
def main():
    # 创建参数解析器
    parser = argparse.ArgumentParser(description=__doc__)
    # 目标平台
    parser.add_argument("--target", choices=("macos", "windows", "linux"), required=True)
    # 输出目录
    parser.add_argument("--out", type=Path, default=ROOT / "output/packages")
    # 解析参数
    args = parser.parse_args()
    # 当前主机平台
    host = {"Darwin": "macos", "Windows": "windows", "Linux": "linux"}.get(platform.system())
    # 必须在目标系统上构建
    if host != args.target:
        # 主机与目标平台不一致
        raise SystemExit(f"Build {args.target} on a {args.target} host. Current host: {host}. No package was produced.")
    # 查找 nsh，不会尝试安装
    cli = shutil.which("nsh")
    # 没找到 nsh
    if not cli:
        # 退出，不尝试安装
        raise SystemExit("nsh is not installed; no tool installation was attempted")
    # 规范化输出目录
    output = args.out.resolve()
    # 目标子目录
    destination = output / args.target
    # 拒绝覆盖已有产物
    if destination.exists():
        # 拒绝覆盖已有输出
        raise SystemExit(f"Refusing to replace existing output: {destination}")
    # 创建输出目录
    output.mkdir(parents=True, exist_ok=True)
    # 读取应用元数据
    metadata = json.loads((ROOT / "nsh.json").read_text())
    # 先在临时目录里打包
    with tempfile.TemporaryDirectory(prefix="loki-package-", dir=output) as stage:
        # 暂存目录
        stage = Path(stage)
        # 调用 nsh 打包
        subprocess.run([cli, "package", "--target", args.target, "--out", str(stage)], cwd=ROOT, check=True)
        # 应用名
        name = metadata["name"]
        # 找出各平台的运行器可执行文件
        if args.target == "macos":
            # macOS：app 包里的可执行文件
            executables = list((stage / f"{name}.app/Contents/MacOS").glob("*"))
        # Windows
        elif args.target == "windows":
            # exe 路径
            executables = [stage / name / f"{name}.exe"]
        # 其他
        else:
            # Linux 的 App 可执行文件
            executables = [stage / name / "App"]
        # 找不到可执行文件或格式与目标不符，则拒绝产物
        if not executables or any(not path.is_file() or executable_platform(path) != args.target for path in executables):
            # 可执行文件格式与目标不符
            raise SystemExit("Runner executable format does not match target; package rejected")
        # 写入校验记录：仅证明格式正确，不代表运行时验证
        (stage / "platform-verification.json").write_text(json.dumps({"target": args.target, "buildHost": host, "nshExecutable": str(Path(cli).resolve()), "runnerFormatVerified": True, "runtimeVerified": False, "note": "Executable format is a packaging check, not an application runtime test."}, indent=2) + "\n")
        # 只有通过验证的包才会离开暂存目录。避免替换
        # 任何先前的交付物,即使两次构建同时完成。
        # 只有校验通过的产物才离开暂存目录；并发构建时也不覆盖先完成的交付物
        if destination.exists():
            # 输出已存在
            raise SystemExit(f"Output already exists: {destination}")
        # 暂存目录改名为输出目录
        stage.rename(destination)
    # 提示已验证的原生包
    print(f"Validated native package: {destination}")


# 作为脚本运行时
if __name__ == "__main__":
    # 调用入口
    main()
