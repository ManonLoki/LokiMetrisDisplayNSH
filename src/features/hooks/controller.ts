import { signal, type Signal } from "nsh";
import type { HookLayout } from "../../shared/contracts/panel";

/** 应用装配层持有 Hook 页码和详情选择；导航切走再回来时保留，不执行 I/O。 */
export interface PanelController {
  hookPage: Signal<number>;
  selectedSlot: Signal<number | null>;
  layout: HookLayout | null;
}
export function createPanelController(): PanelController {
  return { hookPage: signal(0), selectedSlot: signal<number | null>(null), layout: null };
}
