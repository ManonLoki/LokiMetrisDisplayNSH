// 引入响应式原语
import { cleanup, effect, platform, untracked, type Bounds, type Signal, type Size } from "nsh";
// 引入 Rust 外壳与它返回的公开状态类型
import { Shell, type LinkCommand, type SettingsResult } from "loki-metris-mobile-display/native";
// 引入视图与面板类型
import type { ViewState } from "../features/connection";
import type { PanelConfig } from "../shared/contracts/panel";

/** 应用运行时：前台事实与观测回调由宿主注入；连接由 Rust 的 Shell 持有。 */
export interface AppRuntime {
  // 当前是否在前台，未知为 null
  foreground?: () => boolean | null;
  // 是否启用遥控导航
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
export function createRuntime(state: UiState, onView: (view: ViewState) => void) {
  // 是否已挂载，避免重复启动
  let mounted = false;
  // 是否已释放
  let disposed = false;
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
    if (result.view) {
      onView(result.view);
      state.view.set(result.view);
    }
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

  return {
    mount,
    // 下发连接命令
    command: (command: LinkCommand) => run(() => Shell.command(command, state.nameDraft(), Date.now())),
    // 编辑名称草稿
    editName: (draft: string) => { state.nameDraft.set(draft); return run(() => Shell.editName(draft)); },
    // 保存名称
    saveName: () => run(() => Shell.saveName(state.nameDraft(), Date.now())),
    // 保存面板
    savePanel: (panel: PanelConfig) => run(() => Shell.savePanel(panel, state.nameDraft(), Date.now())),
  };
}
