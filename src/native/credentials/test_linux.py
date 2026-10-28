#!/usr/bin/env python3
"""Exercise the production Linux credential adapter against a fake CLI boundary.

The host's rustc compiles the Linux platform source plus its public wrappers
from src/native/credentials/mod.rs. This can run on macOS; it does
not validate a Linux OS, D-Bus, libsecret, or an actual Secret Service keyring.
Only secret-tool is replaced. Transaction/chunking logic stays in production Rust.
All records, input files, and executables are synthetic and live in a temporary
directory. No user credentials or third-party Python dependencies are used.
"""
# 引入标准库
from __future__ import annotations

# 摘要
import hashlib
# JSON 序列化
import json
# 环境变量与路径分隔符
import os
# 路径
from pathlib import Path
# 宿主平台信息
import platform
# 查找 rustc
import shutil
# 运行子进程
import subprocess
# 标准错误输出
import sys
# 临时目录
import tempfile
# 计时
import time
# 测试框架
import unittest

# 仓库根目录
ROOT = Path(__file__).resolve().parents[3]
# 生产代码的凭据入口源文件
SOURCE = ROOT / "src/native/credentials/mod.rs"
# 生产代码的 Linux 实现
LINUX_SOURCE = ROOT / "src/native/credentials/linux.rs"

# 假的 secret-tool 脚本：按配置模拟锁定、警告、截断、读回不一致与超时等故障
FAKE_TOOL = r'''#!/usr/bin/env python3
import base64
import hashlib
import json
import os
from pathlib import Path
import sys
import time

# 假存储根目录，由测试通过环境变量指定
root = Path(os.environ["LOKI_FAKE_SECRET_ROOT"])
# 故障配置文件
config_path = root / "config.json"
# 读取故障配置，没有则为空
config = json.loads(config_path.read_text()) if config_path.exists() else {}
# 命令行参数
args = sys.argv[1:]
# 第一个参数是操作：lookup、store、clear
op = args.pop(0)
# 去掉 --label 之类的选项
args = [arg for arg in args if not arg.startswith("--")]
# 其余参数是“属性名 值”成对出现
attributes = dict(zip(args[::2], args[1::2]))
# 
service, account = attributes["service"], attributes["account"]
# 以服务+账户的摘要作为记录文件名
key = hashlib.sha256(json.dumps([service, account]).encode()).hexdigest()
# 
path = root / "records" / key
# 记录每次调用，便于排查
with (root / "events.jsonl").open("a") as log:
    # 
    log.write(json.dumps({"op": op, "account": account}) + "\n")
# 模拟服务被锁定：报错并以 1 退出
if config.get("locked"):
    # 
    print("mock Secret Service is locked", file=sys.stderr)
    # 
    sys.exit(1)
# 模拟查询卡住
if op == "lookup" and config.get("sleep_lookup"):
    # 
    time.sleep(config["sleep_lookup"])
# 查询
if op == "lookup":
    # 不存在：退出码 1
    if not path.exists():
        # 
        sys.exit(1)
    # 
    record = json.loads(path.read_text())
    # 模拟查询时有警告输出
    if config.get("warn_lookup"):
        # 
        print("mock lookup warning", file=sys.stderr)
    # 原样输出保存的字节
    sys.stdout.buffer.write(base64.b64decode(record["value"]))
# 存储
elif op == "store":
    # 从标准输入读取值
    value = sys.stdin.buffer.read()
    # 模拟 secret-tool 成功退出,即使其输入读取器发生了截断。
    # 这个假实现中不应理解清单或代次。
    # 模拟 secret-tool 的静默截断：输入被截断但仍成功退出；这里不理解根索引或代号
    value = value[:8192]
    # 模拟某个分块写入失败
    if config.get("fail_store_suffix") and account.endswith(config["fail_store_suffix"]):
        # 
        print("mock chunk store failure", file=sys.stderr)
        # 
        sys.exit(1)
    # 模拟存下去的内容与输入不一致
    if config.get("mismatch_store"):
        # 
        value += b"!"
    # 保存记录
    path.write_text(json.dumps({"service": service, "account": account,
                               # 
                               "value": base64.b64encode(value).decode()}))
    # 模拟存储时有警告
    if config.get("warn_store"):
        # 
        print("mock store warning", file=sys.stderr)
    # 仅对根索引模拟警告
    if config.get("warn_manifest_store") and "/" not in account:
        # 
        print("mock manifest store warning", file=sys.stderr)
# 清除
elif op == "clear":
    # 模拟清除时有警告且记录保留
    if config.get("warn_clear"):
        # 
        print("mock clear warning; record retained", file=sys.stderr)
        # 
        sys.exit(0)
    # 
    path.unlink(missing_ok=True)
# 未知操作
else:
    # 
    sys.exit(2)
'''

# 附加到生产代码后的 Rust 小驱动：把 read/write/delete 暴露为命令行
RUST_DRIVER = r'''
fn run() -> Result<(), String> {
    // 驱动的命令行参数：操作、服务、账户，必要时再有文件路径
    let args: Vec<String> = std::env::args().collect();
    // 
    let op = args.get(1).ok_or("missing operation")?;
    // 
    let service = args.get(2).ok_or("missing service")?;
    // 
    let account = args.get(3).ok_or("missing account")?;
    // 
    match op.as_str() {
        // 写入：从文件读值
        "write" => {
            // 
            let path = args.get(4).ok_or("missing input file")?;
            // 
            let value = std::fs::read_to_string(path).map_err(|_| "input unavailable")?;
            // 调用生产代码的 write
            write(service, account, &value)?;
            // 
            println!("WRITE_OK");
        }
        // 读取：把内容写到输出文件
        "read" => match read(service, account)? {
            // 
            Some(value) => {
                // 
                let path = args.get(4).ok_or("missing output file")?;
                // 
                std::fs::write(path, value).map_err(|_| "output unavailable")?;
                // 
                println!("SOME");
            }
            // 不存在
            None => println!("NONE"),
        },
        // 删除
        "delete" => { delete(service, account)?; println!("DELETE_OK"); },
        // 
        _ => return Err("unknown operation".into()),
    }
    // 
    Ok(())
}
// 入口：出错时以退出码 2 结束
fn main() {
    // 
    if let Err(error) = run() {
        // 
        eprintln!("STORE_ERROR: {error}");
        // 
        std::process::exit(2);
    }
}
'''


# 把 Linux 实现放进 platform 模块并接上驱动，不改动生产逻辑
def production_adapter(source: str, linux_source: str) -> str:
    """Select the Linux implementation without rewriting its production logic."""
    # 找到第一个平台选择处，在它之前保留公共封装
    first_platform = source.index("\n#[cfg(")
    # 把 Linux 实现包进 platform 模块
    module = "mod platform {\n" + linux_source + "\n}\n"
    # 拼接：公共封装 + 模块 + 驱动
    return source[:first_platform] + "\n" + module + "\n" + RUST_DRIVER


# 针对假 secret-tool 边界的生产适配器测试
class LinuxAdapterBoundaryTests(unittest.TestCase):
    # 编译出的驱动程序
    binary: Path
    # 放假 secret-tool 的目录
    fake_bin: Path
    # 所有用例共用的临时根目录
    suite_root: Path
    # 测试用服务名与账户名
    service = "com.manonloki.verification.linux-fake"
    # 测试用账户名
    account = "synthetic-probe"

    # 每个用例使用独立的临时目录与环境
    def setUp(self):
        # 每个用例一个临时目录
        self.temp = tempfile.TemporaryDirectory(prefix="case-", dir=self.suite_root)
        # 用例结束自动清理
        self.addCleanup(self.temp.cleanup)
        self.case_root = Path(self.temp.name)
        # 假存储的记录目录
        (self.case_root / "records").mkdir()
        # 让 PATH 先找到假的 secret-tool
        self.env = os.environ.copy()
        # 让 PATH 先找到假的 secret-tool
        self.env["PATH"] = str(self.fake_bin) + os.pathsep + self.env.get("PATH", "")
        # 告诉假工具数据放哪
        self.env["LOKI_FAKE_SECRET_ROOT"] = str(self.case_root)
        # 负载文件的序号
        self.sequence = 0

    # 写入故障配置
    def configure(self, **faults):
        # 写入故障配置
        (self.case_root / "config.json").write_text(json.dumps(faults))

    # 调用驱动执行一次操作，并断言成功或失败
    def call(self, op, value=None, *, ok=True, account=None):
        self.sequence += 1
        # 本次调用的输入/输出文件
        path = self.case_root / f"payload-{self.sequence}"
        # 驱动的命令行
        args = [str(self.binary), op, self.service, account or self.account]
        # 写入：把值放进文件交给驱动
        if op == "write":
            # 写入要保存的值（不转换换行）
            path.write_text(value, encoding="utf-8", newline="")
            args.append(str(path))
        # 读取：指定输出文件
        elif op == "read":
            # 读取：指定输出文件
            args.append(str(path))
        # 执行
        result = subprocess.run(args, env=self.env, capture_output=True, timeout=23)
        # 期望成功
        if ok:
            # 期望成功
            self.assertEqual(result.returncode, 0, result.stderr.decode(errors="replace"))
        # 期望失败：不能谎报成功
        else:
            # 期望失败：不能谎报成功
            self.assertNotEqual(result.returncode, 0, "Operation falsely reported success")
            self.assertNotIn(b"WRITE_OK", result.stdout)
            self.assertNotIn(b"DELETE_OK", result.stdout)
        # 读取成功：NONE 表示不存在，SOME 表示读到内容
        if op == "read" and result.returncode == 0:
            # 读不到条目
            if result.stdout.strip() == b"NONE":
                self.assertFalse(path.exists())
                return None
            # 读到内容
            self.assertEqual(result.stdout.strip(), b"SOME")
            return path.read_bytes()
        return result

    # 读出假存储里的全部记录
    def records(self):
        import base64
        # 读出假存储里全部记录，键为账户名
        return {item["account"]: base64.b64decode(item["value"])
                for path in (self.case_root / "records").iterdir()
                for item in [json.loads(path.read_text())]}

    # 先写入一份原始凭据，返回其内容与根索引
    def old_value(self):
        # 先写入一份原始凭据
        value = "original credential\n"
        self.call("write", value)
        # 返回其内容和根索引
        return value.encode(), self.records()[self.account]

    # 断言失败的更新没有替换已提交的根索引，且恢复正常后仍能读出旧值
    def assert_old_preserved(self, expected, manifest):
        # 根索引未被失败的更新替换
        self.assertEqual(self.records()[self.account], manifest,
                         "A failed update replaced the committed manifest")
        # 取消故障后仍能读到旧值
        self.configure()
        self.assertEqual(self.call("read"), expected)

    # 独立验证故障模型：假工具会把超过 8192 字节的输入静默截断
    def test_01_fake_boundary_reproduces_8192_byte_silent_truncation(self):
        # 独立验证故障模型,不借助生产适配器。
        value = b"x" * 10_000
        # 直接调用假工具，不经过生产适配器
        result = subprocess.run([str(self.fake_bin / "secret-tool"), "store", "service",
                                 self.service, "account", "direct-boundary"],
                                input=value, env=self.env, capture_output=True, timeout=5)
        # 假工具成功退出
        self.assertEqual(result.returncode, 0)
        # 但内容被静默截断到 8192 字节
        self.assertEqual(len(self.records()["direct-boundary"]), 8192)

    # 超大的 Unicode 内容与末尾换行都能往返
    def test_02_large_unicode_and_trailing_newlines_roundtrip(self):
        # 超过 8192 字节的 Unicode 内容，末尾带两个换行
        value = "ab" + "界🙂x" * 1800 + "\n\n"
        self.assertGreater(len(value.encode()), 8192)
        self.call("write", value)
        # 往返一致
        self.assertEqual(self.call("read"), value.encode())
        # 应被拆成多个分块，每块不超过 6000 字节且是合法 UTF-8
        records = self.records()
        # 被拆成多个分块
        self.assertGreater(len(records), 2)
        for account, data in records.items():
            # 每个分块不超过 6000 字节且是合法 UTF-8
            if account != self.account:
                self.assertLessEqual(len(data), 6000)
                data.decode("utf-8", errors="strict")

    # 更新、删除以及重复删除
    def test_03_update_delete_and_repeated_delete(self):
        self.call("write", "first")
        # 更新为更长的内容
        replacement = "🙂 replacement\n" * 1300
        self.call("write", replacement)
        self.assertEqual(self.call("read"), replacement.encode())
        # 删除
        self.call("delete")
        # 读不到
        self.assertIsNone(self.call("read"))
        # 重复删除也应成功
        self.call("delete")
        # 删完后不应留下任何记录
        self.assertEqual(self.records(), {})

    # 第二个分块写失败时，保留旧的根索引
    def test_04_second_chunk_failure_preserves_old_manifest(self):
        old, manifest = self.old_value()
        # 第二个分块写入失败
        self.configure(fail_store_suffix="/1")
        self.call("write", "界" * 5000, ok=False)
        self.assert_old_preserved(old, manifest)

    # 存储时有警告输出则失败关闭，并保留旧值
    def test_05_store_warning_fails_closed_and_preserves_old(self):
        old, manifest = self.old_value()
        # 存储时有警告输出
        self.configure(warn_store=True)
        self.call("write", "new credential", ok=False)
        self.assert_old_preserved(old, manifest)

    # 查询时有警告输出则失败关闭，且不会提交新值
    def test_06_lookup_warning_fails_closed(self):
        old, manifest = self.old_value()
        # 查询时有警告输出
        self.configure(warn_lookup=True)
        # 读取失败
        self.call("read", ok=False)
        # 写入也失败（先要查询旧值）
        self.call("write", "must not commit", ok=False)
        self.assert_old_preserved(old, manifest)

    # 读回内容不一致则失败关闭
    def test_07_exact_readback_mismatch_fails_closed(self):
        old, manifest = self.old_value()
        # 存储后读回内容不一致
        self.configure(mismatch_store=True)
        self.call("write", "different", ok=False)
        self.assert_old_preserved(old, manifest)

    # 服务被锁定时，所有操作都返回错误且不覆盖旧值
    def test_08_locked_service_returns_errors_without_overwriting(self):
        old, manifest = self.old_value()
        # 服务被锁定
        self.configure(locked=True)
        # 三种操作都应报错
        for op in ["read", "write", "delete"]:
            self.call(op, "not committed" if op == "write" else None, ok=False)
        self.assert_old_preserved(old, manifest)

    # 清除时有警告不能报告成功；恢复后重试要能完成删除（含墓碑）
    def test_09_clear_warning_must_not_report_success(self):
        self.call("write", "delete probe")
        # 清除时有警告且记录被保留
        self.configure(warn_clear=True)
        # 不能报告成功
        self.call("delete", ok=False)
        # 一旦可用,重试必须完成删除,包括墓碑记录。
        self.configure()
        # 恢复正常后重试能完成删除（含墓碑）
        self.call("delete")
        self.assertIsNone(self.call("read"))

    # 根索引存储出现警告时保留旧的一代
    def test_10_manifest_store_warning_preserves_old_generation(self):
        old, manifest = self.old_value()
        # 根索引存储时有警告
        self.configure(warn_manifest_store=True)
        self.call("write", "candidate replacement", ok=False)
        # 旧的一代保持完好
        self.assert_old_preserved(old, manifest)

    # 空秘密可以往返；含 NUL 的值与含换行的标识会被拒绝
    def test_11_empty_secret_roundtrip_and_input_validation(self):
        self.assertIsNone(self.call("read"))
        # 空值可以保存
        self.call("write", "")
        # 并读回为空
        self.assertEqual(self.call("read"), b"")
        # 含 NUL 的值被拒绝
        self.call("write", "invalid\0secret", ok=False)
        # 含换行的账户名被拒绝
        self.call("read", account="bad\nidentity", ok=False)
        # 被拒绝的操作没有改动已有的空值
        self.assertEqual(self.call("read"), b"")

    # 子进程超过期限会失败关闭（约 15 秒后）
    def test_12_child_process_deadline_fails_closed(self):
        # 查询会睡 20 秒
        self.configure(sleep_lookup=20)
        start = time.monotonic()
        result = self.call("read", ok=False)
        elapsed = time.monotonic() - start
        # 报告超时
        self.assertIn("超时".encode(), result.stderr)
        # 约 15 秒后放弃
        self.assertGreaterEqual(elapsed, 14)
        self.assertLess(elapsed, 20)


# 入口：编译带假工具的适配器并运行全部用例
def main():
    rustc = shutil.which("rustc")
    if not rustc:
        raise SystemExit("rustc is required; no tool installation attempted")
    # 记录生产源码，运行结束后确认没有被改动
    original = SOURCE.read_bytes()
    linux_original = LINUX_SOURCE.read_bytes()
    # 说明测试范围：只验证适配器与假 secret-tool 的边界，不是真实 Linux 集成
    print(f"Host: {platform.system()} ({platform.machine()})", flush=True)
    # 说明测试范围：只验证适配器与假工具的边界
    print("Scope: production Linux adapter compiled on this host; fake secret-tool subprocess boundary.", flush=True)
    print("This is NOT a Linux OS / D-Bus / actual Secret Service integration test.", flush=True)
    # 打印源码摘要，便于核对运行的是哪一版
    print("Production source SHA256: " + hashlib.sha256(original).hexdigest(), flush=True)
    print("Linux source SHA256: " + hashlib.sha256(linux_original).hexdigest(), flush=True)
    with tempfile.TemporaryDirectory(prefix="loki-linux-store-boundary-") as temp:
        directory = Path(temp)
        fake_bin = directory / "bin"
        fake_bin.mkdir()
        # 安装假工具
        tool = fake_bin / "secret-tool"
        tool.write_text(FAKE_TOOL)
        tool.chmod(0o700)
        # 生成并编译适配器
        source = directory / "adapter.rs"
        source.write_text(production_adapter(original.decode(), linux_original.decode()))
        binary = directory / "adapter"
        # 编译
        subprocess.run([rustc, "--edition", "2024", str(source), "-o", str(binary)],
                       check=True, capture_output=True, timeout=60)
        # 把路径交给测试类
        LinuxAdapterBoundaryTests.binary = binary
        LinuxAdapterBoundaryTests.fake_bin = fake_bin
        LinuxAdapterBoundaryTests.suite_root = directory
        # 运行全部用例
        result = unittest.TextTestRunner(verbosity=2).run(
            unittest.defaultTestLoader.loadTestsFromTestCase(LinuxAdapterBoundaryTests))
    # 运行期间生产源码不得被改动
    unchanged = SOURCE.read_bytes() == original and LINUX_SOURCE.read_bytes() == linux_original
    if not unchanged:
        print("Production source changed during the run; rerun against the final source.", file=sys.stderr)
    return 0 if result.wasSuccessful() and unchanged else 1


# 入口
if __name__ == "__main__":
    # 运行主函数并以其返回值退出
    raise SystemExit(main())
