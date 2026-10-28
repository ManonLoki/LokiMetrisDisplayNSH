// 引入 nsh 运行时与存储
import { effect, mount, reduceMotion, signal, Storage, Text, View } from "nsh";
// 引入按钮
import { Button } from "nsh/base";
// 引入动效作用域
import { CyberMotionScope } from "../../shared/ui/effects";
// 引入图片组件
import { CyberImage } from "../../shared/ui/image";

// 显式的仅图片夹具:没有 Link、真实配对、网络或连接所有者。
// 读取测试选项后挂载
void (async () => {
  // 读取测试选项
  const options = JSON.parse((await Storage.getItem("test.imagePlayback")) ?? "{}");
  // 图片地址
  const source = String(options.source ?? "");
  // 两个图片的活跃开关与前台开关
  const activeA = signal(true), activeB = signal(false), foreground = signal(true);
  // 加载完成标记与错误信息
  const loadedA = signal(false), loadedB = signal(false), error = signal("");
  // 两张图片的固定矩形
  const rects = {
    a: { x: 24, y: 100, width: 200, height: 200 },
    b: { x: 264, y: 100, width: 200, height: 200 },
  };
  // 写入矩形供测试读取
  await Storage.setItem("test.imagePlaybackRects", JSON.stringify(rects));
  // 挂载页面
  mount(() => {
    // 把状态持续写入存储
    effect(() => {
      void Storage.setItem("test.imagePlaybackState", JSON.stringify({
        source, activeA: activeA(), activeB: activeB(), foreground: foreground(),
        reducedMotion: reduceMotion(), loadedA: loadedA(), loadedB: loadedB(), error: error(),
      }));
    });
    return <View style={{ flex: 1, backgroundColor: "#071016" }}>
      {/* Pixel oracle uses this fixed marker to establish bitmap row orientation. */}
      {/* 标记方块 */}
      <View style={{ position: "absolute", left: 4, top: 4, width: 12, height: 12, backgroundColor: "#00ff00" }} />
      {/* 三个切换按钮 */}
      <View style={{ position: "absolute", left: 24, top: 20, width: 440, height: 48, flexDirection: "row", gap: 8 }}>
        {/* 切换 A 图的活跃状态 */}
        <Button testID="image-toggle-a" onPress={() => activeA.set(!activeA())} style={{ flex: 1 }}>
          <Text>A: {() => String(activeA())}</Text>
        </Button>
        {/* 切换 B 图的活跃状态 */}
        <Button testID="image-toggle-b" onPress={() => activeB.set(!activeB())} style={{ flex: 1 }}>
          <Text>B: {() => String(activeB())}</Text>
        </Button>
        {/* 切换动效作用域的前台状态 */}
        <Button testID="image-toggle-foreground" onPress={() => foreground.set(!foreground())} style={{ flex: 1 }}>
          <Text>scope: {() => String(foreground())}</Text>
        </Button>
      </View>
      {/* 动效作用域 */}
      <CyberMotionScope foreground={foreground}>
        {/* A 图：初始活跃 */}
        <CyberImage testID="image-playback-a" src={source} active={activeA}
          style={{ position: "absolute", left: rects.a.x, top: rects.a.y, width: 200, height: 200 }}
          onLoad={() => loadedA.set(true)} onError={(failure) => error.set(failure.message)} />
        {/* B 图：同一地址，初始不活跃，播放进度独立 */}
        <CyberImage testID="image-playback-b" src={source} active={activeB}
          style={{ position: "absolute", left: rects.b.x, top: rects.b.y, width: 200, height: 200 }}
          onLoad={() => loadedB.set(true)} onError={(failure) => error.set(failure.message)} />
      </CyberMotionScope>
      {/* 说明文字与错误信息 */}
      <Text style={{ position: "absolute", left: 24, top: 320, color: "#ffffff" }}>
        Same URI · independent consumers {() => error()}
      </Text>
    </View>;
  });
})();
