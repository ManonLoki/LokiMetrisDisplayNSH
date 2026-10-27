import { View, cleanup, effect, keymap, signal, untracked, type Accessor, type JSX, type StyleProp, type ViewStyle, type YNode } from "nsh";
import { cyberColors as color } from "./theme";

/** 遥控器（D-pad）焦点：电视没有触摸，方向键在可按目标之间按几何位置移动，确认键执行。
 * 只有宿主声明电视形态时启用；手机、平板与桌面的触摸和指针交互不受影响。 */
interface Target {
  node: () => YNode | undefined;
  activate: () => void;
  disabled: () => boolean;
  /** 获得遥控焦点时调用：例如把横向分页滚到该目标所在页。 */
  focused?: () => void;
}

interface Placed {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

type Direction = "up" | "down" | "left" | "right";

const targets = new Map<string, Target>();
const focused = signal<string | null>(null);
const enabled = signal(false);
let scroller: { node: () => YNode | undefined; offset: number; viewport: number } | null = null;

/** 遥控导航是否启用（电视）。 */
export const remoteActive: Accessor<boolean> = () => enabled();

/** 当前遥控焦点所在目标的 testID；未启用时恒为 null。 */
export const remoteFocus: Accessor<string | null> = () => (enabled() ? focused() : null);

/** 可按目标登记自身；组件卸载时自动移除。 */
export function registerRemoteTarget(id: string, target: Target) {
  targets.set(id, target);
  cleanup(() => {
    if (targets.get(id) === target) targets.delete(id);
    if (untracked(focused) === id) focused.set(null);
  });
}

/** 页面的主滚动容器：焦点移出可见区域时滚动到它。 */
export function registerRemoteScroller(node: () => YNode | undefined) {
  const entry = { node, offset: 0, viewport: 0 };
  scroller = entry;
  cleanup(() => { if (scroller === entry) scroller = null; });
  return (offset: number | null, viewport: number) => {
    if (offset != null) entry.offset = offset;
    entry.viewport = viewport;
  };
}

/** 按宿主的电视形态开关遥控导航，并绑定方向键与确认键。 */
export function enableRemoteNavigation(active: Accessor<boolean>) {
  effect(() => {
    enabled.set(active());
    if (!active()) return;
    const remove = keymap.bind({
      up: () => void move("up"),
      down: () => void move("down"),
      left: () => void move("left"),
      right: () => void move("right"),
      enter: () => activate(),
    }, { when: "notEditing" });
    cleanup(remove);
  });
}

function activate() {
  const id = focused();
  const target = id ? targets.get(id) : undefined;
  if (target && !target.disabled()) target.activate();
}

async function measured(): Promise<Placed[]> {
  const entries = [...targets.entries()].filter(([, t]) => !t.disabled());
  const placed = await Promise.all(entries.map(async ([id, t]) => {
    const node = t.node();
    const m = node ? await node.measure() : null;
    if (!m || m.bounds.size.width <= 0 || m.bounds.size.height <= 0) return null;
    return { id, x: m.pagePosition.x, y: m.pagePosition.y, width: m.bounds.size.width, height: m.bounds.size.height };
  }));
  return placed.filter((p): p is Placed => p != null);
}

/** 与 Android FocusFinder 相同的取舍：目标须整体位于当前元素该方向的边缘之外；
 * 与当前元素在垂直方向（左右移动时）或水平方向（上下移动时）有重叠的优先，
 * 否则按主轴边缘距离 + 2 × 副轴中心距离选最近，使同一行的远处目标胜过下方近处的目标。
 * 没有完全越过边缘的目标时退回按中心点判断。 */
export function nearest(from: Placed | undefined, all: Placed[], direction: Direction): Placed | undefined {
  if (!from) return [...all].sort((a, b) => a.y - b.y || a.x - b.x)[0];
  const horizontal = direction === "left" || direction === "right";
  const edge = (p: Placed) => ({ l: p.x, t: p.y, r: p.x + p.width, b: p.y + p.height,
    cx: p.x + p.width / 2, cy: p.y + p.height / 2 });
  const s = edge(from);
  const others = all.filter((p) => p.id !== from.id);
  const beyond = (d: ReturnType<typeof edge>) => direction === "right" ? d.l >= s.r - 1
    : direction === "left" ? d.r <= s.l + 1 : direction === "down" ? d.t >= s.b - 1 : d.b <= s.t + 1;
  const ahead = (d: ReturnType<typeof edge>) => direction === "right" ? d.cx > s.cx
    : direction === "left" ? d.cx < s.cx : direction === "down" ? d.cy > s.cy : d.cy < s.cy;
  let pool = others.filter((p) => beyond(edge(p)));
  if (pool.length === 0) pool = others.filter((p) => ahead(edge(p)));
  const measure = (p: Placed) => {
    const d = edge(p);
    const major = Math.max(0, direction === "right" ? d.l - s.r : direction === "left" ? s.l - d.r
      : direction === "down" ? d.t - s.b : s.t - d.b);
    const minor = horizontal ? Math.abs(d.cy - s.cy) : Math.abs(d.cx - s.cx);
    const overlap = horizontal ? d.t < s.b && d.b > s.t : d.l < s.r && d.r > s.l;
    return { overlap, score: major + minor * 2 };
  };
  let best: Placed | undefined;
  let bestKey: [number, number] = [2, Infinity];
  for (const p of pool) {
    const m = measure(p);
    const key: [number, number] = [m.overlap ? 0 : 1, m.score];
    if (key[0] < bestKey[0] || (key[0] === bestKey[0] && key[1] < bestKey[1])) { bestKey = key; best = p; }
  }
  return best;
}

async function move(direction: Direction) {
  const all = await measured();
  const current = all.find((p) => p.id === focused());
  const next = nearest(current, all, direction);
  if (!next) return;
  focused.set(next.id);
  targets.get(next.id)?.focused?.();
  void reveal(next);
}

async function reveal(target: Placed) {
  const view = scroller;
  const node = view?.node();
  if (!view || !node || view.viewport <= 0) return;
  const frame = await node.measure();
  const top = frame?.pagePosition.y ?? 0;
  const y = target.y - top;
  const margin = 24;
  if (y + target.height > view.viewport - margin) {
    node.scrollTo({ y: view.offset + y + target.height - view.viewport + margin, animated: true });
  } else if (y < margin) {
    node.scrollTo({ y: Math.max(0, view.offset + y - margin), animated: true });
  }
}

/** 遥控焦点框：不参与布局与触摸，只在目标获得焦点时绘制。 */
export function RemoteRing(props: { id: string | undefined; radius?: number }) {
  return <>{() => props.id && remoteFocus() === props.id ? <View testID={`remote-focus-${props.id}`} pointerEvents="none" style={{ position: "absolute",
    top: -3, left: -3, right: -3, bottom: -3, borderRadius: (props.radius ?? 13) + 3, borderWidth: 2,
    borderColor: color.cyan }} /> : null}</>;
}

/** 包住原生开关、单选等非 CyberPressable 的可按元素，使其可被遥控选中并确认。 */
export function RemoteTarget(props: { id: string; activate: () => void; radius?: number;
  style?: StyleProp<ViewStyle>; children: JSX.Element }) {
  let node: YNode | undefined;
  registerRemoteTarget(props.id, { node: () => node, activate: props.activate, disabled: () => false });
  return <View ref={(value: YNode) => { node = value; }} style={props.style}>
    {props.children}
    <RemoteRing id={props.id} radius={props.radius} />
  </View>;
}
