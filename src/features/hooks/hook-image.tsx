import { Text, View, computed, signal, type Accessor, type Size } from "nsh";
import type { HookItem } from "../../shared/contracts/snapshot";
import { agentDisplayName } from "../../shared/domain/agents";
import { CyberImage } from "../../shared/ui/image";
import { cyberColors as color } from "../../shared/ui/theme";

export function HookImageViewport(props: { hook: Accessor<HookItem>; connected: Accessor<boolean>; showImage?: Accessor<boolean>; active?: Accessor<boolean>;
  height?: number; detail?: boolean; onImageLoad?: (hash: string, size: Size) => void }) {
  const failedSource = signal<string | null>(null);
  const visible = computed(() => props.showImage?.() ?? true);
  const source = computed(() => props.hook().imageSource ?? null);
  const imageReady = computed(() => source() != null && failedSource() !== source());
  return <View style={{ flex: props.height == null ? 1 : undefined, height: props.height, minWidth: 0, minHeight: 0,
    width: "100%", borderRadius: 12, backgroundColor: color.surfaceContainer, overflow: "hidden", alignItems: "center", justifyContent: "center" }}>
    {visible() ? imageReady() ? <CyberImage active={props.active}
      testID={props.detail ? "hook-detail-image" : `hook-image-${props.hook().slot}`}
      src={source()!} resizeMode="contain" style={{ width: "100%", height: "100%" }}
      accessibilityLabel={`${agentDisplayName(props.hook().agent)} 的 Hook 图片`}
      onLoad={(size) => props.onImageLoad?.(props.hook().image ?? source()!, size)}
      onError={() => failedSource.set(source())} /> : <Text style={{ color: color.muted, fontSize: 12, lineHeight: 16, padding: 6, textAlign: "center" }}>
        {source() && failedSource() === source() ? "图片无法读取" : props.hook().image == null ? "暂无图片" : props.connected() ? "图片尚未缓存" : "图片不在本地缓存中"}
      </Text> : null}
  </View>;
}
