#!/usr/bin/env python3
"""Exercise the production Linux credential adapter against a fake CLI boundary.

The host's rustc compiles the Linux platform source plus its public wrappers
from src/native/credentials/mod.rs. This can run on macOS; it does
not validate a Linux OS, D-Bus, libsecret, or an actual Secret Service keyring.
Only secret-tool is replaced. Transaction/chunking logic stays in production Rust.
All records, input files, and executables are synthetic and live in a temporary
directory. No user credentials or third-party Python dependencies are used.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "src/native/credentials/mod.rs"
LINUX_SOURCE = ROOT / "src/native/credentials/linux.rs"

FAKE_TOOL = r'''#!/usr/bin/env python3
import base64
import hashlib
import json
import os
from pathlib import Path
import sys
import time

root = Path(os.environ["LOKI_FAKE_SECRET_ROOT"])
config_path = root / "config.json"
config = json.loads(config_path.read_text()) if config_path.exists() else {}
args = sys.argv[1:]
op = args.pop(0)
args = [arg for arg in args if not arg.startswith("--")]
attributes = dict(zip(args[::2], args[1::2]))
service, account = attributes["service"], attributes["account"]
key = hashlib.sha256(json.dumps([service, account]).encode()).hexdigest()
path = root / "records" / key
with (root / "events.jsonl").open("a") as log:
    log.write(json.dumps({"op": op, "account": account}) + "\n")
if config.get("locked"):
    print("mock Secret Service is locked", file=sys.stderr)
    sys.exit(1)
if op == "lookup" and config.get("sleep_lookup"):
    time.sleep(config["sleep_lookup"])
if op == "lookup":
    if not path.exists():
        sys.exit(1)
    record = json.loads(path.read_text())
    if config.get("warn_lookup"):
        print("mock lookup warning", file=sys.stderr)
    sys.stdout.buffer.write(base64.b64decode(record["value"]))
elif op == "store":
    value = sys.stdin.buffer.read()
    # Model secret-tool's successful exit even when its input reader truncates.
    # No understanding of manifests or generations belongs in this fake.
    value = value[:8192]
    if config.get("fail_store_suffix") and account.endswith(config["fail_store_suffix"]):
        print("mock chunk store failure", file=sys.stderr)
        sys.exit(1)
    if config.get("mismatch_store"):
        value += b"!"
    path.write_text(json.dumps({"service": service, "account": account,
                               "value": base64.b64encode(value).decode()}))
    if config.get("warn_store"):
        print("mock store warning", file=sys.stderr)
    if config.get("warn_manifest_store") and "/" not in account:
        print("mock manifest store warning", file=sys.stderr)
elif op == "clear":
    if config.get("warn_clear"):
        print("mock clear warning; record retained", file=sys.stderr)
        sys.exit(0)
    path.unlink(missing_ok=True)
else:
    sys.exit(2)
'''

RUST_DRIVER = r'''
fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let op = args.get(1).ok_or("missing operation")?;
    let service = args.get(2).ok_or("missing service")?;
    let account = args.get(3).ok_or("missing account")?;
    match op.as_str() {
        "write" => {
            let path = args.get(4).ok_or("missing input file")?;
            let value = std::fs::read_to_string(path).map_err(|_| "input unavailable")?;
            write(service, account, &value)?;
            println!("WRITE_OK");
        }
        "read" => match read(service, account)? {
            Some(value) => {
                let path = args.get(4).ok_or("missing output file")?;
                std::fs::write(path, value).map_err(|_| "output unavailable")?;
                println!("SOME");
            }
            None => println!("NONE"),
        },
        "delete" => { delete(service, account)?; println!("DELETE_OK"); },
        _ => return Err("unknown operation".into()),
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("STORE_ERROR: {error}");
        std::process::exit(2);
    }
}
'''


def production_adapter(source: str, linux_source: str) -> str:
    """Select the Linux implementation without rewriting its production logic."""
    first_platform = source.index("\n#[cfg(")
    module = "mod platform {\n" + linux_source + "\n}\n"
    return source[:first_platform] + "\n" + module + "\n" + RUST_DRIVER


class LinuxAdapterBoundaryTests(unittest.TestCase):
    binary: Path
    fake_bin: Path
    suite_root: Path
    service = "com.manonloki.verification.linux-fake"
    account = "synthetic-probe"

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="case-", dir=self.suite_root)
        self.addCleanup(self.temp.cleanup)
        self.case_root = Path(self.temp.name)
        (self.case_root / "records").mkdir()
        self.env = os.environ.copy()
        self.env["PATH"] = str(self.fake_bin) + os.pathsep + self.env.get("PATH", "")
        self.env["LOKI_FAKE_SECRET_ROOT"] = str(self.case_root)
        self.sequence = 0

    def configure(self, **faults):
        (self.case_root / "config.json").write_text(json.dumps(faults))

    def call(self, op, value=None, *, ok=True, account=None):
        self.sequence += 1
        path = self.case_root / f"payload-{self.sequence}"
        args = [str(self.binary), op, self.service, account or self.account]
        if op == "write":
            path.write_text(value, encoding="utf-8", newline="")
            args.append(str(path))
        elif op == "read":
            args.append(str(path))
        result = subprocess.run(args, env=self.env, capture_output=True, timeout=23)
        if ok:
            self.assertEqual(result.returncode, 0, result.stderr.decode(errors="replace"))
        else:
            self.assertNotEqual(result.returncode, 0, "Operation falsely reported success")
            self.assertNotIn(b"WRITE_OK", result.stdout)
            self.assertNotIn(b"DELETE_OK", result.stdout)
        if op == "read" and result.returncode == 0:
            if result.stdout.strip() == b"NONE":
                self.assertFalse(path.exists())
                return None
            self.assertEqual(result.stdout.strip(), b"SOME")
            return path.read_bytes()
        return result

    def records(self):
        import base64
        return {item["account"]: base64.b64decode(item["value"])
                for path in (self.case_root / "records").iterdir()
                for item in [json.loads(path.read_text())]}

    def old_value(self):
        value = "original credential\n"
        self.call("write", value)
        return value.encode(), self.records()[self.account]

    def assert_old_preserved(self, expected, manifest):
        self.assertEqual(self.records()[self.account], manifest,
                         "A failed update replaced the committed manifest")
        self.configure()
        self.assertEqual(self.call("read"), expected)

    def test_01_fake_boundary_reproduces_8192_byte_silent_truncation(self):
        # Verify the fault model independently, without the production adapter.
        value = b"x" * 10_000
        result = subprocess.run([str(self.fake_bin / "secret-tool"), "store", "service",
                                 self.service, "account", "direct-boundary"],
                                input=value, env=self.env, capture_output=True, timeout=5)
        self.assertEqual(result.returncode, 0)
        self.assertEqual(len(self.records()["direct-boundary"]), 8192)

    def test_02_large_unicode_and_trailing_newlines_roundtrip(self):
        value = "ab" + "界🙂x" * 1800 + "\n\n"
        self.assertGreater(len(value.encode()), 8192)
        self.call("write", value)
        self.assertEqual(self.call("read"), value.encode())
        records = self.records()
        self.assertGreater(len(records), 2)
        for account, data in records.items():
            if account != self.account:
                self.assertLessEqual(len(data), 6000)
                data.decode("utf-8", errors="strict")

    def test_03_update_delete_and_repeated_delete(self):
        self.call("write", "first")
        replacement = "🙂 replacement\n" * 1300
        self.call("write", replacement)
        self.assertEqual(self.call("read"), replacement.encode())
        self.call("delete")
        self.assertIsNone(self.call("read"))
        self.call("delete")
        self.assertEqual(self.records(), {})

    def test_04_second_chunk_failure_preserves_old_manifest(self):
        old, manifest = self.old_value()
        self.configure(fail_store_suffix="/1")
        self.call("write", "界" * 5000, ok=False)
        self.assert_old_preserved(old, manifest)

    def test_05_store_warning_fails_closed_and_preserves_old(self):
        old, manifest = self.old_value()
        self.configure(warn_store=True)
        self.call("write", "new credential", ok=False)
        self.assert_old_preserved(old, manifest)

    def test_06_lookup_warning_fails_closed(self):
        old, manifest = self.old_value()
        self.configure(warn_lookup=True)
        self.call("read", ok=False)
        self.call("write", "must not commit", ok=False)
        self.assert_old_preserved(old, manifest)

    def test_07_exact_readback_mismatch_fails_closed(self):
        old, manifest = self.old_value()
        self.configure(mismatch_store=True)
        self.call("write", "different", ok=False)
        self.assert_old_preserved(old, manifest)

    def test_08_locked_service_returns_errors_without_overwriting(self):
        old, manifest = self.old_value()
        self.configure(locked=True)
        for op in ["read", "write", "delete"]:
            self.call(op, "not committed" if op == "write" else None, ok=False)
        self.assert_old_preserved(old, manifest)

    def test_09_clear_warning_must_not_report_success(self):
        self.call("write", "delete probe")
        self.configure(warn_clear=True)
        self.call("delete", ok=False)
        # Once available, retrying must finish deletion, including tombstones.
        self.configure()
        self.call("delete")
        self.assertIsNone(self.call("read"))

    def test_10_manifest_store_warning_preserves_old_generation(self):
        old, manifest = self.old_value()
        self.configure(warn_manifest_store=True)
        self.call("write", "candidate replacement", ok=False)
        self.assert_old_preserved(old, manifest)

    def test_11_empty_secret_roundtrip_and_input_validation(self):
        self.assertIsNone(self.call("read"))
        self.call("write", "")
        self.assertEqual(self.call("read"), b"")
        self.call("write", "invalid\0secret", ok=False)
        self.call("read", account="bad\nidentity", ok=False)
        self.assertEqual(self.call("read"), b"")

    def test_12_child_process_deadline_fails_closed(self):
        self.configure(sleep_lookup=20)
        start = time.monotonic()
        result = self.call("read", ok=False)
        elapsed = time.monotonic() - start
        self.assertIn("超时".encode(), result.stderr)
        self.assertGreaterEqual(elapsed, 14)
        self.assertLess(elapsed, 20)


def main():
    rustc = shutil.which("rustc")
    if not rustc:
        raise SystemExit("rustc is required; no tool installation attempted")
    original = SOURCE.read_bytes()
    linux_original = LINUX_SOURCE.read_bytes()
    print(f"Host: {platform.system()} ({platform.machine()})", flush=True)
    print("Scope: production Linux adapter compiled on this host; fake secret-tool subprocess boundary.", flush=True)
    print("This is NOT a Linux OS / D-Bus / actual Secret Service integration test.", flush=True)
    print("Production source SHA256: " + hashlib.sha256(original).hexdigest(), flush=True)
    print("Linux source SHA256: " + hashlib.sha256(linux_original).hexdigest(), flush=True)
    with tempfile.TemporaryDirectory(prefix="loki-linux-store-boundary-") as temp:
        directory = Path(temp)
        fake_bin = directory / "bin"
        fake_bin.mkdir()
        tool = fake_bin / "secret-tool"
        tool.write_text(FAKE_TOOL)
        tool.chmod(0o700)
        source = directory / "adapter.rs"
        source.write_text(production_adapter(original.decode(), linux_original.decode()))
        binary = directory / "adapter"
        subprocess.run([rustc, "--edition", "2024", str(source), "-o", str(binary)],
                       check=True, capture_output=True, timeout=60)
        LinuxAdapterBoundaryTests.binary = binary
        LinuxAdapterBoundaryTests.fake_bin = fake_bin
        LinuxAdapterBoundaryTests.suite_root = directory
        result = unittest.TextTestRunner(verbosity=2).run(
            unittest.defaultTestLoader.loadTestsFromTestCase(LinuxAdapterBoundaryTests))
    unchanged = SOURCE.read_bytes() == original and LINUX_SOURCE.read_bytes() == linux_original
    if not unchanged:
        print("Production source changed during the run; rerun against the final source.", file=sys.stderr)
    return 0 if result.wasSuccessful() and unchanged else 1


if __name__ == "__main__":
    raise SystemExit(main())
