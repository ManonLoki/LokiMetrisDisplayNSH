// 引入文本、视图与响应式原语
import { Text, View, computed, signal, type Accessor, type Size } from "nsh";
// 引入 Hook 条目类型
import type { HookItem } from "../../shared/contracts/snapshot";
// 引入代理显示名
import { Rules } from "loki-metris-mobile-display/native";
// 引入图片组件
import { CyberImage } from "../../shared/ui/image";
// 引入色板
import { cyberColors as color } from "../../shared/ui/theme";

// Hook 图片视口：显示图片，或在缺失时显示原因
export function HookImageViewport(props: { hook: Accessor<HookItem>; connected: Accessor<boolean>; active?: Accessor<boolean>;
  onImageLoad?: (hash: string, size: Size) => void }) {
  // 记录加载失败的图片地址
  const failedSource = signal<string | null>(null);
  // 当前图片地址
  const source = computed(() => props.hook().imageSource ?? null);
  // 地址存在且未曾加载失败
  const imageReady = computed(() => source() != null && failedSource() !== source());
  // 圆角容器撑满卡片剩余区域
  return <View style={{ flex: 1, minWidth: 0, minHeight: 0,
    // 宽度、圆角与背景
    width: "100%", borderRadius: 12, backgroundColor: color.surfaceContainer, overflow: "hidden", alignItems: "center", justifyContent: "center" }}>
    {/* 图片可用时渲染图片，否则显示说明文字 */}
    {imageReady() ? <CyberImage active={props.active}
      // 测试标识
      testID={`hook-image-${props.hook().slot}`}
      // 图片来源与样式
      src={source()!} style={{ objectFit: "contain", width: "100%", height: "100%" }}
      // 读屏标签
      accessibilityLabel={`${Rules.agentDisplayName(props.hook().agent)} 的 Hook 图片`}
      // 加载成功回调
      onLoad={({ size }) => props.onImageLoad?.(props.hook().image ?? source()!, size)}
      // 加载失败回调
      onError={(error) => { console.warn(`Hook image ${props.hook().image ?? props.hook().slot}: ${error.message}`); failedSource.set(source()); }} /> : <Text style={{ color: color.muted, fontSize: 12, lineHeight: 16, padding: 6, textAlign: "center" }}>
        {source() && failedSource() === source() ? "图片无法读取" : props.hook().image == null ? "暂无图片" : props.connected() ? "图片尚未缓存" : "图片不在本地缓存中"}
      </Text>}
  </View>;
}
