import { cleanup, effect, untracked, type Bounds, type Signal, type Size } from "nsh";
import { Settings } from "loki-metris-mobile-display/native";
import { emptyView, nativeView, type ViewState } from "../features/connection";
import type { PanelConfig } from "../shared/contracts/panel";

export interface LinkPort {
  initialize(deviceId: string, name: string): string;
  command(command: string): void;
  poll(): string;
}

/** 正常入口注入真实端口，测试宿主仍可注入同一公开投影的替身。 */
export interface AppRuntime {
  link: LinkPort;
  foreground: () => boolean | null;
  remoteNavigation?: () => boolean;
  onFrame?: (id: "usage" | "hook", bounds: Bounds) => void;
  onImageLoad?: (hash: string, size: Size) => void;
}

interface UiState {
  panel: Signal<PanelConfig>;
  view: Signal<ViewState>;
  nameDraft: Signal<string>;
  nameFeedback: Signal<string | null>;
  nameIssue: Signal<string | null>;
  panelIssue: Signal<string | null>;
  savingName: Signal<boolean>;
  foreground: Signal<boolean>;
}

interface SettingsResult {
  session: number;
  savingName: boolean;
  nameIssue: string | null;
  nameFeedback: string | null;
  panelIssue: string | null;
  panel: PanelConfig | null;
  device: { id: string; name: string } | null;
  initialize: { id: string; name: string } | null;
  draft: string | null;
  view: ViewState | null;
  commands: Record<string, unknown>[];
}

/** 这里只处理 UI 时钟和端口投递；恢复、保存、确认、超时和回滚由 Settings 决定。 */
export function createRuntime(runtime: AppRuntime, state: UiState, onView: (view: ViewState) => void) {
  let session = 0;
  let mounted = false;
  let disposed = false;
  let nativeStarted = false;
  let nativeReady = false;
  let lastNativeState = "";

  async function request(value: Record<string, unknown>): Promise<SettingsResult> {
    return JSON.parse(await Settings.call(JSON.stringify({ session, ...value }))) as SettingsResult;
  }

  function accept(result: SettingsResult, draftAtRequest?: string) {
    if (disposed) return;
    session = result.session;
    state.savingName.set(result.savingName);
    // 端口响应期间继续输入时，界面保留新的草稿和输入反馈。
    if (draftAtRequest === undefined || state.nameDraft() === draftAtRequest) {
      state.nameIssue.set(result.nameIssue);
      state.nameFeedback.set(result.nameFeedback);
      if (result.draft != null) state.nameDraft.set(result.draft);
    }
    state.panelIssue.set(result.panelIssue);
    if (result.panel) state.panel.set(result.panel);
    if (result.view) state.view.set(result.view);
  }

  async function update(value: Record<string, unknown>) {
    const draft = state.nameDraft();
    const result = await request(value);
    accept(result, draft);
    return result;
  }

  async function fail(stage: string, error: unknown) {
    if (disposed) return;
    try { await update({ op: "portFailure", view: state.view(), stage, error: String(error) }); }
    catch { /* 已关闭或被新界面替代的会话不能再改变其 UI。 */ }
  }

  async function poll() {
    if (disposed) return;
    const raw = runtime.link.poll();
    const next = raw === lastNativeState ? state.view() : nativeView(raw);
    if (raw !== lastNativeState) {
      lastNativeState = raw;
      onView(next);
      state.view.set(next);
    }
    // 即使连接投影未变，也让 Rust 观察时钟，名称确认才能按时结束。
    await update({ op: "observeName", view: next, draft: state.nameDraft(), nowMs: Date.now() });
  }

  async function command(value: Record<string, unknown>) {
    if (!nativeStarted || disposed) return;
    try {
      runtime.link.command(JSON.stringify(value));
      await poll();
    } catch (error) { await fail("command", error); }
  }

  function mount() {
    if (mounted) return;
    mounted = true;
    effect(() => {
      let timer: ReturnType<typeof setTimeout> | null = null;
      const initialize = async () => {
        try {
          const restored = await update({ op: "boot" });
          if (disposed) return;
          const device = restored.device!;
          state.view.set(emptyView(device.id, device.name));
          try {
            const identity = restored.initialize!;
            const raw = runtime.link.initialize(identity.id, identity.name);
            nativeStarted = true;
            const next = nativeView(raw);
            state.view.set(next);
            lastNativeState = raw;
            await update({ op: "initialized", view: next });
            if (disposed) return;
            nativeReady = true;
          } catch (error) { await fail("initialize", error); }
        } catch (error) { await fail("settings", error); }
      };
      untracked(() => {
        void initialize();
        const tick = () => {
          if (disposed) return;
          try { state.foreground.set(runtime.foreground() === true); }
          catch { state.foreground.set(false); }
          if (nativeReady) {
            void poll().catch((error) => fail("poll", error));
          }
          timer = setTimeout(tick, 250);
        };
        tick();
      });
      cleanup(() => {
        disposed = true;
        if (timer != null) clearTimeout(timer);
        state.foreground.set(false);
        void request({ op: "dispose" }).catch(() => { /* VM 可能已开始关闭。 */ });
        if (nativeStarted) {
          try { runtime.link.command(JSON.stringify({ op: "active", value: false })); }
          catch { /* 宿主可能已关闭原生 owner。 */ }
        }
      });
    });
  }

  async function editName(draft: string) {
    state.nameDraft.set(draft);
    try { await update({ op: "editName", draft }); } catch (error) { await fail("settings", error); }
  }

  async function saveName() {
    if (disposed) return;
    const result = await update({ op: "beginNameSave", draft: state.nameDraft(), view: state.view(), nowMs: Date.now() });
    if (disposed) return;
    try {
      for (const value of result.commands) runtime.link.command(JSON.stringify(value));
    } catch {
      await update({ op: "nameDispatchFailed" });
      return;
    }
    if (result.commands.length) {
      try { await poll(); } catch (error) { await fail("poll", error); }
    }
  }

  async function savePanel(panel: PanelConfig) {
    if (disposed) return;
    const result = await update({ op: "savePanel", panel });
    for (const value of result.commands) await command(value);
  }

  return { mount, command, editName, saveName, savePanel };
}
