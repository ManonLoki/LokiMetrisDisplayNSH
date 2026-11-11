// 引入信号工厂与类型
import { signal, type Signal } from "nsh";
// 引入 Hooks 布局类型
import type { HookLayout } from "../../shared/contracts/panel";

/** 应用装配层持有 Hook 页码；导航切走再回来时保留，不执行 I/O。 */
// 控制器接口
export interface PanelController {
  // 当前 Hooks 页码
  hookPage: Signal<number>;
  // 上一次生效的布局，用于检测布局切换
  layout: HookLayout | null;
}
// 创建控制器，初始为第 0 页、无布局
export function createPanelController(): PanelController {
  // 控制器：页码与布局
  return { hookPage: signal(0), layout: null };
}
