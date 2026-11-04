package com.manonloki.lokimetrisdisplay.widget;

/** 夹具核心：返回固定摘要与描述，可切换为不可用状态；生产编译里这个类由原生实现承担。 */
public final class WidgetCore {
    // 是否模拟核心不可用
    public static boolean unavailable;
    // 是否模拟共享目录缺失
    public static boolean missingDir;

    private WidgetCore() {}

    // 共享目录；可模拟缺失
    static String widgetDir() {
        if (missingDir) return null;
        return "/fixture/widget";
    }

    // 核心调用；按操作返回固定夹具
    static String coreCall(String request) {
        if (unavailable) return null;
        if (request.contains("store.load")) {
            return "{"
                + "\"ok\":{"
                + "\"receivedAtMs\":1730000000000,"
                + "\"view\":{"
                + "\"tokens\":{"
                + "\"day\":{\"date\":\"11-03\"},"
                + "\"total\":{\"text\":\"4,321\",\"compact\":\"4.3k\"}"
                + "},"
                + "\"hooksAvailable\":true,"
                + "\"hooks\":[{\"slot\":1,\"agent\":\"codex\",\"behavior\":\"running\"}],"
                + "\"quota\":{\"observedAtUnix\":1730000000,\"providers\":[{"
                + "\"provider\":\"codex\",\"name\":\"Codex\","
                + "\"windows\":[{\"kind\":\"fiveHour\",\"label\":\"5 小时\",\"percent\":\"71%\"}]"
                + "}]},"
                + "\"agents\":["
                + "{\"id\":\"agent:codex\",\"name\":\"Codex\",\"text\":\"4,321\",\"compact\":\"4.3k\"},"
                + "{\"id\":\"agent:grok\",\"name\":\"Grok\",\"text\":\"0\",\"compact\":\"0\"}"
                + "]"
                + "}"
                + "}}";
        }
        if (request.contains("view.describe")) {
            // 夹具有两行明细；可见数不小于两行时没有剩余提示
            boolean narrow = request.contains("\"visibleAgents\":0");
            String remaining = narrow ? "另 2 个 Agent" : "";
            return "{"
                + "\"ok\":{"
                + "\"appleCompactSyncText\":\"11-03 · 同步 08:00\","
                + "\"appleAccessoryInline\":\"上次 08:00 · Hooks 1\","
                + "\"remainingAgents\":\"" + remaining + "\","
                + "\"appleSyncText\":\"上次同步 08:00\","
                + "\"widgetEmptyInline\":\"诡秘神谕 · 尚未同步\","
                + "\"widgetEmptyTitle\":\"尚未同步\","
                + "\"widgetEmptyHelp\":\"打开 App 连接桌面\""
                + "}}";
        }
        return "{\"error\":\"unsupported\"}";
    }
}
