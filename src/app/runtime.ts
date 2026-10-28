// 引入响应式原语
import { cleanup, effect, untracked, type Bounds, type Signal, type Size } from "nsh";
// 引入 Rust 外壳与它返回的公开状态类型
import { Shell, type LinkCommand, type SettingsResult } from "loki-metris-mobile-display/native";
// 引入视图与面板类型
import type { ViewState } from "../features/connection";
import type { PanelConfig } from "../shared/contracts/panel";

/** 应用运行时：前台事实与观测回调由宿主注入；连接由 Rust 的 Shell 持有。 */
export interface AppRuntime {
  // 当前是否在前台，未知为 null
  foreground: () => boolean | null;
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
  foreground: Signal<boolean>;
}

/**
 * 这里只有 UI 时钟和信号赋值：启动、轮询、保存、命令转发、回滚和超时都由 Rust 的 Shell 决定。
 * TS 唯一的判断是：异步结果返回时用户若又改了草稿，就不覆盖名称相关字段。
 */
export function createRuntime(runtime: AppRuntime, state: UiState, onView: (view: ViewState) => void) {
  // 是否已挂载，避免重复启动
  let mounted = false;
  // 是否已释放
  let disposed = false;

  // 把 Rust 返回的公开状态赋给信号
  function accept(result: SettingsResult | null | undefined, draftAtRequest?: string) {
    if (disposed || !result) return;
    state.savingName.set(result.savingName);
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

  // 挂载：启动外壳并开始 250ms 的 UI 时钟
  function mount() {
    if (mounted) return;
    mounted = true;
    effect(() => {
      let timer: ReturnType<typeof setTimeout> | null = null;
      untracked(() => {
        // 启动：Rust 恢复身份、初始化连接（调试构建下测试夹具由 Rust 扮演桌面端）
        void run(() => Shell.start(Date.now()));
        // 时钟：读前台事实，并让 Rust 轮询、推进名称确认
        const tick = () => {
          if (disposed) return;
          try { state.foreground.set(runtime.foreground() === true); }
          catch { state.foreground.set(false); }
          void run(() => Shell.tick(state.nameDraft(), Date.now()));
          timer = setTimeout(tick, 250);
        };
        tick();
      });
      cleanup(() => {
        disposed = true;
        if (timer != null) clearTimeout(timer);
        state.foreground.set(false);
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
