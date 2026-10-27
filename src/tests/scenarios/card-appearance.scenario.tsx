import { expect, open, test, type App } from "nsh/test";

// Native screenshots require --target macos. This fixture never opens the production Link.
const OUTPUT = "__OUTPUT_DIRECTORY__";
const ENTRY = "__APP_ENTRY__";
const viewport = { width: 520, height: 680 };

function gates(app: App) {
  return JSON.parse(app.storage()["test.appearanceGates"] ?? "{}");
}

async function capture(app: App, group: string, index: number) {
  const file = `${group}-${index}.png`;
  await app.screenshot(`${OUTPUT}/${file}`);
  console.log(`CARD_APPEARANCE_CAPTURE=${JSON.stringify({ file, group, index, viewport,
    rect: JSON.parse(app.storage()["test.appearanceRect"]), gates: gates(app), timeMs: Date.now() })}`);
}

test("rounded Token edge travels around four sides and respects motion gates", async () => {
  const app = await open(ENTRY, { ...viewport, reducedMotion: false, network: {},
    storage: { "test.cardAppearance": JSON.stringify({ mode: "motion" }) } });
  await app.waitFor("usage-surface-scan", { timeout: 15_000 });
  await app.waitUntil(() => gates(app).active === true && gates(app).foreground === true);
  expect(gates(app).reducedMotion).toBe(false);
  await app.wait(900); // Finish the card's arrival before measuring only its border.
  for (let index = 0; index < 12; index++) {
    if (index) await app.wait(800);
    await capture(app, "orbit", index);
  }
  await app.press("appearance-toggle-active");
  await app.waitUntil(() => gates(app).active === false);
  await app.wait(500);
  await capture(app, "inactive", 0);
  await app.wait(1400);
  await capture(app, "inactive", 1);
  await app.press("appearance-toggle-active");
  await app.waitUntil(() => gates(app).active === true);
  await app.wait(800);
  await capture(app, "resumed", 0);
  await app.wait(1400);
  await capture(app, "resumed", 1);
  await app.press("appearance-toggle-foreground");
  await app.waitUntil(() => gates(app).foreground === false);
  await app.wait(500);
  await capture(app, "background", 0);
  await app.wait(1400);
  await capture(app, "background", 1);
  app.dispose();

  const reduced = await open(ENTRY, { ...viewport, reducedMotion: true, network: {},
    storage: { "test.cardAppearance": JSON.stringify({ mode: "motion" }) } });
  await reduced.waitFor("usage-surface-scan", { timeout: 15_000 });
  await reduced.waitUntil(() => gates(reduced).reducedMotion === true);
  await reduced.wait(600);
  await capture(reduced, "reduced", 0);
  await reduced.wait(1400);
  await capture(reduced, "reduced", 1);
}, 90_000);

test("shared phone and desktop cards and settings render with isolated fixture data", async () => {
  for (const [name, width, height] of [["phone", 390, 844], ["desktop", 1120, 900]] as const) {
    const app = await open(ENTRY, { width, height, reducedMotion: true, network: {},
      storage: { "test.cardAppearance": JSON.stringify({ mode: "app" }),
        "loki.panel": JSON.stringify({ showUsage: true, showHooks: true, usageDetail: "detailed", hookLayout: "2x2" }) } });
    await app.waitFor("usage-total", { timeout: 15_000 });
    await app.waitFor("hook-card-4");
    await app.wait(600);
    await app.screenshot(`${OUTPUT}/${name}-panel.png`);
    console.log(`CARD_APPEARANCE_VISUAL=${JSON.stringify({ file: `${name}-panel.png`, width, height, page: "panel" })}`);
    await app.press("menu-settings");
    await app.waitFor("settings-panel-card");
    await app.wait(400);
    await app.screenshot(`${OUTPUT}/${name}-settings.png`);
    console.log(`CARD_APPEARANCE_VISUAL=${JSON.stringify({ file: `${name}-settings.png`, width, height, page: "settings" })}`);
    app.dispose();
  }
}, 60_000);
