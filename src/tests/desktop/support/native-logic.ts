import { open } from "../app-environment";

export type NativeQuery = { op: string; args: unknown[] };
type NativeReply = NativeQuery & { value?: unknown; error?: string };

export function query(op: string, ...args: unknown[]): NativeQuery { return { op, args }; }

function key(op: string, args: unknown[]): string {
  // JSON 对象键序不属于业务输入；不同 Rust/JS 序列化顺序应命中同一次查询。
  return JSON.stringify([op, ...args], (_, value) => value && typeof value === "object" && !Array.isArray(value)
    ? Object.fromEntries(Object.keys(value).sort().map((name) => [name, value[name]])) : value);
}

/** 同一批规则在 open 选择的真实宿主执行，套件只断言结果；headless 与桌面共用。 */
export async function nativeFunctions(queries: NativeQuery[]) {
  const app = await open("./NativeLogicApp.tsx", { storage: { "test.nativeQueries": JSON.stringify(queries) } });
  try {
    await app.waitFor("native-logic-ready", { timeout: 15_000 });
    const replies = JSON.parse(app.storage()["test.nativeReplies"]) as NativeReply[];
    if (replies.length !== queries.length) throw new Error("Native query batch is incomplete");
    const values = new Map(replies.map((reply) => [key(reply.op, reply.args), reply]));
    return <T>(op: string, ...args: unknown[]): T => {
      const reply = values.get(key(op, args));
      // A missing query cannot accidentally satisfy a test expecting a native exception.
      if (!reply) return undefined as T;
      if (reply.error != null) throw new Error(reply.error);
      return reply.value as T;
    };
  } finally { app.dispose(); }
}
