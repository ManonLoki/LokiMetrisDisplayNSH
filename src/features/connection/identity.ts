// 引入原生规则模块，校验逻辑由 Rust 实现
import { Rules } from "loki-metris-mobile-display/native";

// 校验设备名称，返回错误文案；合法时返回 null
export function nameError(name: string): string | null {
  // 委托给原生侧的名称规则
  return Rules.connectionNameError(name);
}

// 判断设备 ID 是否为规范格式
export function validDeviceId(value: string): boolean {
  // 委托给原生侧的 ID 规则
  return Rules.connectionValidDeviceId(value);
}
