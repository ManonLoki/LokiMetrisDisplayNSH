import { computed, signal, untracked, View, type JSX, windowSize, safeArea } from "nsh";
import { CyberBackdrop, CyberMotionScope } from "../shared/ui/effects";
import { enableRemoteNavigation } from "../shared/ui/remote-focus";
import { CyberNavigation } from "./navigation";
import { PanelBody } from "../features/dashboard";
import { createPanelController } from "../features/hooks";
import { SettingsPage, createSettingsController } from "../features/settings";
import { cyberColors } from "../shared/ui/theme";
import { emptyView, type ViewState } from "../features/connection";
import { defaultPanel } from "../shared/logic";
import type { PanelConfig } from "../shared/contracts/panel";
import { navPlacement } from "./navigation-layout";
import { createRuntime, type AppRuntime } from "./runtime";

export type { AppRuntime, LinkPort } from "./runtime";

/** 应用层只持有导航、输入草稿、可见状态和布局控制器。 */
export function createApp(runtime: AppRuntime): () => JSX.Element {
  const page = signal<"panel" | "settings">("panel");
  const width = signal(untracked(() => windowSize().width));
  const height = signal(untracked(() => windowSize().height));
  const wideNavigation = computed(() => navPlacement(width()) === "side");
  const contentWidth = signal(390);
  const contentHeight = signal(776);
  const panel = signal<PanelConfig>(defaultPanel());
  const view = signal<ViewState>(emptyView("", ""));
  const nameDraft = signal("");
  const nameFeedback = signal<string | null>(null);
  const nameIssue = signal<string | null>(null);
  const panelIssue = signal<string | null>(null);
  const savingName = signal(false);
  const appForeground = signal(false);
  const panelController = createPanelController();
  const settingsController = createSettingsController();
  const ports = createRuntime(runtime, {
    panel, view, nameDraft, nameFeedback, nameIssue, panelIssue, savingName, foreground: appForeground,
  }, closeMissingHook);
  const bg = cyberColors.canvas;

  function closeMissingHook(next: ViewState) {
    const slot = panelController.selectedSlot();
    if (slot != null && (!next.snapshot?.hooksAvailable || !next.snapshot.hooks.some((hook) => hook.slot === slot))) {
      panelController.selectedSlot.set(null);
    }
  }

  function chooseDesktop(id: string) {
    panelController.selectedSlot.set(null);
    ports.command({ op: "select", clientId: id });
  }

  function forgetDesktopNow(id: string) {
    panelController.selectedSlot.set(null);
    ports.command({ op: "forget", clientId: id });
  }

  function App() {
    ports.mount();
    // 电视没有触摸：方向键在可按目标之间移动焦点，确认键执行。
    enableRemoteNavigation(() => runtime.remoteNavigation?.() === true);
    return (
      <View
        testID="app-root"
        style={{ flex: 1, backgroundColor: bg, paddingTop: safeArea().top, paddingRight: safeArea().right,
          paddingBottom: safeArea().bottom, paddingLeft: safeArea().left }}
        onLayout={(bounds) => {
          width.set(bounds.size.width);
          height.set(bounds.size.height);
        }}
      >
        <CyberMotionScope foreground={appForeground}>
          <View style={{ flex: 1 }}>
            <CyberBackdrop testID="cyber-backdrop" />
            <View style={{ flex: 1, flexDirection: wideNavigation() ? "row" : "column" }}>
              {() => wideNavigation() ? nav("side") : null}
              <View style={{ flex: 1, minWidth: 0, minHeight: 0 }} onLayout={(bounds) => {
                contentWidth.set(bounds.size.width);
                contentHeight.set(bounds.size.height);
              }}>
                {() => page() === "panel" ? <PanelBody width={contentWidth} height={contentHeight}
                  viewportWidth={width} viewportHeight={height} state={view} config={panel} controller={panelController}
                  onSettings={() => page.set("settings")}
                  onFrame={runtime.onFrame} onImageLoad={runtime.onImageLoad} /> : settingsBody()}
              </View>
              {() => wideNavigation() ? null : nav("bottom")}
            </View>
          </View>
        </CyberMotionScope>
      </View>
    );

  }

  function nav(place: "side" | "bottom") {
    return <CyberNavigation placement={place} selected={page} onSelect={(next) => page.set(next)} />;
  }

  function settingsBody() {
    return <SettingsPage controller={settingsController} width={contentWidth} state={view} config={panel}
      name={nameDraft} nameIssue={nameIssue} nameFeedback={nameFeedback} panelError={panelIssue} savingName={savingName}
      onNameChange={ports.editName} onSaveName={ports.saveName} onPanelChange={ports.savePanel}
      onRetry={() => ports.command({ op: "refresh" })} onSelect={chooseDesktop} onForget={forgetDesktopNow} />;
  }

  return App;
}
