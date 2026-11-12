import { View, cleanup, effect, keymap, signal, untracked, type Accessor, type JSX, type StyleProp, type ViewStyle, type ViewRef, type ScrollRef } from "nsh";
import { cyberColors as color } from "./theme";

/** 遥控器（D-pad）焦点：电视没有触摸，方向键按几何位置移动焦点，确认键执行当前目标的动作。
 * 只有宿主声明电视形态时启用；手机、平板与桌面的触摸和指针交互不受影响。 */
interface Target {
  // 取得节点用于测量位置
  node: () => ViewRef | undefined;
  activate?: () => void;
  disabled: () => boolean;
  /** 获得遥控焦点时调用：例如把横向分页滚到该目标所在页。 */
  focused?: () => void;
}

// 目标在页面上的位置
interface Placed {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

type Direction = "up" | "down" | "left" | "right";

const targets = new Map<string, Target>();
// 当前获得遥控焦点的目标
const focused = signal<string | null>(null);
const enabled = signal(false);
// 页面主滚动容器及其当前偏移与可视高度
let scroller: { node: () => ScrollRef | undefined; offset: number; viewport: number } | null = null;

/** 遥控导航是否启用（电视）。 */
// 对外暴露：遥控导航是否启用
export const remoteActive: Accessor<boolean> = () => enabled();

/** 当前遥控焦点所在目标的 testID；未启用时恒为 null。 */
// 对外暴露：当前焦点目标，未启用时为 null
export const remoteFocus: Accessor<string | null> = () => (enabled() ? focused() : null);

/** 可聚焦目标登记自身；组件卸载时自动移除。 */
// 登记目标，卸载时自动清理
export function registerRemoteTarget(id: string, target: Target) {
  targets.set(id, target);
  // 卸载时移除目标，并在其持有焦点时清除焦点
  cleanup(() => {
    // 仍是同一目标才注销
    if (targets.get(id) === target) targets.delete(id);
    // 当前焦点正是它则清除焦点
    if (untracked(focused) === id) focused.set(null);
  });
}

/** 页面的主滚动容器：焦点移出可见区域时滚动到它。 */
// 登记主滚动容器，返回同步滚动状态的回调
export function registerRemoteScroller(node: () => ScrollRef | undefined) {
  const entry = { node, offset: 0, viewport: 0 };
  scroller = entry;
  // 卸载时若仍是当前容器则清除
  cleanup(() => { if (scroller === entry) scroller = null; });
  // 返回更新函数：offset 为 null 表示仅更新可视高度
  return (offset: number | null, viewport: number) => {
    if (offset != null) entry.offset = offset;
    entry.viewport = viewport;
  };
}

/** 按宿主的电视形态开关遥控导航，并绑定方向键与确认键。 */
// 启用或关闭遥控导航，并绑定方向键与确认键
export function enableRemoteNavigation(active: Accessor<boolean>) {
  // 随电视形态开关变化
  effect(() => {
    enabled.set(active());
    // 未启用则不绑定按键
    if (!active()) return;
    // 绑定四个方向键与确认键，输入框编辑时不拦截
    const remove = keymap.bind({
      up: () => void move("up"),
      down: () => void move("down"),
      left: () => void move("left"),
      right: () => void move("right"),
      // 确认键执行当前焦点目标
      enter: () => activate(),
    }, { when: "notEditing" });
    // 副作用销毁时解除绑定
    cleanup(remove);
  });
}

// 执行当前焦点目标的动作（目标未禁用时）
function activate() {
  const id = focused();
  const target = id ? targets.get(id) : undefined;
  if (target && !target.disabled()) target.activate?.();
}

// 测量所有未禁用目标的位置
async function measured(): Promise<Placed[]> {
  const entries = [...targets.entries()].filter(([, t]) => !t.disabled());
  const placed = await Promise.all(entries.map(async ([id, t]) => {
    const node = t.node();
    const m = node ? await node.measure() : null;
    // 无法测量或尺寸为零的目标忽略
    if (!m || m.bounds.size.width <= 0 || m.bounds.size.height <= 0) return null;
    // 返回标识与几何信息
    return { id, x: m.pagePosition.x, y: m.pagePosition.y, width: m.bounds.size.width, height: m.bounds.size.height };
  }));
  return placed.filter((p): p is Placed => p != null);
}

/** 与 Android FocusFinder 相同的取舍：目标须整体位于当前元素该方向的边缘之外；
 * 与当前元素在垂直方向（左右移动时）或水平方向（上下移动时）有重叠的优先，
 * 否则按主轴边缘距离 + 2 × 副轴中心距离选最近，使同一行的远处目标胜过下方近处的目标。
 * 没有完全越过边缘的目标时退回按中心点判断。 */
// 选出指定方向上最合适的目标
export function nearest(from: Placed | undefined, all: Placed[], direction: Direction): Placed | undefined {
  // 当前无焦点：选页面最靠上、最靠左的目标
  if (!from) return [...all].sort((a, b) => a.y - b.y || a.x - b.x)[0];
  const horizontal = direction === "left" || direction === "right";
  // 计算矩形四边与中心点
  const edge = (p: Placed) => ({ l: p.x, t: p.y, r: p.x + p.width, b: p.y + p.height,
    cx: p.x + p.width / 2, cy: p.y + p.height / 2 });
  // 当前目标的几何信息
  const s = edge(from);
  const others = all.filter((p) => p.id !== from.id);
  // 候选整体越过当前目标该方向的边缘
  const beyond = (d: ReturnType<typeof edge>) => direction === "right" ? d.l >= s.r - 1
    : direction === "left" ? d.r <= s.l + 1 : direction === "down" ? d.t >= s.b - 1 : d.b <= s.t + 1;
  // 候选中心点位于该方向前方
  const ahead = (d: ReturnType<typeof edge>) => direction === "right" ? d.cx > s.cx
    : direction === "left" ? d.cx < s.cx : direction === "down" ? d.cy > s.cy : d.cy < s.cy;
  // 优先整体越过边缘的候选，没有则退回中心点判断
  let pool = others.filter((p) => beyond(edge(p)));
  // 前方没有候选则放宽条件
  if (pool.length === 0) pool = others.filter((p) => ahead(edge(p)));
  // 计算候选的重叠情况与得分
  const measure = (p: Placed) => {
    const d = edge(p);
    const major = Math.max(0, direction === "right" ? d.l - s.r : direction === "left" ? s.l - d.r
      : direction === "down" ? d.t - s.b : s.t - d.b);
    const minor = horizontal ? Math.abs(d.cy - s.cy) : Math.abs(d.cx - s.cx);
    // 副轴方向是否与当前目标重叠
    const overlap = horizontal ? d.t < s.b && d.b > s.t : d.l < s.r && d.r > s.l;
    // 得分 = 主轴距离 + 2 × 副轴距离
    return { overlap, score: major + minor * 2 };
  };
  let best: Placed | undefined;
  // 排序键：先看是否重叠，再看得分
  let bestKey: [number, number] = [2, Infinity];
  for (const p of pool) {
    const m = measure(p);
    // 排序键：先看是否重叠，再看得分
    const key: [number, number] = [m.overlap ? 0 : 1, m.score];
    if (key[0] < bestKey[0] || (key[0] === bestKey[0] && key[1] < bestKey[1])) { bestKey = key; best = p; }
  }
  return best;
}

async function move(direction: Direction) {
  const all = await measured();
  const current = all.find((p) => p.id === focused());
  const next = nearest(current, all, direction);
  // 没有可去处则保持原位
  if (!next) return;
  focused.set(next.id);
  targets.get(next.id)?.focused?.();
  // 必要时滚动使目标可见
  void reveal(next);
}

// 让目标进入可视区域
async function reveal(target: Placed) {
  const view = scroller;
  const node = view?.node();
  // 没有滚动容器或高度未知则不处理
  if (!view || !node || view.viewport <= 0) return;
  // 测量滚动容器自身位置
  const frame = await node.measure();
  const top = frame?.pagePosition.y ?? 0;
  // 目标相对滚动容器顶部的位置
  const y = target.y - top;
  const margin = 24;
  // 目标超出底部：向下滚动
  if (y + target.height > view.viewport - margin) {
    // 目标在可视区之下：向下滚动
    node.scrollTo({ y: view.offset + y + target.height - view.viewport + margin, animated: true });
  // 目标高于顶部：向上滚动
  } else if (y < margin) {
    // 目标在可视区之上：向上滚动
    node.scrollTo({ y: Math.max(0, view.offset + y - margin), animated: true });
  }
}

/** 遥控焦点框：不参与布局与触摸，只在目标获得焦点时绘制。 */
// 遥控焦点框：仅在目标获得焦点时显示，不参与布局和触摸
export function RemoteRing(props: { id: string | undefined; radius?: number }) {
  return <>{() => props.id && remoteFocus() === props.id ? <View testID={`remote-focus-${props.id}`} style={{ pointerEvents: "none", position: "absolute",
    top: -3, left: -3, right: -3, bottom: -3, borderRadius: (props.radius ?? 13) + 3, borderWidth: 2,
    borderColor: color.cyan }} /> : null}</>;
}

/** 包住原生开关、单选等非 CyberPressable 的可按元素，使其可被遥控选中并确认。 */
// 为开关、单选等非 CyberPressable 元素包一层遥控目标
export function RemoteTarget(props: { id: string; activate: () => void; radius?: number;
  style?: StyleProp<ViewStyle>; children: JSX.Element }) {
  let node: ViewRef | undefined;
  registerRemoteTarget(props.id, { node: () => node, activate: props.activate, disabled: () => false });
  return <View ref={(value: ViewRef) => { node = value; }} style={props.style}>
    {/* 被包裹的元素 */}
    {props.children}
    {/* 焦点框 */}
    <RemoteRing id={props.id} radius={props.radius} />
  </View>;
}
