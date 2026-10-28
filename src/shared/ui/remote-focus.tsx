// 引入 nsh 的视图、响应式原语、键位映射与类型
import { View, cleanup, effect, keymap, signal, untracked, type Accessor, type JSX, type StyleProp, type ViewStyle, type YNode } from "nsh";
// 引入色板
import { cyberColors as color } from "./theme";

/** 遥控器（D-pad）焦点：电视没有触摸，方向键在可按目标之间按几何位置移动，确认键执行。
 * 只有宿主声明电视形态时启用；手机、平板与桌面的触摸和指针交互不受影响。 */
// 遥控目标
interface Target {
  // 取得节点用于测量位置
  node: () => YNode | undefined;
  // 确认键触发的动作
  activate: () => void;
  // 是否禁用
  disabled: () => boolean;
  /** 获得遥控焦点时调用：例如把横向分页滚到该目标所在页。 */
  // 获得焦点的回调
  focused?: () => void;
}

// 目标在页面上的位置
interface Placed {
  // 目标标识
  id: string;
  // 左上角横坐标
  x: number;
  // 左上角纵坐标
  y: number;
  // 宽度
  width: number;
  // 高度
  height: number;
}

// 方向键方向
type Direction = "up" | "down" | "left" | "right";

// 已登记的目标表
const targets = new Map<string, Target>();
// 当前获得遥控焦点的目标
const focused = signal<string | null>(null);
// 遥控导航是否启用
const enabled = signal(false);
// 页面主滚动容器及其当前偏移与可视高度
let scroller: { node: () => YNode | undefined; offset: number; viewport: number } | null = null;

/** 遥控导航是否启用（电视）。 */
// 对外暴露：遥控导航是否启用
export const remoteActive: Accessor<boolean> = () => enabled();

/** 当前遥控焦点所在目标的 testID；未启用时恒为 null。 */
// 对外暴露：当前焦点目标，未启用时为 null
export const remoteFocus: Accessor<string | null> = () => (enabled() ? focused() : null);

/** 可按目标登记自身；组件卸载时自动移除。 */
// 登记目标，卸载时自动清理
export function registerRemoteTarget(id: string, target: Target) {
  // 登记目标
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
export function registerRemoteScroller(node: () => YNode | undefined) {
  // 滚动容器的记录
  const entry = { node, offset: 0, viewport: 0 };
  // 登记滚动容器
  scroller = entry;
  // 卸载时若仍是当前容器则清除
  cleanup(() => { if (scroller === entry) scroller = null; });
  // 返回更新函数：offset 为 null 表示仅更新可视高度
  return (offset: number | null, viewport: number) => {
    // 有偏移则更新
    if (offset != null) entry.offset = offset;
    // 更新可视高度
    entry.viewport = viewport;
  };
}

/** 按宿主的电视形态开关遥控导航，并绑定方向键与确认键。 */
// 启用或关闭遥控导航，并绑定方向键与确认键
export function enableRemoteNavigation(active: Accessor<boolean>) {
  // 随电视形态开关变化
  effect(() => {
    // 同步启用状态
    enabled.set(active());
    // 未启用则不绑定按键
    if (!active()) return;
    // 绑定四个方向键与确认键，输入框编辑时不拦截
    const remove = keymap.bind({
      // 方向键向上
      up: () => void move("up"),
      // 向下移动
      down: () => void move("down"),
      // 向左移动
      left: () => void move("left"),
      // 向右移动
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
  // 当前焦点标识
  const id = focused();
  // 对应的目标
  const target = id ? targets.get(id) : undefined;
  // 目标可用才触发
  if (target && !target.disabled()) target.activate();
}

// 测量所有未禁用目标的位置
async function measured(): Promise<Placed[]> {
  // 可用的目标
  const entries = [...targets.entries()].filter(([, t]) => !t.disabled());
  // 并发测量
  const placed = await Promise.all(entries.map(async ([id, t]) => {
    // 取目标节点
    const node = t.node();
    // 测量位置与大小
    const m = node ? await node.measure() : null;
    // 无法测量或尺寸为零的目标忽略
    if (!m || m.bounds.size.width <= 0 || m.bounds.size.height <= 0) return null;
    // 返回标识与几何信息
    return { id, x: m.pagePosition.x, y: m.pagePosition.y, width: m.bounds.size.width, height: m.bounds.size.height };
  }));
  // 去掉无效结果
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
  // 是否为左右移动
  const horizontal = direction === "left" || direction === "right";
  // 计算矩形四边与中心点
  const edge = (p: Placed) => ({ l: p.x, t: p.y, r: p.x + p.width, b: p.y + p.height,
    // 中心点坐标
    cx: p.x + p.width / 2, cy: p.y + p.height / 2 });
  // 当前目标的几何信息
  const s = edge(from);
  // 候选目标不含自身
  const others = all.filter((p) => p.id !== from.id);
  // 候选整体越过当前目标该方向的边缘
  const beyond = (d: ReturnType<typeof edge>) => direction === "right" ? d.l >= s.r - 1
    // 向左与向下的判断
    : direction === "left" ? d.r <= s.l + 1 : direction === "down" ? d.t >= s.b - 1 : d.b <= s.t + 1;
  // 候选中心点位于该方向前方
  const ahead = (d: ReturnType<typeof edge>) => direction === "right" ? d.cx > s.cx
    // 向左与向下的判断
    : direction === "left" ? d.cx < s.cx : direction === "down" ? d.cy > s.cy : d.cy < s.cy;
  // 优先整体越过边缘的候选，没有则退回中心点判断
  let pool = others.filter((p) => beyond(edge(p)));
  // 前方没有候选则放宽条件
  if (pool.length === 0) pool = others.filter((p) => ahead(edge(p)));
  // 计算候选的重叠情况与得分
  const measure = (p: Placed) => {
    // 候选到边缘的距离
    const d = edge(p);
    // 主轴上的边缘距离
    const major = Math.max(0, direction === "right" ? d.l - s.r : direction === "left" ? s.l - d.r
      // 向下与向上的距离
      : direction === "down" ? d.t - s.b : s.t - d.b);
    // 副轴上的中心距离
    const minor = horizontal ? Math.abs(d.cy - s.cy) : Math.abs(d.cx - s.cx);
    // 副轴方向是否与当前目标重叠
    const overlap = horizontal ? d.t < s.b && d.b > s.t : d.l < s.r && d.r > s.l;
    // 得分 = 主轴距离 + 2 × 副轴距离
    return { overlap, score: major + minor * 2 };
  };
  // 当前最优候选
  let best: Placed | undefined;
  // 排序键：先看是否重叠，再看得分
  let bestKey: [number, number] = [2, Infinity];
  // 遍历候选取最优
  for (const p of pool) {
    // 测量候选
    const m = measure(p);
    // 排序键：先看是否重叠，再看得分
    const key: [number, number] = [m.overlap ? 0 : 1, m.score];
    // 键更小则替换
    if (key[0] < bestKey[0] || (key[0] === bestKey[0] && key[1] < bestKey[1])) { bestKey = key; best = p; }
  }
  // 返回最佳候选
  return best;
}

// 按方向移动焦点
async function move(direction: Direction) {
  // 测量所有目标
  const all = await measured();
  // 当前焦点目标
  const current = all.find((p) => p.id === focused());
  // 选出下一个目标
  const next = nearest(current, all, direction);
  // 没有可去处则保持原位
  if (!next) return;
  // 更新焦点
  focused.set(next.id);
  // 通知目标获得焦点
  targets.get(next.id)?.focused?.();
  // 必要时滚动使目标可见
  void reveal(next);
}

// 让目标进入可视区域
async function reveal(target: Placed) {
  // 当前滚动容器
  const view = scroller;
  // 滚动容器的节点
  const node = view?.node();
  // 没有滚动容器或高度未知则不处理
  if (!view || !node || view.viewport <= 0) return;
  // 测量滚动容器自身位置
  const frame = await node.measure();
  // 目标顶部位置
  const top = frame?.pagePosition.y ?? 0;
  // 目标相对滚动容器顶部的位置
  const y = target.y - top;
  // 预留边距
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
  // 焦点高亮层
  return <>{() => props.id && remoteFocus() === props.id ? <View testID={`remote-focus-${props.id}`} pointerEvents="none" style={{ position: "absolute",
    // 位置与圆角
    top: -3, left: -3, right: -3, bottom: -3, borderRadius: (props.radius ?? 13) + 3, borderWidth: 2,
    // 边框颜色
    borderColor: color.cyan }} /> : null}</>;
}

/** 包住原生开关、单选等非 CyberPressable 的可按元素，使其可被遥控选中并确认。 */
// 为开关、单选等非 CyberPressable 元素包一层遥控目标
export function RemoteTarget(props: { id: string; activate: () => void; radius?: number;
  // 样式与子节点
  style?: StyleProp<ViewStyle>; children: JSX.Element }) {
  // 节点引用
  let node: YNode | undefined;
  // 登记为遥控目标
  registerRemoteTarget(props.id, { node: () => node, activate: props.activate, disabled: () => false });
  // 容器节点
  return <View ref={(value: YNode) => { node = value; }} style={props.style}>
    {/* 被包裹的元素 */}
    {props.children}
    {/* 焦点框 */}
    <RemoteRing id={props.id} radius={props.radius} />
  </View>;
}
