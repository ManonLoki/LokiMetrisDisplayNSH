"""Regression: a renamed Mac executable is never accepted as Windows/Linux."""
# 引入动态加载模块与测试框架
import importlib.util
from pathlib import Path
import tempfile
import unittest

# 仓库根目录
ROOT = Path(__file__).resolve().parents[3]
# 按路径加载打包脚本作为被测模块
spec = importlib.util.spec_from_file_location("package_desktop", ROOT / "tools/build/package_desktop.py")
package_desktop = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package_desktop)


# 可执行文件格式判断的回归测试
class ExecutableFormatTests(unittest.TestCase):
    # 改了扩展名的 Mach-O 不会被当作 Windows/Linux，格式错误的 PE 也被拒绝
    def test_rejects_renamed_macho_and_malformed_pe(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "app.exe"
            # Mach-O 魔数，即使叫 .exe 也应识别为 macOS
            path.write_bytes(b"\xcf\xfa\xed\xfe" + bytes(64))
            self.assertEqual(package_desktop.executable_platform(path), "macos")
            # 只有 MZ 而没有 PE 签名，不是合法 PE
            path.write_bytes(b"MZ" + bytes(62))
            self.assertIsNone(package_desktop.executable_platform(path))

    # 能识别 PE 签名与 ELF
    def test_recognizes_pe_signature_and_elf(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "runner"
            # 带有正确 PE 头偏移与签名的最小 PE
            path.write_bytes(b"MZ" + bytes(58) + (64).to_bytes(4, "little") + b"PE\0\0")
            self.assertEqual(package_desktop.executable_platform(path), "windows")
            # ELF 魔数
            path.write_bytes(b"\x7fELF" + bytes(64))
            self.assertEqual(package_desktop.executable_platform(path), "linux")


# 作为脚本运行时
if __name__ == "__main__":
    # 运行全部用例
    unittest.main()
