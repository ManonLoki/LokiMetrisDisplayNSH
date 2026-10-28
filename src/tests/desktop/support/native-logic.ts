// 引入打开函数
import { open } from "../app-environment";

// 原生规则查询
export type NativeQuery = { op: string; args: unknown[] };
// 查询回复
type NativeReply = NativeQuery & { value?: unknown; error?: string };

// 构造查询
export function query(op: string, ...args: unknown[]): NativeQuery { return { op, args }; }

// 生成查询键：对象键排序，使序列化顺序不影响命中
function key(op: string, args: unknown[]): string {
  // JSON 对象键序不属于业务输入；不同 Rust/JS 序列化顺序应命中同一次查询。
  return JSON.stringify([op, ...args], (_, value) => value && typeof value === "object" && !Array.isArray(value)
    ? Object.fromEntries(Object.keys(value).sort().map((name) => [name, value[name]])) : value);
}

/** 同一批规则在 open 选择的真实宿主执行，套件只断言结果；headless 与桌面共用。 */
// 批量执行规则查询，返回按查询取结果的函数
export async function nativeFunctions(queries: NativeQuery[]) {
  // 打开专用宿主执行查询
  const app = await open("./native-logic-app.tsx", { storage: { "test.nativeQueries": JSON.stringify(queries) } });
  try {
    // 等待宿主完成
    await app.waitFor("native-logic-ready", { timeout: 15_000 });
    // 读取回复
    const replies = JSON.parse(app.storage()["test.nativeReplies"]) as NativeReply[];
    // 批次不完整则报错
    if (replies.length !== queries.length) throw new Error("Native query batch is incomplete");
    // 建立查询键到回复的映射
    const values = new Map(replies.map((reply) => [key(reply.op, reply.args), reply]));
    // 返回取值函数
    return <T>(op: string, ...args: unknown[]): T => {
      // 查找对应回复
      const reply = values.get(key(op, args));
      // 缺失的查询不会意外满足期望原生异常的测试。
      // 没发起过的查询返回 undefined，不会被误当作异常
      if (!reply) return undefined as T;
      // 原生异常原样抛出
      if (reply.error != null) throw new Error(reply.error);
      return reply.value as T;
    };
  // 无论成败都释放宿主
  } finally { app.dispose(); }
}
