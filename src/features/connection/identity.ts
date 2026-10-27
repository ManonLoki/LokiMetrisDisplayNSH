import { invoke } from "../../shared/logic";

export const MAX_NAME_BYTES = invoke<number>("connection.maxNameBytes");
export const ZERO_DEVICE_ID = invoke<string>("connection.zeroDeviceId");

export function nameError(name: string): string | null {
  return invoke("connection.nameError", name);
}

export function utf8Bytes(value: string): number {
  return invoke("connection.utf8Bytes", value);
}

export function validDeviceId(value: string): boolean {
  return invoke("connection.validDeviceId", value);
}

export function commitDeviceName(name: string, deviceId: string): { ok: true; name: string } | { ok: false; error: string } {
  return invoke("connection.commitDeviceName", name, deviceId);
}
