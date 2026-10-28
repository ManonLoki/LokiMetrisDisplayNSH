#!/usr/bin/env python3
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
