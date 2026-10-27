import { invoke } from "../../shared/native-bridge";

export function nameError(name: string): string | null {
  return invoke("connection.nameError", name);
}

export function validDeviceId(value: string): boolean {
  return invoke("connection.validDeviceId", value);
}
