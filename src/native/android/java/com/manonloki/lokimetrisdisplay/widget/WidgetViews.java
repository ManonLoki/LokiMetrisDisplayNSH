// 小组件包：系统小组件的 RemoteViews 渲染
package com.manonloki.lokimetrisdisplay.widget;

// 引入系统小组件与界面类型
import android.appwidget.AppWidgetManager;
import android.content.Context;
import android.widget.RemoteViews;
// 引入 JSON 解析
import org.json.JSONArray;
import org.json.JSONObject;
// 引入日期格式化
import java.text.DateFormat;
import java.util.Date;

/** 小组件的 RemoteViews 渲染：视图字段与 iOS 小组件共用同一投影规则，
 *  日期与时间由本机区域格式化后交回核心生成描述文案。 */
// 渲染工具类：只做 RemoteViews 装配，不保存状态
final class WidgetViews {
    // 工具类不实例化
    private WidgetViews() {
    }

    /** 按实例尺寸选择布局并填充文案；视图字段与 iOS 小组件共用同一投影。 */
    // 渲染一个实例
    static RemoteViews views(Context context, AppWidgetManager manager, int id, JSONObject loaded) {
        // 中号阈值：最小宽度不小于 250dp 视为中号
        boolean medium = manager.getAppWidgetOptions(id).getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH, 0) >= 250;
        // 按尺寸选择布局
        RemoteViews views = new RemoteViews(context.getPackageName(),
                medium ? com.manonloki.lokimetrisdisplay.R.layout.widget_medium
                       : com.manonloki.lokimetrisdisplay.R.layout.widget_small);
        // 尚未同步：渲染空态
        if (loaded == null) {
            // 空态文案
            JSONObject empty = describe(null, 0);
            // 填充空态；描述不可用时给出与产品一致的缺省，不留空白
            views.setTextViewText(com.manonloki.lokimetrisdisplay.R.id.widget_token,
                    empty.optString("widgetEmptyTitle", "尚未同步"));
            views.setTextViewText(com.manonloki.lokimetrisdisplay.R.id.widget_sync,
                    empty.optString("widgetEmptyHelp", "打开 App 连接桌面"));
            views.setTextViewText(com.manonloki.lokimetrisdisplay.R.id.widget_hooks,
                    empty.optString("widgetEmptyInline", "诡秘神谕 · 尚未同步"));
            // 隐藏明细区
            hideRows(views, medium ? 3 : 0, medium ? 2 : 0);
            // 返回空态视图
            return views;
        }
        // 接收时间
        long received = loaded.optLong("receivedAtMs", 0L);
        // 视图投影
        JSONObject view = loaded.optJSONObject("view");
        // 视图缺失按空态处理
        if (view == null) {
            // 空态渲染
            return views(context, manager, id, null);
        }
        // 中号最多显示三行明细；可见数与槽位数一致，剩余提示才有槽位可渲染
        int visibleAgents = medium ? 3 : 0;
        // 生成全部描述文案
        JSONObject text = describe(view, visibleAgents);
        // 统计日期与同步时间行
        views.setTextViewText(com.manonloki.lokimetrisdisplay.R.id.widget_sync, text.optString("appleCompactSyncText"));
        // Hooks 行
        views.setTextViewText(com.manonloki.lokimetrisdisplay.R.id.widget_hooks, text.optString("appleAccessoryInline"));
        // Token 大字：无用量时显示备用文案
        JSONObject tokens = view.optJSONObject("tokens");
        // 总量文本
        String token = tokens == null ? view.optString("tokenFallback")
                : tokens.optJSONObject("total").optString("compact", view.optString("tokenFallback"));
        // 填充大字
        views.setTextViewText(com.manonloki.lokimetrisdisplay.R.id.widget_token, token);
        // 明细与额度行
        int agent = 0;
        int quota = 0;
        // 中号才填充明细
        if (medium) {
            // 明细行
            JSONArray agents = view.optJSONArray("agents");
            // 逐行填充
            for (int index = 0; agents != null && index < agents.length() && agent < 3; index++) {
                // 行内容
                JSONObject row = agents.optJSONObject(index);
                // 行无效跳过
                if (row == null) {
                    // 下一行
                    continue;
                }
                // 填充行文本：名称与紧凑用量
                views.setTextViewText(rowId(agent), row.optString("name") + "  " + row.optString("compact"));
                // 下一行
                agent++;
            }
            // 仍有未显示的明细时给出提示
            String remaining = text.optString("remainingAgents");
            // 有剩余且还有空位才显示
            if (!remaining.isEmpty() && agent < 3) {
                // 填充提示行
                views.setTextViewText(rowId(agent), remaining);
                // 占一行
                agent++;
            }
            // 额度行
            JSONArray providers = view.optJSONObject("quota") == null ? null
                    : view.optJSONObject("quota").optJSONArray("providers");
            // 逐个填充
            for (int index = 0; providers != null && index < providers.length() && quota < 2; index++) {
                // 供应商内容
                JSONObject provider = providers.optJSONObject(index);
                // 无效跳过
                if (provider == null) {
                    // 下一个
                    continue;
                }
                // 第一个窗口的剩余文本
                JSONObject window = provider.optJSONArray("windows") == null ? null
                        : provider.optJSONArray("windows").optJSONObject(0);
                // 无窗口则跳过
                if (window == null) {
                    // 下一个
                    continue;
                }
                // 填充：名称加窗口剩余
                views.setTextViewText(quotaId(quota), provider.optString("name")
                        + "  " + window.optString("label") + " " + window.optString("percent"));
                // 占一行
                quota++;
            }
        }
        // 隐藏未用到的行
        hideRows(views, agent, quota);
        // 返回视图
        return views;
    }

    /** 把未用到的明细与额度行设为隐藏。 */
    // 隐藏未用行
    private static void hideRows(RemoteViews views, int agents, int quotas) {
        // 三个明细位
        for (int index = 0; index < 3; index++) {
            // 已用位跳过
            if (index < agents) {
                // 保留
                continue;
            }
            // 隐藏
            views.setViewVisibility(rowId(index), android.view.View.GONE);
        }
        // 两个额度位
        for (int index = 0; index < 2; index++) {
            // 已用位跳过
            if (index < quotas) {
                // 保留
                continue;
            }
            // 隐藏
            views.setViewVisibility(quotaId(index), android.view.View.GONE);
        }
    }

    // 明细行的视图标识
    private static int rowId(int index) {
        // 三个固定位
        switch (index) {
            // 第一行
            case 0: return com.manonloki.lokimetrisdisplay.R.id.widget_agent_1;
            // 第二行
            case 1: return com.manonloki.lokimetrisdisplay.R.id.widget_agent_2;
            // 第三行
            default: return com.manonloki.lokimetrisdisplay.R.id.widget_agent_3;
        }
    }

    // 额度行的视图标识
    private static int quotaId(int index) {
        // 两个固定位
        switch (index) {
            // 第一行
            case 0: return com.manonloki.lokimetrisdisplay.R.id.widget_quota_1;
            // 第二行
            default: return com.manonloki.lokimetrisdisplay.R.id.widget_quota_2;
        }
    }

    /** 生成描述文案：视图为 null 时输出空态文案；日期与时间由本机区域格式化。 */
    // 描述文案入口
    static JSONObject describe(JSONObject view, int visibleAgents) {
        try {
            // 描述请求
            JSONObject request = new JSONObject();
            // 操作名
            request.put("op", "view.describe");
            // 视图可能为空对象（空态）
            request.put("view", view == null ? new JSONObject() : view);
            // 应用名固定为产品名
            request.put("appName", "诡秘神谕");
            // 显示的明细行数
            request.put("visibleAgents", visibleAgents);
            // 有视图才附加时间戳
            if (view != null) {
                // 当前时刻
                long now = System.currentTimeMillis();
                // 完整同步时间（本机区域）
                request.put("syncTime", DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(new Date(now)));
                // 紧凑同步时间（本机区域）
                request.put("compactSyncTime", DateFormat.getTimeInstance(DateFormat.SHORT).format(new Date(now)));
                // 额度自身的观测时间：缺失时省略，不冒充刚刚更新
                JSONObject quota = view.optJSONObject("quota");
                // 观测时间（秒）
                long observed = quota == null ? 0L : quota.optLong("observedAtUnix", 0L);
                // 有观测时间才附加
                if (observed > 0) {
                    // 额度更新时间（本机区域）
                    Date at = new Date(observed * 1000L);
                    // 完整与紧凑两种写法
                    request.put("quotaTime", DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.MEDIUM).format(at));
                    request.put("quotaCompactTime", DateFormat.getTimeInstance(DateFormat.SHORT).format(at));
                }
            }
            // 调用核心
            JSONObject result = LokiWidgetProvider.call(request);
            // 取 ok 字段；失败返回空对象
            JSONObject ok = result == null ? null : result.optJSONObject("ok");
            // 无文案则给空对象，调用方用缺省文案
            return ok == null ? new JSONObject() : ok;
        // 组装失败
        } catch (Exception error) {
            // 空文案
            return new JSONObject();
        }
    }
}
