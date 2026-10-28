// 引入挂载函数、文本、视图与存储
import { mount, Text, View, Storage } from "nsh";
// 引入真实原生规则模块
import { Rules } from "loki-metris-mobile-display/native";
// 引入分页函数

// 查询：操作名与参数
type Query = { op: string; args: unknown[] };
// 回复：附带结果或错误
type Reply = Query & { value?: unknown; error?: string };

// 原生目标的套件 VM 由 CLI 持有；只有这个显式测试宿主链接真实应用 Rust。
// 输入仅是规则查询，结果来自 Rules，不接入网络或生产入口。
// 测试查询名 "feature.name" 对应 Rules 的方法：shared 为 name，其余为 featureName
const ruleMethod = (op: string) => op.startsWith("shared.") ? op.slice(7) : op.replace(/\.(.)/, (_, c: string) => c.toUpperCase());

// 读取查询、逐条执行并写回结果
void (async () => {
  // 读取查询批次
  const queries = JSON.parse((await Storage.getItem("test.nativeQueries"))!) as Query[];
  // 收集回复
  const replies: Reply[] = [];
  // 依次执行
  for (const query of queries) {
    // 参数可用 $result 引用前面查询的结果
    const args = query.args.map((arg) => {
      if (arg && typeof arg === "object" && Object.keys(arg).length === 1 && "$result" in arg) {
        // 替换为引用的结果
        return replies[(arg as { $result: number }).$result].value;
      }
      return arg;
    });
    try {
      // 规则查询直接走 Rust 的 Rules 方法
      const value = (Rules as unknown as Record<string, (...rest: unknown[]) => unknown>)[ruleMethod(query.op)](...args);
      // 记录成功结果
      replies.push({ op: query.op, args, value });
    }
    // 记录异常
    catch (error) { replies.push({ op: query.op, args, error: String(error) }); }
  }
  // 全部完成后写回
  await Storage.setItem("test.nativeReplies", JSON.stringify(replies));
  // 挂载就绪标记，测试据此等待
  mount(() => <View testID="native-logic-ready"><Text>原生规则查询完成</Text></View>);
})();
