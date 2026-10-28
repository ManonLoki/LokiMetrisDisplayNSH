// 引入 nsh 的画布、文本、视图组件以及信号读取器类型
import { Canvas, Text, View, type Accessor } from "nsh";
// 引入带焦点与按压效果的可点击容器
import { CyberPressable } from "../shared/ui/effects";
// 引入主题色板并简写为 color
import { cyberColors as color } from "../shared/ui/theme";

// 应用的两个页面：展示台与设置
export type DisplayPage = "panel" | "settings";

/** 源 App.kt 绘制这些图标;不使用图标字体,也不使用替代图片。 */
// 菜单图标：用画布绘制，选中与否决定着色
export function MenuIcon(props: { page: DisplayPage; selected: boolean }) {
  // 返回 21x21 的画布，draw 回调拿到绘图上下文与实际尺寸
  return <Canvas accessible={false} style={{ width: 21, height: 21 }} draw={(cx, { width: w, height: h }) => {
    // 选中用高亮色，未选中用弱化色
    const tint = props.selected ? color.onSecondaryContainer : color.muted;
    // 填充、描边同色，线宽 1.8，端点圆角
    cx.fillStyle = tint; cx.strokeStyle = tint; cx.lineWidth = 1.8; cx.lineCap = "round";
    // 展示台图标：2x2 圆角方块网格
    if (props.page === "panel") {
      // 方块间距与单个方块边长
      const gap = 3.5, cell = (w - gap) / 2;
      // 依次绘制四个方块
      for (let x = 0; x < 2; x++) for (let y = 0; y < 2; y++) {
        // 圆角半径 2 的实心方块
        cx.beginPath(); cx.roundRect(x * (cell + gap), y * (cell + gap), cell, cell, 2); cx.fill();
      }
    // 否则分支
    } else {
      // 设置图标：三条横线，每条带一个滑块圆点
      for (let row = 0; row < 3; row++) {
        // 行的纵坐标；中间一行的滑块偏右，其余偏左
        const y = h * (0.2 + row * 0.3), x = w * (row === 1 ? 0.68 : 0.32);
        // 画横线
        cx.beginPath(); cx.moveTo(0, y); cx.lineTo(w, y); cx.stroke();
        // 画滑块圆点
        cx.beginPath(); cx.arc(x, y, 3, 0, Math.PI * 2); cx.fill();
      }
    }
  // 绘制回调结束
  }} />;
}

// 品牌标志：青色圆角底板上画一个直角折线加圆点
export function BrandMark() {
  // 38x38 的圆角底板，内边距 10 给画布留出空间
  return <View accessible={false} style={{ width: 38, height: 38, borderRadius: 10, backgroundColor: color.cyan, padding: 10 }}>
    <Canvas style={{ flex: 1 }} draw={(cx, { width: w, height: h }) => {
      // 使用底板上的前景色，线宽 3，圆角端点
      cx.strokeStyle = color.onPrimary; cx.fillStyle = color.onPrimary; cx.lineWidth = 3; cx.lineCap = "round";
      // 画 L 形折线：自左上向下再向右
      cx.beginPath(); cx.moveTo(0, 0); cx.lineTo(0, h); cx.lineTo(w, h); cx.stroke();
      // 在右上角画一个实心圆点
      cx.beginPath(); cx.arc(w, 2.5, 2.5, 0, Math.PI * 2); cx.fill();
    // 绘制回调结束
    }} />
  </View>;
}

// 导航栏：side 为竖排侧栏，bottom 为底部横排标签栏
export function CyberNavigation(props: { placement: "side" | "bottom"; selected: Accessor<DisplayPage>; onSelect: (page: DisplayPage) => void }) {
  // 是否为侧边布局
  const side = props.placement === "side";
  // 容器样式随位置切换：侧栏固定宽度，底栏固定高度并带半透明底色
  return <View testID={side ? "nav-side" : "nav-bottom"} style={side
    // 侧边布局的宽高与对齐
    ? { width: 104, height: "100%", alignItems: "center", paddingTop: 16 }
    // 否则使用底部横向布局的样式
    : { width: "100%", height: 68, flexDirection: "row", backgroundColor: "#111D2DF7", alignItems: "center" }}>
    {/* 仅侧栏顶部显示品牌标志 */}
    {side ? <View style={{ marginBottom: 30 }}><BrandMark /></View> : null}
    {/* 逐页生成标签；返回函数使选中态可随信号更新 */}
    {(["panel", "settings"] as const).map((page) => () => {
      // 当前页是否被选中
      const selected = props.selected() === page;
      // 菜单项：当前页、角色与点击回调
      return <CyberPressable testID={`menu-${page}`} selected={selected} role="tab" onPress={() => props.onSelect(page)}
        // 样式：侧边或底部布局
        style={{ ...(side ? { width: 80, height: 68, marginBottom: 12 } : { flex: 1, height: 68 }),
          // 居中对齐与间距
          alignItems: "center", justifyContent: "center", gap: 4, borderRadius: 16 }}>
        {/* 图标外的胶囊，选中时显示容器底色 */}
        <View style={{ width: side ? 56 : 64, height: 32, borderRadius: 16,
          // 居中与背景色
          alignItems: "center", justifyContent: "center", backgroundColor: selected ? color.primaryContainer : "transparent" }}>
          <MenuIcon page={page} selected={selected} />
        </View>
        {/* 标签文字：展示台或设置 */}
        <Text style={{ color: selected ? color.violet : color.muted, fontSize: side ? 12 : 11, lineHeight: 16,
          // 字重
          fontWeight: "500" }}>{page === "panel" ? "展示台" : "设置"}</Text>
      </CyberPressable>;
    })}
  </View>;
}
