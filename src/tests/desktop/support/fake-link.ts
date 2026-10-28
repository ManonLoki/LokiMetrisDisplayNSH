// 引入存储
import { Storage } from "nsh";
// 引入端口接口
import type { LinkPort } from "../../../app/application";
// 引入空视图与配对记录类型
import type { ViewState } from "../../../features/connection";
import type { LinkCommand } from "loki-metris-mobile-display/native";
import { changedOnly } from "./link-port";
// 引入真实原生规则：测试替身也复用 Rust 的校验与投影
import { Rules } from "loki-metris-mobile-display/native";

// 测试替身用的规则别名
const emptyView = (id: string, name: string) => Rules.connectionEmptyView(id, name) as ViewState;
const validDeviceId = (id: string) => Rules.connectionValidDeviceId(id);
const parseSnapshot = (value: unknown, images: Record<string, string> = {}) => Rules.connectionParseSnapshot(value, images) as Snapshot | null;
// 引入快照类型
import type { Snapshot } from "../../../shared/contracts/snapshot";
// 引入视图变换辅助函数
import type { PairingRecord } from "./view-fixture";
// 引入视图变换辅助函数
import { acceptHandshake, acceptSnapshot, expirePairing, forgetDesktop, hostsFrom, selectDesktop } from "./view-fixture";

// 夹具事件：握手、快照、传输失败
type Event =
  | { atMs: number; kind: "handshake" }
  | { atMs: number; kind: "snapshot"; snapshot: Snapshot; receivedAtMs: number; images?: Record<string, string> }
  | { atMs: number; kind: "transport-fail" };
// 夹具：已发现的桌面与事件序列
type Fixture = { discovered?: { id: string; name: string }[]; events?: Event[] };
// 存储键
const PAIRED = "loki.paired", CACHE = "loki.cache", INJECT = "loki.inject";

/** 只模拟原生桥的公开 JSON 投影，不实现 SPAKE2、网络或系统凭据。 */
// 假端口：只模拟公开 JSON 投影
export class FakeLink implements LinkPort {
  // 当前视图状态
  private state = emptyView("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", "LokiMetris Display");
  // 当前图片映射
  private images: Record<string, string> = {};
  // 待触发的事件定时器
  private timers: ReturnType<typeof setTimeout>[] = [];

  // 用夹具、已配对记录与缓存初始化
  private constructor(private fixture: Fixture, paired: Record<string, PairingRecord>, cache: Cache | null) {
    // 载入配对记录
    this.state.paired = paired;
    // 据发现列表与配对记录得到主机列表
    this.state.hosts = hostsFrom(fixture.discovered ?? [], paired);
    // 有缓存则恢复旧快照
    if (cache) {
      this.state.snapshot = cache.snapshot;
      this.state.receivedAtMs = cache.receivedAtMs;
      this.state.cacheSourceId = cache.sourceId;
    }
  }

  // 从存储读取夹具、配对与缓存
  static async load(): Promise<FakeLink> {
    const [fixture, paired, cache] = await Promise.all([
      Storage.getItem(INJECT), Storage.getItem(PAIRED), Storage.getItem(CACHE),
    ]);
    return new FakeLink(readFixture(fixture), readPaired(paired), readCache(cache));
  }

  // 初始化：套用身份并按夹具安排事件
  initialize(deviceId: string, name: string): ViewState {
    this.state.deviceId = deviceId || this.state.deviceId;
    this.state.deviceName = name;
    // 为每个事件安排定时器
    for (const event of this.fixture.events ?? []) {
      this.timers.push(setTimeout(() => this.apply(event), Math.max(0, event.atMs - Date.now())));
    }
    // 与产品 Link 一样：初始化返回首个视图，之后的轮询只返回变化
    return this.changed(this.projection())!;
  }

  // 处理命令
  command(command: LinkCommand): void {
    // 选择桌面
    if (command.op === "select") this.state = selectDesktop(this.state, command.clientId!, Date.now());
    // 忘记桌面：同步更新存储与缓存
    if (command.op === "forget") {
      this.state = forgetDesktop(this.state, command.clientId!);
      void Storage.setItem(PAIRED, JSON.stringify(this.state.paired));
      void this.persistCache();
    }
    // 重命名
    if (command.op === "rename") this.state = { ...this.state, deviceName: command.name! };
    // 刷新：清除错误并提示正在发现
    if (["refresh", "discover"].includes(command.op)) {
      this.state = { ...this.state, error: null, status: "正在发现桌面端…" };
    }
    // 界面不再活跃：取消所有待触发事件
    if (command.op === "active" && command.value === false) {
      for (const timer of this.timers) clearTimeout(timer);
      this.timers = [];
    }
  }

  // 轮询：先让过期的配对失效，再输出投影与图片
  poll(): ViewState | null {
    return this.changed(this.projection());
  }

  // 先让过期的配对失效，再输出投影文本
  private projection(): string {
    this.state = expirePairing(this.state, Date.now());
    return JSON.stringify({ ...this.state, images: this.images });
  }

  // 只在投影变化时返回新视图
  private changed = changedOnly();

  // 应用一个夹具事件
  private apply(event: Event): void {
    const current = this.state;
    // 握手：保存配对
    if (event.kind === "handshake") {
      this.state = acceptHandshake(current, Date.now());
      if (this.state.paired !== current.paired) void Storage.setItem(PAIRED, JSON.stringify(this.state.paired));
    // 快照：解析后接受并缓存
    } else if (event.kind === "snapshot") {
      const snapshot = parseSnapshot(event.snapshot, event.images);
      if (!snapshot) return;
      this.state = acceptSnapshot(current, snapshot, event.receivedAtMs);
      if (this.state === current) return;
      this.images = event.images ?? {};
      void this.persistCache();
    // 传输失败：标记断开
    } else {
      this.state = { ...current, connected: false, live: false,
        error: "连接中断，正在等待桌面恢复…", status: "暂未连接" };
    }
  }

  // 把当前快照写入缓存；缺任何字段则删除缓存
  private async persistCache(): Promise<void> {
    const { snapshot, receivedAtMs, cacheSourceId } = this.state;
    if (snapshot == null || receivedAtMs == null || cacheSourceId == null) {
      await Storage.removeItem(CACHE);
    } else {
      await Storage.setItem(CACHE, JSON.stringify({ sourceId: cacheSourceId, receivedAtMs, snapshot }));
    }
  }
}

// 缓存结构
type Cache = { sourceId: string; receivedAtMs: number; snapshot: Snapshot };
// 宽松解析 JSON，失败返回 null
function parsed(raw: string | null): unknown {
  try { return raw == null ? null : JSON.parse(raw); } catch { return null; }
}
// 判断是否为普通对象
function record(value: unknown): value is Record<string, unknown> {
  return value != null && typeof value === "object" && !Array.isArray(value);
}
// 读取并校验配对记录，丢弃非法项
function readPaired(raw: string | null): Record<string, PairingRecord> {
  const value = parsed(raw), result: Record<string, PairingRecord> = {};
  if (!record(value)) return result;
  for (const [id, item] of Object.entries(value)) {
    if (validDeviceId(id) && record(item) && typeof item.clientId === "string"
      && Number.isSafeInteger(item.savedAtMs) && (item.savedAtMs as number) >= 0) {
      result[id] = { clientId: item.clientId, savedAtMs: item.savedAtMs as number };
    }
  }
  // 返回结果
  return result;
}
// 读取并校验缓存，非法返回 null
function readCache(raw: string | null): Cache | null {
  const value = parsed(raw);
  if (!record(value) || typeof value.sourceId !== "string" || !validDeviceId(value.sourceId)
    || !Number.isSafeInteger(value.receivedAtMs) || (value.receivedAtMs as number) < 0) return null;
  const snapshot = parseSnapshot(value.snapshot);
  return snapshot ? { sourceId: value.sourceId, receivedAtMs: value.receivedAtMs as number, snapshot } : null;
}
// 读取并校验夹具，非法返回空夹具
function readFixture(raw: string | null): Fixture {
  const value = parsed(raw);
  if (!record(value)) return {};
  const fixture = value as Fixture;
  // 发现列表必须是合法数组
  if (fixture.discovered != null && (!Array.isArray(fixture.discovered)
    || !fixture.discovered.every((host) => host && typeof host.name === "string" && validDeviceId(host.id)))) return {};
  // 事件必须是数组且字段合法
  if (fixture.events != null && (!Array.isArray(fixture.events) || !fixture.events.every((event) => event
    && Number.isFinite(event.atMs) && ["handshake", "snapshot", "transport-fail"].includes(event.kind)))) return {};
  return fixture;
}
