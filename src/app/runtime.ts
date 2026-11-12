import { cleanup, effect, platform, signal, untracked, type Bounds, type Signal, type Size } from "nsh";
import { Shell, type LinkCommand, type SettingsResult } from "loki-metris-mobile-display/native";
import type { ViewState } from "../features/connection";
import type { PanelConfig } from "../shared/contracts/panel";

/** 应用运行时：前台事实与观测回调由宿主注入；连接由 Rust 的 Shell 持有。 */
export interface AppRuntime {
  // 当前是否在前台，未知为 null
  foreground?: () => boolean | null;
  remoteNavigation?: () => boolean;
  // 测试用：上报面板边界
  onFrame?: (id: "usage" | "hook", bounds: Bounds) => void;
  // 测试用：上报图片加载尺寸
  onImageLoad?: (hash: string, size: Size) => void;
}

// 外壳要更新的界面信号
interface UiState {
  panel: Signal<PanelConfig>;
  view: Signal<ViewState>;
  nameDraft: Signal<string>;
  nameFeedback: Signal<string | null>;
  nameIssue: Signal<string | null>;
  panelIssue: Signal<string | null>;
  savingName: Signal<boolean>;
}

/**
 * 连接变化由 Rust 通知。界面按 Rust 给出的截止时间触发名称确认超时。
 * TS 唯一的判断是：异步结果返回时用户若又改了草稿，就不覆盖名称相关字段。
 */
export function createRuntime(state: UiState, connectionVisible?: () => boolean) {
  // 是否已挂载，避免重复启动
  let mounted = false;
  let disposed = false;
  // 外壳是否已启动；之前下发的 active 命令会被忽略
  const started = signal(false);
  let nameTimer: ReturnType<typeof setTimeout> | undefined;
  let fixtureTimer: ReturnType<typeof setTimeout> | undefined;

  // 把 Rust 返回的公开状态赋给信号
  function accept(result: SettingsResult | null | undefined, draftAtRequest?: string) {
    if (disposed || !result) return;
    state.savingName.set(result.savingName);
    if (nameTimer !== undefined) clearTimeout(nameTimer);
    nameTimer = undefined;
    const deadline = result.savingName ? Shell.nameConfirmationDeadline() : null;
    if (deadline != null) {
      nameTimer = setTimeout(() => void refresh(), Math.max(0, deadline - Date.now()));
    }
    // 请求期间用户又改了草稿，就不覆盖名称相关字段
    if (draftAtRequest === undefined || state.nameDraft() === draftAtRequest) {
      state.nameIssue.set(result.nameIssue ?? null);
      state.nameFeedback.set(result.nameFeedback ?? null);
      if (result.draft != null) state.nameDraft.set(result.draft);
    }
    state.panelIssue.set(result.panelIssue ?? null);
    if (result.panel) state.panel.set(result.panel);
    if (result.view) state.view.set(result.view);
  }

  // 调用外壳并应用结果
  async function run(call: () => Promise<SettingsResult | null | undefined>) {
    const draft = state.nameDraft();
    accept(await call(), draft);
  }

  const refresh = () => run(() => Shell.tick(state.nameDraft(), Date.now()));

  // 调试夹具按 NSH 测试时钟推进模拟的桌面事件。
  function tickFixture(startedAt: number) {
    if (disposed) return;
    void refresh();
    const delay = 250 - (Date.now() - startedAt) % 250;
    fixtureTimer = setTimeout(() => tickFixture(startedAt), delay);
  }

  async function start() {
    const startedAt = Date.now();
    await run(() => Shell.start(startedAt, platform.idiom));
    started.set(true);
    while (!disposed) {
      const changed = await Shell.waitForViewChange();
      if (disposed || changed == null) return;
      if (!changed) {
        tickFixture(startedAt);
        return;
      }
      await refresh();
    }
  }

  function mount() {
    if (mounted) return;
    mounted = true;
    // 前后台由 NSH 的 lifecycle() 提供，连接 owner 据此暂停或恢复发现与会话
    if (connectionVisible) {
      effect(() => {
        const visible = connectionVisible();
        if (!started()) return;
        untracked(() => void command({ op: "active", value: visible }));
      });
    }
    effect(() => {
      untracked(() => void start());
      cleanup(() => {
        disposed = true;
        if (nameTimer !== undefined) clearTimeout(nameTimer);
        if (fixtureTimer !== undefined) clearTimeout(fixtureTimer);
        void Shell.dispose().catch(() => { /* VM 可能已开始关闭。 */ });
      });
    });
  }

  const command = (command: LinkCommand) => run(() => Shell.command(command, state.nameDraft(), Date.now()));

  return {
    mount,
    command,
    editName: (draft: string) => { state.nameDraft.set(draft); return run(() => Shell.editName(draft)); },
    saveName: () => run(() => Shell.saveName(state.nameDraft(), Date.now())),
    savePanel: (panel: PanelConfig) => run(() => Shell.savePanel(panel, state.nameDraft(), Date.now())),
  };
}
