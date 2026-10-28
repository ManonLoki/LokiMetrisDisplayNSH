// 引入原生桥接调用函数
import { invoke } from "../../shared/native-bridge";
// 引入快照类型
import type { Snapshot } from "../../shared/contracts/snapshot";

// 解析桌面发来的快照；images 为哈希到图片地址的映射，非法数据返回 null
export function parseSnapshot(value: unknown, images: Record<string, string> = {}): Snapshot | null {
  // 解析与校验交给原生侧
  return invoke("connection.parseSnapshot", value, images);
}
