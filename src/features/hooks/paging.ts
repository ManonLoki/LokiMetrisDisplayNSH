import type { HookLayout } from "../../shared/contracts/panel";
import { invoke } from "../../shared/logic";

export function hookLabel(behavior: string): string {
  return invoke("hooks.label", behavior);
}

export function hookPageSize(layout: HookLayout): number {
  return layout === "1x1" ? 1 : 4;
}

export function hookPages<T>(items: T[], layout: HookLayout): (T | null)[][] {
  const pageSize = hookPageSize(layout);
  const count = items.length <= 0 ? 1 : 1 + Math.floor((items.length - 1) / pageSize);
  const pages: (T | null)[][] = [];
  for (let page = 0; page < count; page += 1) {
    const cells: (T | null)[] = [];
    for (let cell = 0; cell < pageSize; cell += 1) {
      const index = page * pageSize + cell;
      cells.push(items.length > 0 && index < items.length ? items[index] : null);
    }
    pages.push(cells);
  }
  return pages;
}
