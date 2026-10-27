import { mount, Text, View, Storage } from "nsh";
import { Presentation } from "loki-metris-mobile-display/native";
import { hookPages } from "../../features/hooks/paging";
import type { HookLayout } from "../../shared/contracts/panel";

type Query = { op: string; args: unknown[] };
type Reply = Query & { value?: unknown; error?: string };

// 原生目标的套件 VM 由 CLI 持有；只有这个显式测试宿主链接真实应用 Rust。
// 输入仅是规则查询，结果来自 Presentation，不接入网络或生产入口。
void (async () => {
  const queries = JSON.parse((await Storage.getItem("test.nativeQueries"))!) as Query[];
  const replies: Reply[] = [];
  for (const query of queries) {
    const args = query.args.map((arg) => {
      if (arg && typeof arg === "object" && Object.keys(arg).length === 1 && "$result" in arg) {
        return replies[(arg as { $result: number }).$result].value;
      }
      return arg;
    });
    try {
      // 页码分格属于现有 UI 布局；直接调用原组件 helper，规则查询仍走 Rust。
      const value = query.op === "ui.hookPages" ? hookPages(args[0] as unknown[], args[1] as HookLayout)
        : JSON.parse(Presentation.call(JSON.stringify({ op: query.op, args })));
      replies.push({ op: query.op, args, value });
    }
    catch (error) { replies.push({ op: query.op, args, error: String(error) }); }
  }
  await Storage.setItem("test.nativeReplies", JSON.stringify(replies));
  mount(() => <View testID="native-logic-ready"><Text>原生规则查询完成</Text></View>);
})();
