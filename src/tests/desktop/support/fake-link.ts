import { Storage } from "nsh";
import type { LinkPort } from "../../../app/application";
import { emptyView, type PairingRecord } from "../../../features/connection";
import { parseSnapshot, validDeviceId } from "../../../features/connection";
import type { Snapshot } from "../../../shared/contracts/snapshot";
import { acceptHandshake, acceptSnapshot, expirePairing, forgetDesktop, hostsFrom, selectDesktop } from "./view-fixture";

type Event =
  | { atMs: number; kind: "handshake" }
  | { atMs: number; kind: "snapshot"; snapshot: Snapshot; receivedAtMs: number; images?: Record<string, string> }
  | { atMs: number; kind: "transport-fail" };
type Fixture = { discovered?: { id: string; name: string }[]; events?: Event[] };
const PAIRED = "loki.paired", CACHE = "loki.cache", INJECT = "loki.inject";

/** 只模拟原生桥的公开 JSON 投影，不实现 SPAKE2、网络或系统凭据。 */
export class FakeLink implements LinkPort {
  private state = emptyView("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", "LokiMetris Display");
  private images: Record<string, string> = {};
  private timers: ReturnType<typeof setTimeout>[] = [];

  private constructor(private fixture: Fixture, paired: Record<string, PairingRecord>, cache: Cache | null) {
    this.state.paired = paired;
    this.state.hosts = hostsFrom(fixture.discovered ?? [], paired);
    if (cache) {
      this.state.snapshot = cache.snapshot;
      this.state.receivedAtMs = cache.receivedAtMs;
      this.state.cacheSourceId = cache.sourceId;
    }
  }

  static async load(): Promise<FakeLink> {
    const [fixture, paired, cache] = await Promise.all([
      Storage.getItem(INJECT), Storage.getItem(PAIRED), Storage.getItem(CACHE),
    ]);
    return new FakeLink(readFixture(fixture), readPaired(paired), readCache(cache));
  }

  initialize(deviceId: string, name: string): string {
    this.state.deviceId = deviceId || this.state.deviceId;
    this.state.deviceName = name;
    for (const event of this.fixture.events ?? []) {
      this.timers.push(setTimeout(() => this.apply(event), Math.max(0, event.atMs - Date.now())));
    }
    return this.poll();
  }

  command(raw: string): void {
    const command = JSON.parse(raw) as { op: string; clientId?: string; name?: string; value?: boolean };
    if (command.op === "select") this.state = selectDesktop(this.state, command.clientId!, Date.now());
    if (command.op === "forget") {
      this.state = forgetDesktop(this.state, command.clientId!);
      void Storage.setItem(PAIRED, JSON.stringify(this.state.paired));
      void this.persistCache();
    }
    if (command.op === "rename") this.state = { ...this.state, deviceName: command.name! };
    if (["refresh", "discover"].includes(command.op)) {
      this.state = { ...this.state, error: null, status: "正在发现桌面端…" };
    }
    if (command.op === "active" && command.value === false) {
      for (const timer of this.timers) clearTimeout(timer);
      this.timers = [];
    }
  }

  poll(): string {
    this.state = expirePairing(this.state, Date.now());
    return JSON.stringify({ ...this.state, images: this.images });
  }

  private apply(event: Event): void {
    const current = this.state;
    if (event.kind === "handshake") {
      this.state = acceptHandshake(current, Date.now());
      if (this.state.paired !== current.paired) void Storage.setItem(PAIRED, JSON.stringify(this.state.paired));
    } else if (event.kind === "snapshot") {
      const snapshot = parseSnapshot(event.snapshot, event.images);
      if (!snapshot) return;
      this.state = acceptSnapshot(current, snapshot, event.receivedAtMs);
      if (this.state === current) return;
      this.images = event.images ?? {};
      void this.persistCache();
    } else {
      this.state = { ...current, connected: false, live: false,
        error: "连接中断，正在等待桌面恢复…", status: "暂未连接" };
    }
  }

  private async persistCache(): Promise<void> {
    const { snapshot, receivedAtMs, cacheSourceId } = this.state;
    if (snapshot == null || receivedAtMs == null || cacheSourceId == null) {
      await Storage.removeItem(CACHE);
    } else {
      await Storage.setItem(CACHE, JSON.stringify({ sourceId: cacheSourceId, receivedAtMs, snapshot }));
    }
  }
}

type Cache = { sourceId: string; receivedAtMs: number; snapshot: Snapshot };
function parsed(raw: string | null): unknown {
  try { return raw == null ? null : JSON.parse(raw); } catch { return null; }
}
function record(value: unknown): value is Record<string, unknown> {
  return value != null && typeof value === "object" && !Array.isArray(value);
}
function readPaired(raw: string | null): Record<string, PairingRecord> {
  const value = parsed(raw), result: Record<string, PairingRecord> = {};
  if (!record(value)) return result;
  for (const [id, item] of Object.entries(value)) {
    if (validDeviceId(id) && record(item) && typeof item.clientId === "string"
      && Number.isSafeInteger(item.savedAtMs) && (item.savedAtMs as number) >= 0) {
      result[id] = { clientId: item.clientId, savedAtMs: item.savedAtMs as number };
    }
  }
  return result;
}
function readCache(raw: string | null): Cache | null {
  const value = parsed(raw);
  if (!record(value) || typeof value.sourceId !== "string" || !validDeviceId(value.sourceId)
    || !Number.isSafeInteger(value.receivedAtMs) || (value.receivedAtMs as number) < 0) return null;
  const snapshot = parseSnapshot(value.snapshot);
  return snapshot ? { sourceId: value.sourceId, receivedAtMs: value.receivedAtMs as number, snapshot } : null;
}
function readFixture(raw: string | null): Fixture {
  const value = parsed(raw);
  if (!record(value)) return {};
  const fixture = value as Fixture;
  if (fixture.discovered != null && (!Array.isArray(fixture.discovered)
    || !fixture.discovered.every((host) => host && typeof host.name === "string" && validDeviceId(host.id)))) return {};
  if (fixture.events != null && (!Array.isArray(fixture.events) || !fixture.events.every((event) => event
    && Number.isFinite(event.atMs) && ["handshake", "snapshot", "transport-fail"].includes(event.kind)))) return {};
  return fixture;
}
