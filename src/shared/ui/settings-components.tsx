/** 偏好页与连接模块共用的表单视觉组件，不读取业务状态或提交系统命令。 */
// 引入画布、文本、视图及类型
import { Canvas, Text, View, type JSX, type ViewStyle } from "nsh";
// 引入入场动效容器
import { CyberArrival } from "./effects";
// 引入透明度函数与色板
import { cyberAlpha, cyberColors as color } from "./theme";

// 横向居中排列的行样式
export const row: ViewStyle = { flexDirection: "row", alignItems: "center", gap: 12 };
// 描边按钮样式
export const outlineButton: ViewStyle = { minHeight: 40, minWidth: 58, borderWidth: 1, borderColor: color.outline,
  // 圆角与内边距
  borderRadius: 20, paddingHorizontal: 24, paddingVertical: 8, alignItems: "center", justifyContent: "center", flexShrink: 0 };
// 文字按钮样式
export const textButton: ViewStyle = { minHeight: 40, minWidth: 58, justifyContent: "center", alignSelf: "flex-start" };
// 装饰元素的公共属性：不接收事件、对无障碍隐藏
export const decoration = { pointerEvents: "none" as const, accessible: false, accessibilityElementsHidden: true,
  // 对子孙元素隐藏读屏
  importantForAccessibility: "no-hide-descendants" as const };
// 图标种类
type Symbol = "usage" | "hooks" | "desktop" | "device";

// 设置卡片：入场动效容器，叠加一层背景渐变
export function SettingsCard(props: { order?: number; children?: JSX.Element; testID?: string }) {
  // 卡片：进场顺序与测试标识
  return <CyberArrival order={props.order ?? 0} testID={props.testID}
    // 卡片样式：相对定位、圆角与背景
    style={{ position: "relative", width: "100%", borderRadius: 22, backgroundColor: color.surface,
      // 边框与内边距
      borderWidth: 1, borderColor: cyberAlpha(color.cyan, 0.13), padding: 18, gap: 16 }}>
    {/* 背景光层，内缩 1px 以避开边框 */}
    <Canvas {...decoration} style={{ position: "absolute", top: 1, right: 1, bottom: 1, left: 1 }}
      // 自定义绘制回调
      draw={(cx, { width: w, height: h }) => {
        // 尺寸过小不绘制
        if (w <= 2 || h <= 2) return;
        // 圆角不超过半边长
        const radius = Math.min(21, w / 2, h / 2);
        // 保存状态并裁剪为圆角矩形
        cx.save(); cx.beginPath(); cx.roundRect(0, 0, w, h, radius); cx.clip();
        // 底色渐变
        const surface = cx.createLinearGradient(0, 0, w * 0.35, h);
        // 顶部亮色渐变起点
        surface.addColorStop(0, cyberAlpha(color.surfaceContainer, 0.72));
        // 渐变终点（透明）
        surface.addColorStop(1, cyberAlpha(color.surfaceContainer, 0));
        // 填充整个面板
        cx.fillStyle = surface; cx.fillRect(0, 0, w, h);
        // 描边渐变
        const edge = cx.createLinearGradient(0, 0, w, h);
        // 边缘渐变起点
        edge.addColorStop(0, cyberAlpha(color.ink, 0.1));
        // 边缘渐变中点
        edge.addColorStop(0.5, cyberAlpha(color.cyan, 0.025));
        // 边缘渐变终点
        edge.addColorStop(1, cyberAlpha(color.violet, 0.065));
        // 描边
        cx.strokeStyle = edge; cx.lineWidth = 1;
        // 描出圆角边框
        cx.beginPath(); cx.roundRect(0.5, 0.5, w - 1, h - 1, radius); cx.stroke();
        // 还原状态
        cx.restore();
      // 绘制回调结束
      }} />
    {/* 卡片内容 */}
    {props.children}
  </CyberArrival>;
}

// 设置分区：标题行加内容
export function SettingsSection(props: { title: string; note?: string; children?: JSX.Element }) {
  // 纵向间距容器
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

// 渐变分隔线
export function SettingsDivider() {
  // 分隔线画布
  return <Canvas {...decoration} style={{ height: 1, width: "100%" }} draw={(cx, { width: w }) => {
    // 宽度为 0 不绘制
    if (w <= 0) return;
    // 横向渐变：青、白、紫
    const edge = cx.createLinearGradient(0, 0, w, 0);
    // 分隔线渐变起点
    edge.addColorStop(0, cyberAlpha(color.cyan, 0.14));
    // 分隔线渐变中点
    edge.addColorStop(0.55, cyberAlpha(color.ink, 0.07));
    // 分隔线渐变终点
    edge.addColorStop(1, cyberAlpha(color.violet, 0.08));
    // 填充 1px 高的线
    cx.fillStyle = edge; cx.fillRect(0, 0, w, 1);
  // 绘制回调结束
  }} />;
}

// 设置图标：圆角底板加画布图标
export function SettingsIcon(props: { symbol: Symbol }) {
  // 图标容器
  return <View {...decoration} style={{ width: 38, height: 38, padding: 10, borderRadius: 11,
    // 背景色与不收缩
    backgroundColor: cyberAlpha(color.cyan, 0.09), flexShrink: 0 }}>
    {/* 图标画布 */}
    <Canvas style={{ width: 18, height: 18 }} draw={(cx, { width: w, height: h }) => {
      // 基础线宽
      const stroke = 1.6;
      // 统一使用青色
      cx.strokeStyle = color.cyan; cx.fillStyle = color.cyan; cx.lineWidth = stroke;
      // 用量：三根递增柱
      if (props.symbol === "usage") {
        // 柱用粗线、圆角端点
        cx.lineWidth = stroke * 1.8; cx.lineCap = "round";
        // 逐根绘制
        for (let i = 0; i <= 2; i++) {
          // 柱的横坐标
          const x = w * (0.16 + i * 0.34);
          // 画一条竖线
          cx.beginPath(); cx.moveTo(x, h * (0.58 - i * 0.22)); cx.lineTo(x, h * 0.9); cx.stroke();
        }
      // Hooks：2x2 方格
      } else if (props.symbol === "hooks") {
        // 间距与单格边长
        const gap = 3, cell = (w - gap) / 2;
        // 逐格描边
        for (let x = 0; x < 2; x++) for (let y = 0; y < 2; y++) {
          // 画一个圆角格子
          cx.beginPath(); cx.roundRect(x * (cell + gap), y * (cell + gap), cell, cell, 1.5); cx.stroke();
        }
      // 桌面：显示器轮廓加支架
      } else if (props.symbol === "desktop") {
        // 屏幕
        cx.beginPath(); cx.roundRect(0, 0, w, h * 0.7, 2); cx.stroke();
        // 立柱
        cx.beginPath(); cx.moveTo(w * 0.5, h * 0.7); cx.lineTo(w * 0.5, h); cx.stroke();
        // 底座
        cx.lineCap = "round"; cx.beginPath(); cx.moveTo(w * 0.25, h); cx.lineTo(w * 0.75, h); cx.stroke();
      // 设备：手机轮廓加底部横条
      } else {
        // 画图标的圆角矩形
        cx.beginPath(); cx.roundRect(w * 0.18, 0, w * 0.64, h, 2.5); cx.stroke();
        // 设置圆头线端并画横线
        cx.lineCap = "round"; cx.beginPath(); cx.moveTo(w * 0.4, h * 0.82); cx.lineTo(w * 0.6, h * 0.82); cx.stroke();
      }
    // 绘制回调结束
    }} />
  </View>;
}
