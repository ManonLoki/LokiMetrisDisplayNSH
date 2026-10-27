"""Regression: a renamed Mac executable is never accepted as Windows/Linux."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location("package_desktop", ROOT / "tools/build/package_desktop.py")
package_desktop = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package_desktop)


class ExecutableFormatTests(unittest.TestCase):
    def test_rejects_renamed_macho_and_malformed_pe(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "app.exe"
            path.write_bytes(b"\xcf\xfa\xed\xfe" + bytes(64))
            self.assertEqual(package_desktop.executable_platform(path), "macos")
            path.write_bytes(b"MZ" + bytes(62))
            self.assertIsNone(package_desktop.executable_platform(path))

    def test_recognizes_pe_signature_and_elf(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "runner"
            path.write_bytes(b"MZ" + bytes(58) + (64).to_bytes(4, "little") + b"PE\0\0")
            self.assertEqual(package_desktop.executable_platform(path), "windows")
            path.write_bytes(b"\x7fELF" + bytes(64))
            self.assertEqual(package_desktop.executable_platform(path), "linux")


if __name__ == "__main__":
    unittest.main()
