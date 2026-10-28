// 引入响应式原语：清理钩子、副作用、非追踪读取，以及边界/信号/尺寸类型
import { cleanup, effect, untracked, type Bounds, type Signal, type Size } from "nsh";
// 引入原生设置模块，Rust 侧负责设置的恢复、保存与确认
import { Rules, Settings, type LinkCommand, type SettingsResult } from "loki-metris-mobile-display/native";
// 引入连接视图的空状态构造与原生状态解析
import type { ViewState } from "../features/connection";
// 引入面板配置类型
import type { PanelConfig } from "../shared/contracts/panel";

// 原生连接端口的最小抽象
export interface LinkPort {
  // 用设备 id 与名称初始化连接，返回 Rust 已校验的公开视图
  initialize(deviceId: string, name: string): ViewState;
  // 下发一条命令
  command(command: LinkCommand): void;
  // 拉取最新视图；自上次返回以来没有变化时为 null
  poll(): ViewState | null;
}

/** 正常入口注入真实端口，测试宿主仍可注入同一公开投影的替身。 */
// 应用运行时接口
export interface AppRuntime {
  // 连接端口
  link: LinkPort;
  // 查询是否在前台，null 表示未知
  foreground: () => boolean | null;
  // 是否启用遥控器导航（电视形态）
  remoteNavigation?: () => boolean;
  // 测试用：上报卡片布局边界
  onFrame?: (id: "usage" | "hook", bounds: Bounds) => void;
  // 测试用：上报图片加载后的尺寸
  onImageLoad?: (hash: string, size: Size) => void;
}

// 运行时读写的界面信号集合
interface UiState {
  // 面板配置
  panel: Signal<PanelConfig>;
  // 连接视图状态
  view: Signal<ViewState>;
  // 设备名称输入草稿
  nameDraft: Signal<string>;
  // 名称保存后的反馈文案
  nameFeedback: Signal<string | null>;
  // 名称输入的校验问题
  nameIssue: Signal<string | null>;
  // 面板设置的错误提示
  panelIssue: Signal<string | null>;
  // 名称是否正在保存
  savingName: Signal<boolean>;
  // 界面是否在前台
  foreground: Signal<boolean>;
}

// Rust Settings 模块每次调用的返回结构

/** 这里只处理 UI 时钟和端口投递；恢复、保存、确认、超时和回滚由 Settings 决定。 */
// 创建运行时：把界面状态与视图回调接上
export function createRuntime(runtime: AppRuntime, state: UiState, onView: (view: ViewState) => void) {
  // 当前设置会话序号
  let session = 0;
  // 是否已挂载，防止重复挂载
  let mounted = false;
  // 是否已销毁，销毁后所有异步回调直接返回
  let disposed = false;
  // 原生端口是否已调用 initialize
  let nativeStarted = false;
  // 原生端口是否已完成初始化流程，可以开始轮询
  let nativeReady = false;

  // 调用 Rust 设置模块，并自动携带会话序号
  function accept(result: SettingsResult, draftAtRequest?: string) {
    // 已销毁则忽略
    if (disposed) return;
    // 同步最新会话序号
    session = result.session;
    // 同步保存中状态
    state.savingName.set(result.savingName);
    // 端口响应期间继续输入时，界面保留新的草稿和输入反馈。
    if (draftAtRequest === undefined || state.nameDraft() === draftAtRequest) {
      // 同步名称问题到界面状态
      state.nameIssue.set(result.nameIssue ?? null);
      // 同步名称反馈到界面状态
      state.nameFeedback.set(result.nameFeedback ?? null);
      // 有草稿时更新名称草稿
      if (result.draft != null) state.nameDraft.set(result.draft);
    }
    // 同步面板错误提示
    state.panelIssue.set(result.panelIssue ?? null);
    // 有新面板配置则覆盖
    if (result.panel) state.panel.set(result.panel);
    // 有新连接视图则覆盖
    if (result.view) state.view.set(result.view);
  }

  // 发起请求并应用结果，返回原始结果供调用方继续处理
  async function update(call: () => Promise<SettingsResult>) {
    // 记录请求发出时的草稿
    const draft = state.nameDraft();
    // 向引擎发送请求
    const result = await call();
    // 接受响应并更新界面状态
    accept(result, draft);
    // 返回响应
    return result;
  }

  // 把端口错误上报给 Rust，由其决定如何展示
  async function fail(stage: string, error: unknown) {
    // 已销毁则不再处理
    if (disposed) return;
    // 上报端口失败
    try { await update(() => Settings.portFailure(session, state.view(), stage, String(error))); }
    // 已关闭或被取代的会话不再改变界面
    catch { /* 已关闭或被新界面替代的会话不能再改变其 UI。 */ }
  }

  // 轮询原生状态，并驱动名称确认的时钟
  async function poll() {
    // 已销毁则不再处理
    if (disposed) return;
    // Rust 只在投影变化时返回新视图，未变时为 null
    const changed = runtime.link.poll();
    // 未变化时沿用当前视图
    const next = changed ?? state.view();
    // 状态变化：通知外部并更新信号
    if (changed) {
      // 通知视图变化
      onView(next);
      // 更新视图状态
      state.view.set(next);
    }
    // 即使连接投影未变，也让 Rust 观察时钟，名称确认才能按时结束。
    await update(() => Settings.observeName(session, state.nameDraft(), next, Date.now()));
  }

  // 向端口下发命令并立即轮询以获取结果
  async function command(value: LinkCommand) {
    // 端口未启动或已销毁时忽略
    if (!nativeStarted || disposed) return;
    // 尝试发送命令
    try {
      // 把命令交给原生链接
      runtime.link.command(value);
      // 随后轮询最新状态
      await poll();
    // 出错则上报命令失败
    } catch (error) { await fail("command", error); }
  }

  // 挂载运行时：启动初始化与周期轮询
  function mount() {
    // 保证只挂载一次
    if (mounted) return;
    // 标记已挂载
    mounted = true;
    // 在响应式副作用中管理定时器与清理
    effect(() => {
      // 轮询定时器句柄
      let timer: ReturnType<typeof setTimeout> | null = null;
      // 初始化流程：先从 Rust 恢复设备身份，再初始化原生端口
      const initialize = async () => {
        // 尝试读取设置
        try {
          // 请求 Rust 恢复启动数据
          const restored = await update(() => Settings.boot());
          // 已销毁则返回
          if (disposed) return;
          // 设备身份必然存在
          const device = restored.device!;
          // 先显示空视图
          state.view.set(Rules.connectionEmptyView(device.id, device.name));
          // 尝试初始化原生链接
          try {
            // 取得用于原生端口初始化的身份
            const identity = restored.initialize!;
            // 初始化并取回 Rust 已校验的公开视图
            const next = runtime.link.initialize(identity.id, identity.name);
            // 标记端口已启动，之后才允许下发命令
            nativeStarted = true;
            // 更新视图状态
            state.view.set(next);
            // 通知 Rust 初始化完成
            await update(() => Settings.initialized(session, next));
            // 已销毁则返回
            if (disposed) return;
            // 之后的 tick 才开始轮询
            nativeReady = true;
          // 初始化失败的上报
          } catch (error) { await fail("initialize", error); }
        // 读取设置失败的上报
        } catch (error) { await fail("settings", error); }
      };
      // 在非追踪上下文中启动，避免副作用订阅这些读取
      untracked(() => {
        // 开始初始化
        void initialize();
        // 每 250ms 的心跳：同步前台状态并轮询
        const tick = () => {
          // 已销毁则返回
          if (disposed) return;
          // 读取前台状态，异常视为后台
          try { state.foreground.set(runtime.foreground() === true); }
          // 读取前后台状态失败则视为后台
          catch { state.foreground.set(false); }
          // 端口就绪后才轮询，失败交给 fail 处理
          if (nativeReady) {
            // 轮询失败时上报
            void poll().catch((error) => fail("poll", error));
          }
          // 安排下一次心跳
          timer = setTimeout(tick, 250);
        };
        // 立即执行第一次心跳
        tick();
      });
      // 副作用销毁时清理所有资源
      cleanup(() => {
        // 标记销毁，阻止后续异步回调
        disposed = true;
        // 取消待执行的心跳
        if (timer != null) clearTimeout(timer);
        // 标记为后台
        state.foreground.set(false);
        // 通知 Rust 释放会话
        void Settings.dispose(session).catch(() => { /* VM 可能已开始关闭。 */ });
        // 告知原生端口界面已不再活跃
        if (nativeStarted) {
          // 通知原生端停用
          try { runtime.link.command({ op: "active", value: false }); }
          // 宿主可能已关闭原生所有者，忽略
          catch { /* 宿主可能已关闭原生 owner。 */ }
        }
      });
    });
  }

  // 编辑名称：立即更新草稿，再让 Rust 校验
  async function editName(draft: string) {
    // 更新名称草稿
    state.nameDraft.set(draft);
    // 请求引擎编辑名称，失败则上报
    try { await update(() => Settings.editName(session, draft)); } catch (error) { await fail("settings", error); }
  }

  // 保存名称：由 Rust 给出命令，运行时只负责投递
  async function saveName() {
    // 已销毁则返回
    if (disposed) return;
    // 请求 Rust 开始保存并取得需要下发的命令
    const result = await update(() => Settings.beginNameSave(session, state.nameDraft(), state.view(), Date.now()));
    // 已销毁则返回
    if (disposed) return;
    // 依次投递命令，失败则通知 Rust 回滚
    try {
      // 逐个派发待执行命令
      for (const value of result.commands) runtime.link.command(value);
    // 派发失败
    } catch {
      // 通知引擎名称派发失败
      await update(() => Settings.nameDispatchFailed(session));
      // 然后返回
      return;
    }
    // 有命令发出时轮询一次以尽快看到确认
    if (result.commands.length) {
      // 轮询最新状态，失败则上报
      try { await poll(); } catch (error) { await fail("poll", error); }
    }
  }

  // 保存面板配置，并下发由 Rust 生成的命令
  async function savePanel(panel: PanelConfig) {
    // 已销毁则返回
    if (disposed) return;
    // 请求引擎保存面板
    const result = await update(() => Settings.savePanel(session, panel));
    // 逐个执行引擎返回的命令
    for (const value of result.commands) await command(value);
  }

  // 对外暴露的操作集合
  return { mount, command, editName, saveName, savePanel };
}
