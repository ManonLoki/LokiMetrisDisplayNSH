// 引入视图类型
import type { ViewState } from "../../../features/connection";
// 引入真实原生规则：测试替身的投影也经过 Rust 的校验
import { FixtureRules } from "loki-metris-mobile-display/native";

/** 把测试替身产生的投影文本交给 Rust 校验，得到与产品 Link 相同的公开视图。 */
export const validatedView = (raw: string): ViewState => FixtureRules.connectionNativeView(raw);

/** 和产品 Link.poll 一样：只在投影文本变化时返回新视图，否则返回 null。 */
export function changedOnly(initial = ""): (raw: string) => ViewState | null {
  let last = initial;
  return (raw) => {
    if (raw === last) return null;
    last = raw;
    return validatedView(raw);
  };
}
