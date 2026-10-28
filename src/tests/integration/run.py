#!/usr/bin/env python3
"""Run the normal macOS app against a real TCP two-frame GIF fixture.

Saves six native screenshots and checks their pixels for both fixture colors.
Transfer, decoding and changing rendered frames must all pass.
"""
# 引入标准库
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time

# 仓库根目录
ROOT = Path(__file__).resolve().parents[3]
# 夹具桌面的客户端 ID
CLIENT = "f0f0f0f0-f0f0-40f0-80f0-f0f0f0f0f0f0"


# 停止子进程：先温和终止，5 秒后强制结束
def stop(process):
    if process is None or process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        # 等待进程退出，最多 5 秒
        process.wait(timeout=5)


# 用 Swift 脚本检查六张原生截图中确实出现了两种夹具颜色且在变化
def verify_frames(output):
    frames = [output / f"frame-{index}.png" for index in range(6)]
    result = subprocess.run(["swift", str(Path(__file__).resolve().with_name("verify-animation-frames.swift")),
                             *map(str, frames)], cwd=ROOT, capture_output=True, text=True, timeout=90)
    # 保存检查日志
    (output / "frame-check.log").write_text(result.stdout + result.stderr)
    if result.returncode:
        raise RuntimeError("Rendered GIF frames did not change; inspect frame-check.log")
    report = json.loads(result.stdout)
    (output / "frames.json").write_text(json.dumps(report, indent=2) + "\n")
    if not report.get("changingRenderedFrames"):
        raise RuntimeError("Native screenshots did not contain both fixture colors")


# 入口：启动夹具桌面与真实应用，转发配对码，并校验动图帧
def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "output/tests/integration")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    # 读取场景模板并替换入口与输出目录占位符
    scenario = (ROOT / "src/tests/scenarios/desktop-animation.scenario.tsx").read_text()
    scenario = scenario.replace('"__APP_ENTRY__"', json.dumps(str(ROOT / "src/tests/desktop/native-app.tsx")))
    scenario = scenario.replace('"__OUTPUT_DIRECTORY__"', json.dumps(str(output)))
    # 写成临时测试文件
    target = ROOT / ".nsh/test/desktop-animation.test.tsx"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(scenario)
    fixture = app = None
    try:
        # 启动夹具桌面（真实加密 TCP，带双帧 GIF）
        with (output / "fixture.log").open("w") as fixture_log:
            fixture = subprocess.Popen(["cargo", "run", "--example", "protocol_host", "--", "--id", CLIENT,
                                        "--name", "Loki 动图验证台", "--asset",
                                        "src/tests/fixtures/assets/two-frame.gif"],
                                       cwd=ROOT, stdin=subprocess.PIPE, stdout=fixture_log,
                                       stderr=subprocess.STDOUT, text=True)
            # 最多等 60 秒就绪
            deadline = time.monotonic() + 60
            while "FIXTURE_READY" not in (output / "fixture.log").read_text():
                if fixture.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError("Animation fixture did not become ready; inspect fixture.log")
                time.sleep(0.1)
            # 每次运行使用全新的身份:被中断的运行遗留的配对不得改变场景。
            # 每次运行使用全新身份，中断的运行遗留的配对不能影响场景
            data = output / "native-data"
            shutil.rmtree(data, ignore_errors=True)
            env = os.environ.copy()
            env["LOKI_TEST_DATA_DIR"] = str(data)
            with (output / "app.log").open("w") as log:
                # 在 macOS 上运行真实应用的场景测试
                app = subprocess.Popen(["nsh", "test", str(target), "--target", "macos", "--verbose",
                                        "--junit", str(output / "results.xml")], cwd=ROOT, env=env,
                                       stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
                assert app.stdout is not None
                # 逐行读取输出
                for line in app.stdout:
                    # 发现应用打印的配对码，就写入夹具的标准输入
                    code = re.search(r"ANIMATION_PAIR_CODE=([0-9]{4})", line)
                    if code:
                        assert fixture.stdin is not None
                        fixture.stdin.write(code[1] + "\n")
                        fixture.stdin.flush()
                    # 其余输出写日志并回显
                    else:
                        log.write(line)
                        log.flush()
                        print(line, end="", flush=True)
                result = app.wait()
                if result != 0:
                    raise SystemExit(result)
        verify_frames(output)
    # 无论成败都停止进程并删除临时测试文件
    finally:
        stop(app)
        stop(fixture)
        target.unlink(missing_ok=True)
    print("Authenticated GIF transfer, decoding and changing native red/blue frames passed.")


# 作为脚本运行时
if __name__ == "__main__":
    # 调用入口
    main()
