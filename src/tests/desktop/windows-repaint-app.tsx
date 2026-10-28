import { Canvas, mount, platform, reduceMotion, signal, Storage, Text, View, type AnimatedValue, type CanvasNumber } from "nsh";
import { Button } from "nsh/base";
import { useWindowsRepaint } from "../../shared/ui/windows-repaint";

// 独立的重绘生命周期夹具，不启动连接、配对或生产应用的轮询。
const foreground = signal(false), attached = signal(true);
// 保留最近一次挂载的值，使测试能观察组件卸载后的停止状态。
let lastValue: CanvasNumber = 0, sequence = 0;

function RepaintSurface() {
  const repaint = useWindowsRepaint(foreground);
  lastValue = repaint;
  return <Canvas testID="repaint-surface" style={{ width: 200, height: 120 }} draw={(cx) => {
    cx.fillStyle = "#071016";
    cx.fillRect(0, 0, 200, 120);
    cx.fillRect(-2, -2, repaint, 1);
  }} />;
}

async function snapshot() {
  const value = typeof lastValue === "number" ? lastValue : await (lastValue as AnimatedValue).getValue();
  await Storage.setItem("test.windowsRepaint", JSON.stringify({
    sequence: ++sequence, value, foreground: foreground(), attached: attached(),
    reducedMotion: reduceMotion(), platform: platform.os,
  }));
}

mount(() => <View style={{ flex: 1, gap: 12, padding: 16 }}>
  <Button testID="repaint-toggle-foreground" onPress={() => foreground.set(!foreground())}>
    <Text>切换前台</Text>
  </Button>
  <Button testID="repaint-toggle-attachment" onPress={() => attached.set(!attached())}>
    <Text>切换挂载</Text>
  </Button>
  <Button testID="repaint-probe" onPress={() => { void snapshot(); }}>
    <Text>读取重绘进度</Text>
  </Button>
  {() => attached() ? <RepaintSurface /> : null}
</View>);
