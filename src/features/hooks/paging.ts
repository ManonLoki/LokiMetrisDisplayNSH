// 引入布局类型
import type { HookLayout } from "../../shared/contracts/panel";
// 引入原生规则模块
import { Rules } from "loki-metris-mobile-display/native";

// 行为状态对应的展示文案，由原生侧提供
export function hookLabel(behavior: string): string {
  // 调用规则：Hook 标签
  return Rules.hooksLabel(behavior);
}

// 每页容量：1x1 为一个，其余为四个
export function hookPageSize(layout: HookLayout): number {
  // 单列为 1，其余为 4
  return layout === "1x1" ? 1 : 4;
}

// 把 Hook 列表分页，不足一页的格子用 null 占位
export function hookPages<T>(items: T[], layout: HookLayout): (T | null)[][] {
  // 每页格数
  const pageSize = hookPageSize(layout);
  // 页数，至少一页
  const count = items.length <= 0 ? 1 : 1 + Math.floor((items.length - 1) / pageSize);
  // 结果页列表
  const pages: (T | null)[][] = [];
  // 逐页构造
  for (let page = 0; page < count; page += 1) {
    // 当前页的格子
    const cells: (T | null)[] = [];
    // 逐格填充
    for (let cell = 0; cell < pageSize; cell += 1) {
      // 格子对应的全局下标
      const index = page * pageSize + cell;
      // 有数据则放入，否则置空
      cells.push(items.length > 0 && index < items.length ? items[index] : null);
    }
    // 收集当前页
    pages.push(cells);
  }
  // 返回全部页
  return pages;
}
