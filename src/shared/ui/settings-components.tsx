/** 偏好页与连接模块共用的表单视觉组件，不读取业务状态或提交系统命令。 */
import { Canvas, Text, View, type JSX, type ViewStyle } from "nsh";
import { CyberArrival } from "./effects";
import { cyberAlpha, cyberColors as color } from "./theme";

// 横向居中排列的行样式
export const row: ViewStyle = { flexDirection: "row", alignItems: "center", gap: 12 };
export const outlineButton: ViewStyle = { minHeight: 40, minWidth: 58, borderWidth: 1, borderColor: color.outline,
  borderRadius: 20, paddingHorizontal: 24, paddingVertical: 8, alignItems: "center", justifyContent: "center", flexShrink: 0 };
export const textButton: ViewStyle = { minHeight: 40, minWidth: 58, justifyContent: "center", alignSelf: "flex-start" };
// 装饰元素的公共属性：不接收事件、对无障碍隐藏
export const decoration = { accessible: false, accessibilityElementsHidden: true,
  // 对子孙元素隐藏读屏
  importantForAccessibility: "no-hide-descendants" as const };
type Symbol = "usage" | "hooks" | "desktop" | "device";

// 设置卡片：入场动效容器，叠加一层背景渐变
export function SettingsCard(props: { order?: number; children?: JSX.Element; testID?: string }) {
  // 卡片：进场顺序与测试标识
  return <CyberArrival order={props.order ?? 0} testID={props.testID}
    // 卡片样式：相对定位、圆角与背景
    style={{ position: "relative", width: "100%", borderRadius: 22, backgroundColor: color.surface,
      borderWidth: 1, borderColor: cyberAlpha(color.cyan, 0.13), padding: 18, gap: 16 }}>
    {/* 背景光层，内缩 1px 以避开边框 */}
    <Canvas {...decoration} style={{ pointerEvents: "none", position: "absolute", top: 1, right: 1, bottom: 1, left: 1 }}
      draw={(cx, { width: w, height: h }) => {
        if (w <= 2 || h <= 2) return;
        const radius = Math.min(21, w / 2, h / 2);
        // 保存状态并裁剪为圆角矩形
        cx.save(); cx.beginPath(); cx.roundRect(0, 0, w, h, radius); cx.clip();
        const surface = cx.createLinearGradient(0, 0, w * 0.35, h);
        surface.addColorStop(0, cyberAlpha(color.surfaceContainer, 0.72));
        surface.addColorStop(1, cyberAlpha(color.surfaceContainer, 0));
        cx.fillStyle = surface; cx.fillRect(0, 0, w, h);
        const edge = cx.createLinearGradient(0, 0, w, h);
        edge.addColorStop(0, cyberAlpha(color.ink, 0.1));
        edge.addColorStop(0.5, cyberAlpha(color.cyan, 0.025));
        edge.addColorStop(1, cyberAlpha(color.violet, 0.065));
        cx.strokeStyle = edge; cx.lineWidth = 1;
        cx.beginPath(); cx.roundRect(0.5, 0.5, w - 1, h - 1, radius); cx.stroke();
        cx.restore();
      }} />
    {/* 卡片内容 */}
    {props.children}
  </CyberArrival>;
}

// 设置分区：标题行加内容
export function SettingsSection(props: { title: string; note?: string; children?: JSX.Element }) {
  return <View style={{ gap: 10 }}>
    {/* 标题行 */}
    <View style={{ flexDirection: "row", alignItems: "center", paddingHorizontal: 4 }}>
      {/* 标题 */}
      <Text style={{ flex: 1, color: color.muted, fontSize: 13, lineHeight: 19, fontWeight: 600 }}>{props.title}</Text>
      {/* 可选的右侧备注 */}
      {props.note ? <Text style={{ color: color.muted, fontSize: 11, lineHeight: 16 }}>{props.note}</Text> : null}
    </View>
    {/* 分区内容 */}
    {props.children}
  </View>;
}

export function SettingsDivider() {
  return <Canvas {...decoration} style={{ pointerEvents: "none", height: 1, width: "100%" }} draw={(cx, { width: w }) => {
    // 宽度为 0 不绘制
    if (w <= 0) return;
    // 横向渐变：青、白、紫
    const edge = cx.createLinearGradient(0, 0, w, 0);
    edge.addColorStop(0, cyberAlpha(color.cyan, 0.14));
    edge.addColorStop(0.55, cyberAlpha(color.ink, 0.07));
    edge.addColorStop(1, cyberAlpha(color.violet, 0.08));
    // 填充 1px 高的线
    cx.fillStyle = edge; cx.fillRect(0, 0, w, 1);
  }} />;
}

// 设置图标：圆角底板加画布图标
export function SettingsIcon(props: { symbol: Symbol }) {
  return <View {...decoration} style={{ pointerEvents: "none", width: 38, height: 38, padding: 10, borderRadius: 11,
    backgroundColor: cyberAlpha(color.cyan, 0.09), flexShrink: 0 }}>
    {/* 图标画布 */}
    <Canvas style={{ width: 18, height: 18 }} draw={(cx, { width: w, height: h }) => {
      const stroke = 1.6;
      cx.strokeStyle = color.cyan; cx.fillStyle = color.cyan; cx.lineWidth = stroke;
      if (props.symbol === "usage") {
        // 柱用粗线、圆角端点
        cx.lineWidth = stroke * 1.8; cx.lineCap = "round";
        for (let i = 0; i <= 2; i++) {
          const x = w * (0.16 + i * 0.34);
          cx.beginPath(); cx.moveTo(x, h * (0.58 - i * 0.22)); cx.lineTo(x, h * 0.9); cx.stroke();
        }
      // Hooks：2x2 方格
      } else if (props.symbol === "hooks") {
        const gap = 3, cell = (w - gap) / 2;
        for (let x = 0; x < 2; x++) for (let y = 0; y < 2; y++) {
          cx.beginPath(); cx.roundRect(x * (cell + gap), y * (cell + gap), cell, cell, 1.5); cx.stroke();
        }
      // 桌面：显示器轮廓加支架
      } else if (props.symbol === "desktop") {
        cx.beginPath(); cx.roundRect(0, 0, w, h * 0.7, 2); cx.stroke();
        cx.beginPath(); cx.moveTo(w * 0.5, h * 0.7); cx.lineTo(w * 0.5, h); cx.stroke();
        cx.lineCap = "round"; cx.beginPath(); cx.moveTo(w * 0.25, h); cx.lineTo(w * 0.75, h); cx.stroke();
      // 设备：手机轮廓加底部横条
      } else {
        cx.beginPath(); cx.roundRect(w * 0.18, 0, w * 0.64, h, 2.5); cx.stroke();
        // 设置圆头线端并画横线
        cx.lineCap = "round"; cx.beginPath(); cx.moveTo(w * 0.4, h * 0.82); cx.lineTo(w * 0.6, h * 0.82); cx.stroke();
      }
    }} />
  </View>;
}
