import { invoke } from "../../shared/logic";
import type { Snapshot } from "../../shared/contracts/snapshot";

export function parseSnapshot(value: unknown, images: Record<string, string> = {}): Snapshot | null {
  return invoke("connection.parseSnapshot", value, images);
}
