import { effect, mount, reduceMotion, signal, Storage, Text, View } from "nsh";
import { Button } from "nsh/base";
import { CyberMotionScope } from "../../shared/ui/effects";
import { CyberImage } from "../../shared/ui/image";

// Explicit image-only fixture: no Link, real pairing, network, or connection owner.
void (async () => {
  const options = JSON.parse((await Storage.getItem("test.imagePlayback")) ?? "{}");
  const source = String(options.source ?? "");
  const activeA = signal(true), activeB = signal(false), foreground = signal(true);
  const loadedA = signal(false), loadedB = signal(false), error = signal("");
  const rects = {
    a: { x: 24, y: 100, width: 200, height: 200 },
    b: { x: 264, y: 100, width: 200, height: 200 },
  };
  await Storage.setItem("test.imagePlaybackRects", JSON.stringify(rects));
  mount(() => {
    effect(() => {
      void Storage.setItem("test.imagePlaybackState", JSON.stringify({
        source, activeA: activeA(), activeB: activeB(), foreground: foreground(),
        reducedMotion: reduceMotion(), loadedA: loadedA(), loadedB: loadedB(), error: error(),
      }));
    });
    return <View style={{ flex: 1, backgroundColor: "#071016" }}>
      {/* Pixel oracle uses this fixed marker to establish bitmap row orientation. */}
      <View style={{ position: "absolute", left: 4, top: 4, width: 12, height: 12, backgroundColor: "#00ff00" }} />
      <View style={{ position: "absolute", left: 24, top: 20, width: 440, height: 48, flexDirection: "row", gap: 8 }}>
        <Button testID="image-toggle-a" onPress={() => activeA.set(!activeA())} style={{ flex: 1 }}>
          <Text>A: {() => String(activeA())}</Text>
        </Button>
        <Button testID="image-toggle-b" onPress={() => activeB.set(!activeB())} style={{ flex: 1 }}>
          <Text>B: {() => String(activeB())}</Text>
        </Button>
        <Button testID="image-toggle-foreground" onPress={() => foreground.set(!foreground())} style={{ flex: 1 }}>
          <Text>scope: {() => String(foreground())}</Text>
        </Button>
      </View>
      <CyberMotionScope foreground={foreground}>
        <CyberImage testID="image-playback-a" src={source} active={activeA}
          style={{ position: "absolute", left: rects.a.x, top: rects.a.y, width: 200, height: 200 }}
          onLoad={() => loadedA.set(true)} onError={(failure) => error.set(failure.message)} />
        <CyberImage testID="image-playback-b" src={source} active={activeB}
          style={{ position: "absolute", left: rects.b.x, top: rects.b.y, width: 200, height: 200 }}
          onLoad={() => loadedB.set(true)} onError={(failure) => error.set(failure.message)} />
      </CyberMotionScope>
      <Text style={{ position: "absolute", left: 24, top: 320, color: "#ffffff" }}>
        Same URI · independent consumers {() => error()}
      </Text>
    </View>;
  });
})();
