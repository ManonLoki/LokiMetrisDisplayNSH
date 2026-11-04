// 小组件包：系统小组件提供器
package com.manonloki.lokimetrisdisplay.widget;

// 引入系统小组件与界面类型
import android.appwidget.AppWidgetManager;
import android.appwidget.AppWidgetProvider;
import android.content.ComponentName;
import android.content.Context;
import android.os.Bundle;
import android.widget.RemoteViews;
// 引入 JSON 解析
import org.json.JSONArray;
import org.json.JSONObject;
// 引入文本与日期格式化
import java.text.DateFormat;
import java.util.Date;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** Android 系统小组件：与主应用同进程读取共享摘要，渲染与独立只读 HTTP 全部走小组件核心。
 *  提供器不保存任何状态；刷新按提交顺序在单一工作线程上串行执行。 */
// 系统小组件提供器：读取共享摘要并渲染 RemoteViews
public final class LokiWidgetProvider extends AppWidgetProvider {
    // 单一工作线程：渲染与网络互斥，结果按提交顺序生效
    private static final ExecutorService WORKER = Executors.newSingleThreadExecutor(runnable -> {
        // 后台线程不阻止进程退出
        Thread thread = new Thread(runnable, "loki-widget-render");
        // 守护线程
        thread.setDaemon(true);
        // 返回线程
        return thread;
    });

    // 重写父类方法
    @Override
    // 系统按小组件信息调度刷新：允许独立只读 HTTP
    public void onUpdate(Context context, AppWidgetManager manager, int[] ids) {
        // 串行提交一次带网络的刷新
        WORKER.execute(() -> refresh(context.getApplicationContext(), true));
    }

    // 重写父类方法
    @Override
    // 尺寸跨过小/中阈值时重新选择布局并重绘
    public void onAppWidgetOptionsChanged(Context context, AppWidgetManager manager,
            int id, Bundle options) {
        // 串行提交一次仅渲染的刷新
        WORKER.execute(() -> refresh(context.getApplicationContext(), false));
    }

    /** 主应用摘要变化时由 Rust 侧调用的静态入口：只重绘共享缓存，不发网络请求。 */
    // Rust 侧的静态刷新入口
    public static void refreshAll(Context context) {
        // 串行提交一次仅渲染的刷新
        WORKER.execute(() -> refresh(context.getApplicationContext(), false));
    }

    /** 一次完整刷新：可选独立 HTTP，然后从共享存储渲染全部小组件。 */
    // 刷新入口：allowNetwork 为真时先尝试独立只读读取
    private static void refresh(Context context, boolean allowNetwork) {
        // 允许网络时先尝试独立读取；失败不阻断渲染
        if (allowNetwork) {
            // 独立只读读取
            fetch(context);
        }
        // 渲染全部小组件
        render(context);
    }

    /** 从共享存储读取摘要并渲染全部已添加的小组件。 */
    // 仅渲染入口
    private static void render(Context context) {
        // 系统小组件管理器
        AppWidgetManager manager = AppWidgetManager.getInstance(context);
        // 本提供器的全部实例
        int[] ids = manager.getAppWidgetIds(new ComponentName(context, LokiWidgetProvider.class));
        // 无实例则无事可做
        if (ids.length == 0) {
            // 直接返回
            return;
        }
        // 读取共享目录；未就绪按空态渲染
        String directory = WidgetCore.widgetDir();
        // 摘要投影结果：null 表示尚未同步
        JSONObject loaded = null;
        // 有目录才读取
        if (directory != null) {
            // 读取缓存的摘要
            loaded = store(directory, System.currentTimeMillis());
        }
        // 逐个实例渲染
        for (int id : ids) {
            // 按当前尺寸选择布局
            RemoteViews views = WidgetViews.views(context, manager, id, loaded);
            // 推送给系统
            manager.updateAppWidget(id, views);
        }
    }

    /** 调用小组件核心读取缓存的摘要；无摘要或读取失败返回 null。 */
    // 读取共享摘要
    private static JSONObject store(String directory, long nowMs) {
        // 组装核心请求
        JSONObject result = call(request("store.load", directory, nowMs));
        // 无结果
        if (result == null) {
            // 未同步
            return null;
        }
        // 取 ok 字段
        JSONObject ok = result.optJSONObject("ok");
        // 空摘要或解析失败都按未同步处理
        if (ok == null || ok.isNull("view")) {
            // 未同步
            return null;
        }
        // 返回摘要记录
        return ok;
    }

    /** 独立只读读取：核心生成加密请求，Java 只执行一次 HTTP，结果交回核心校验入库。 */
    // 独立只读读取
    private static void fetch(Context context) {
        // 共享目录不可用时无法读取凭据
        String directory = WidgetCore.widgetDir();
        // 无目录
        if (directory == null) {
            // 直接返回
            return;
        }
        // 当前时间
        long now = System.currentTimeMillis();
        // 请求一次独立读取
        JSONObject begin = call(request("widget.begin", directory, now));
        // 请求失败按无读取处理
        if (begin == null) {
            // 直接返回
            return;
        }
        // 取 ok 字段
        JSONObject ok = begin.optJSONObject("ok");
        // 租约内或无凭据都会跳过
        if (ok == null || ok.has("skip") || !ok.has("request")) {
            // 直接返回
            return;
        }
        // 请求描述与凭据
        JSONObject request = ok.optJSONObject("request");
        JSONObject target = ok.optJSONObject("target");
        // 两者齐备才发送
        if (request == null || target == null) {
            // 直接返回
            return;
        }
        // 执行一次 HTTP
        JSONObject reply = http(request);
        // 请求失败
        if (reply == null) {
            // 直接返回
            return;
        }
        // 组装完成请求：目标、请求、状态、媒体类型与响应体
        JSONObject finish = request("widget.finish", directory, now);
        // 完成请求构造失败
        if (finish == null) {
            // 直接返回
            return;
        }
        // 附上完成所需的全部字段
        try {
            // 凭据与请求原样带回
            finish.put("target", target);
            finish.put("request", request);
            finish.put("status", reply.optLong("status"));
            // 媒体类型可能缺失
            if (reply.has("mediaType")) {
                // 附上响应类型
                finish.put("mediaType", reply.getString("mediaType"));
            }
            // 响应体文本
            finish.put("body", reply.optString("body"));
        // 组装失败
        } catch (Exception error) {
            // 直接返回
            return;
        }
        // 交回核心校验入库；失败静默，渲染仍使用最近缓存
        WidgetCore.coreCall(finish.toString());
    }

    /** 按核心给出的请求描述执行一次 HTTP；只接受固定方法与大小上限，不跟随重定向。 */
    // 执行 HTTP 请求
    private static JSONObject http(JSONObject request) {
        // 连接对象
        java.net.HttpURLConnection connection = null;
        try {
            // 只接受核心给出的固定地址
            connection = (java.net.HttpURLConnection) new java.net.URL(request.getString("url")).openConnection();
            // 固定为 POST；实例重定向由核心的地址规范约束，这里不跟随
            connection.setRequestMethod("POST");
            // 不跟随重定向
            connection.setInstanceFollowRedirects(false);
            // 请求体为 JSON
            connection.setRequestProperty("Content-Type", "application/json");
            // 核心给出的超时
            int timeout = (int) Math.min(request.optLong("timeoutMs", 6000L), 10_000L);
            // 连接与读取超时
            connection.setConnectTimeout(timeout);
            connection.setReadTimeout(timeout);
            // 发送请求体
            connection.setDoOutput(true);
            // 写入加密请求体
            connection.getOutputStream().write(request.getString("body").getBytes("UTF-8"));
            // 状态码
            int status = connection.getResponseCode();
            // 响应类型
            String mediaType = connection.getContentType();
            // 响应上限
            long limit = Math.min(request.optLong("maxResponseBytes", 96 * 1024L), 96 * 1024L);
            // 读到上限为止
            java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream();
            // 读流
            try (java.io.InputStream stream = connection.getInputStream()) {
                // 有界读取
                byte[] buffer = new byte[8192];
                // 逐块读取
                for (int read; (read = stream.read(buffer)) > 0; ) {
                    // 追加
                    bytes.write(buffer, 0, read);
                    // 超限即止
                    if (bytes.size() > limit) {
                        // 停止读取
                        break;
                    }
                }
            }
            // 组装回复
            JSONObject reply = new JSONObject();
            // 状态码
            reply.put("status", status);
            // 媒体类型可能为 null
            if (mediaType != null) {
                // 附上类型
                reply.put("mediaType", mediaType);
            }
            // 响应体按 UTF-8 文本带回
            reply.put("body", bytes.toString("UTF-8"));
            // 返回回复
            return reply;
        // 任一网络步骤失败
        } catch (Exception error) {
            // 返回失败
            return null;
        // 收尾
        } finally {
            // 连接存在才断开
            if (connection != null) {
                // 归还连接
                connection.disconnect();
            }
        }
    }

    /** 组装核心请求：目录与当前时间由调用方提供。 */
    // 组装基础请求
    private static JSONObject request(String op, String directory, long nowMs) {
        try {
            // 请求对象
            JSONObject request = new JSONObject();
            // 操作名
            request.put("op", op);
            // 共享目录
            request.put("dir", directory);
            // 当前时间
            request.put("nowMs", nowMs);
            // 返回请求
            return request;
        // 组装失败
        } catch (Exception error) {
            // 无请求
            return null;
        }
    }

    /** 调用小组件核心并解析回复；失败返回 null。 */
    // 调用核心
    static JSONObject call(JSONObject request) {
        // 无请求
        if (request == null) {
            // 失败
            return null;
        }
        try {
            // 原生核心调用
            String response = WidgetCore.coreCall(request.toString());
            // 空回复
            if (response == null) {
                // 失败
                return null;
            }
            // 解析回复
            return new JSONObject(response);
        // 调用或解析失败
        } catch (Exception error) {
            // 失败
            return null;
        }
    }
}
