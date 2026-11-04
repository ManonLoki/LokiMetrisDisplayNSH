"""Execute the production widget provider against host-JVM Android API doubles."""
# 引入标准库
from pathlib import Path
# 环境变量
import os
# 运行 javac 与 java
import subprocess
# 调整导入路径
import sys
# 临时目录
import tempfile
# 测试框架
import unittest

# 仓库根目录
ROOT = Path(__file__).resolve().parents[3]
# 让测试能导入 tools/build 下的构建脚本
sys.path.insert(0, str(ROOT / 'tools/build'))
# 复用 javac、SDK 与平台 jar 定位逻辑
from build_android_bridge import java_compiler, platform_jar, sdk_directory

# 有界的 org.json 替身：宿主 JVM 上平台 jar 是编译期桩，渲染路径需要可运行的解析
from widget_json_doubles import SOURCES as JSON_DOUBLE_SOURCES

# 编译用的平台 jar：提供替身未覆盖的编译期符号
ANDROID_JAR = platform_jar(sdk_directory(None), 37)

# 生产小组件源码目录
PRODUCTION = ROOT / 'src/native/android/java/com/manonloki/lokimetrisdisplay/widget'



# 宿主 JVM 上的最小 Android 替身：只覆盖小组件路径触达的接口
DOUBLES = {
    'android/content/Context.java': '''package android.content;
// Context 替身：只提供包名与应用上下文
public class Context {
    // 包名固定为产品标识
    public String getPackageName() { return "com.manonloki.lokimetrisdisplay"; }
    // 应用上下文返回自身
    public Context getApplicationContext() { return this; }
}
''',
    'android/content/ComponentName.java': '''package android.content;
// ComponentName 替身：记录包与类名
public class ComponentName {
    // 包名与类名
    public final String packageName, className;
    // 由上下文与类构造
    public ComponentName(Context context, Class<?> klass) {
        this.packageName = context.getPackageName();
        this.className = klass.getName();
    }
}
''',
    'android/os/Bundle.java': '''package android.os;
import java.util.HashMap;
import java.util.Map;
// Bundle 替身：只存整型
public class Bundle {
    // 键值表
    private final Map<String, Integer> values = new HashMap<>();
    // 写入整型
    public void putInt(String key, int value) { values.put(key, value); }
    // 读取整型
    public int getInt(String key, int fallback) { return values.containsKey(key) ? values.get(key) : fallback; }
}
''',
    'android/view/View.java': '''package android.view;
// View 替身：可见性常量
public class View {
    // 可见性
    public static final int VISIBLE = 0, GONE = 8;
}
''',
    'android/app/Application.java': '''package android.app;
// Application 替身
public class Application extends android.content.Context {}
''',
    'android/appwidget/AppWidgetProvider.java': '''package android.appwidget;
import android.content.Context;
import android.os.Bundle;
// AppWidgetProvider 替身：生命周期钩子为空实现
public class AppWidgetProvider {
    // 系统调度刷新
    public void onUpdate(Context context, AppWidgetManager manager, int[] ids) {}
    // 尺寸变化
    public void onAppWidgetOptionsChanged(Context context, AppWidgetManager manager, int id, Bundle options) {}
}
''',
    'android/appwidget/AppWidgetManager.java': '''package android.appwidget;
import android.content.ComponentName;
import android.content.Context;
import android.os.Bundle;
import android.widget.RemoteViews;
import java.util.ArrayList;
import java.util.List;
// AppWidgetManager 替身：固定实例、可配置宽度与更新记录
public class AppWidgetManager {
    // 系统选项键
    public static final String OPTION_APPWIDGET_MIN_WIDTH = "appWidgetMinWidth";
    // 单例
    private static final AppWidgetManager INSTANCE = new AppWidgetManager();
    // 当前宽度
    public int minWidth = 300;
    // 已推送的视图
    public final List<RemoteViews> updated = new ArrayList<>();
    // 取单例
    public static AppWidgetManager getInstance(Context context) { return INSTANCE; }
    // 固定一个实例
    public int[] getAppWidgetIds(ComponentName name) { return new int[] {7}; }
    // 宽度选项
    public Bundle getAppWidgetOptions(int id) {
        Bundle options = new Bundle();
        options.putInt(OPTION_APPWIDGET_MIN_WIDTH, minWidth);
        return options;
    }
    // 记录推送
    public void updateAppWidget(int id, RemoteViews views) { updated.add(views); }
}
''',
    'android/widget/RemoteViews.java': '''package android.widget;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
// RemoteViews 替身：记录全部文本与可见性写入
public class RemoteViews {
    // 布局资源
    public final int layout;
    // 文本记录
    public final Map<Integer, String> texts = new LinkedHashMap<>();
    // 可见性记录
    public final Map<Integer, Integer> visibility = new LinkedHashMap<>();
    // 全部实例
    public static final List<RemoteViews> all = new ArrayList<>();
    // 构造
    public RemoteViews(String packageName, int layout) { this.layout = layout; all.add(this); }
    // 记录文本
    public void setTextViewText(int viewId, CharSequence text) { texts.put(viewId, String.valueOf(text)); }
    // 记录可见性
    public void setViewVisibility(int viewId, int visibility) { this.visibility.put(viewId, visibility); }
}
''',
    # 资源桩：与小组件布局一致的固定标识（打包时由 aapt2 生成真实 R）
    'com/manonloki/lokimetrisdisplay/R.java': '''package com.manonloki.lokimetrisdisplay;
// 资源桩
public final class R {
    // 布局
    public static final class layout { public static final int widget_small = 11, widget_medium = 12; }
    // 视图
    public static final class id { public static final int widget_root = 21, widget_label = 22, widget_token = 23,
        widget_sync = 24, widget_hooks = 25, widget_agent_1 = 26, widget_agent_2 = 27, widget_agent_3 = 28,
        widget_quota_1 = 29, widget_quota_2 = 30; }
}
''',
    # 渲染检查入口
    'com/manonloki/lokimetrisdisplay/widget_check/WidgetCheck.java': '''package com.manonloki.lokimetrisdisplay.widget_check;
import android.appwidget.AppWidgetManager;
import android.view.View;
import android.widget.RemoteViews;
import com.manonloki.lokimetrisdisplay.widget.LokiWidgetProvider;
import android.widget.RemoteViews;
import com.manonloki.lokimetrisdisplay.R;
// 小组件渲染检查：空态降级、中号渲染与行隐藏
public class WidgetCheck {
    // 等待异步渲染线程推送一次更新
    private static void awaitUpdate(AppWidgetManager manager, int expected) {
        // 有界轮询，避免固定睡眠
        for (int waited = 0; manager.updated.size() < expected && waited < 100; waited++) {
            // 
            try { Thread.sleep(20); } catch (InterruptedException error) { Thread.currentThread().interrupt(); }
        }
        // 超时仍未达到
        if (manager.updated.size() < expected) throw new AssertionError("update not delivered: " + manager.updated.size());
    }

    // 
    public static void main(String[] ignored) {
        // 场景一：夹具核心 + 中号宽度，渲染摘要与明细
        AppWidgetManager manager = AppWidgetManager.getInstance(null);
        // 
        manager.minWidth = 300;
        // 渲染一次
        new LokiWidgetProvider().onUpdate(new android.content.Context(), manager, manager.getAppWidgetIds(null));
        // 
        awaitUpdate(manager, 1);
        // 
        RemoteViews medium = manager.updated.get(0);
        // 中号布局
        if (medium.layout != R.layout.widget_medium) throw new AssertionError("wide instance uses medium layout");
        // Token 大字来自视图投影的紧凑写法
        if (!"4.3k".equals(medium.texts.get(R.id.widget_token))) throw new AssertionError("token: " + medium.texts.get(R.id.widget_token));
        // 紧凑同步行与 Hooks 行来自描述
        if (!"11-03 · 同步 08:00".equals(medium.texts.get(R.id.widget_sync))) throw new AssertionError("sync line");
        if (!"上次 08:00 · Hooks 1".equals(medium.texts.get(R.id.widget_hooks))) throw new AssertionError("hooks line");
        // 明细行：名称加紧凑用量
        if (!"Codex  4.3k".equals(medium.texts.get(R.id.widget_agent_1))) throw new AssertionError("agent 1");
        if (!"Grok  0".equals(medium.texts.get(R.id.widget_agent_2))) throw new AssertionError("agent 2");
        // 额度行：名称加窗口剩余
        if (!"Codex  5 小时 71%".equals(medium.texts.get(R.id.widget_quota_1))) throw new AssertionError("quota 1");
        // 未用的第三行明细隐藏
        if (!Integer.valueOf(View.GONE).equals(medium.visibility.get(R.id.widget_agent_3))) throw new AssertionError("agent 3 hidden");
        // 未用的第二行额度隐藏
        if (!Integer.valueOf(View.GONE).equals(medium.visibility.get(R.id.widget_quota_2))) throw new AssertionError("quota 2 hidden");
        // 场景二：窄宽度选择小号布局，明细区整体隐藏
        // 
        manager.updated.clear(); RemoteViews.all.clear();
        // 
        manager.minWidth = 200;
        // 渲染
        new LokiWidgetProvider().onUpdate(new android.content.Context(), manager, manager.getAppWidgetIds(null));
        // 
        awaitUpdate(manager, 1);
        // 
        RemoteViews small = manager.updated.get(0);
        // 小号布局
        if (small.layout != R.layout.widget_small) throw new AssertionError("narrow instance uses small layout");
        // 明细与额度行全部隐藏
        for (int id : new int[] {R.id.widget_agent_1, R.id.widget_agent_2, R.id.widget_agent_3,
                R.id.widget_quota_1, R.id.widget_quota_2}) {
            // 
            if (!Integer.valueOf(View.GONE).equals(small.visibility.get(id))) throw new AssertionError("row " + id + " hidden");
        }
        // 场景三：核心不可用（无目录、调用失败）渲染空态
        // 
        manager.updated.clear();
        // 
        manager.minWidth = 300;
        // 夹具核心切换为不可用
        com.manonloki.lokimetrisdisplay.widget.WidgetCore.unavailable = true;
        // 渲染
        new LokiWidgetProvider().onUpdate(new android.content.Context(), manager, manager.getAppWidgetIds(null));
        // 
        awaitUpdate(manager, 1);
        // 
        RemoteViews empty = manager.updated.get(0);
        // 空态文案
        if (!"尚未同步".equals(empty.texts.get(R.id.widget_token))) throw new AssertionError("empty token");
        if (!"打开 App 连接桌面".equals(empty.texts.get(R.id.widget_sync))) throw new AssertionError("empty help");
        if (!"诡秘神谕 · 尚未同步".equals(empty.texts.get(R.id.widget_hooks))) throw new AssertionError("empty inline");
        // 场景四：共享目录缺失时不发请求直接渲染空态
        // 
        manager.updated.clear();
        // 
        com.manonloki.lokimetrisdisplay.widget.WidgetCore.missingDir = true;
        com.manonloki.lokimetrisdisplay.widget.WidgetCore.unavailable = false;
        // 渲染
        new LokiWidgetProvider().onUpdate(new android.content.Context(), manager, manager.getAppWidgetIds(null));
        // 
        awaitUpdate(manager, 1);
        // 
        if (manager.updated.get(0).texts.get(R.id.widget_token) == null) throw new AssertionError("missing dir renders empty state");
        System.out.println("widget render: 4 scenarios passed");
    }
}
''',
    # 可切换行为的夹具核心
}


# 夹具核心源码：独立 Java 文件（可被 javac 直接编译，也在 JVM 测试里当作替身）
WIDGET_FIXTURE_CORE = (ROOT / 'src/tests/android/widget_fixture_core.java').read_text()

# 完整替身集合 = Android 接口替身 + JSON 替身 + 夹具核心
DOUBLES_ALL = {**DOUBLES, **JSON_DOUBLE_SOURCES,
    'com/manonloki/lokimetrisdisplay/widget/WidgetCore.java': WIDGET_FIXTURE_CORE}


# 生产小组件渲染的 JVM 测试
class AndroidWidgetRender(unittest.TestCase):
    # 生产提供器按宽度选择布局、渲染摘要并在核心不可用时降级为空态
    def test_production_widget_renders_views_and_degrades_to_empty_state(self):
        # 定位 javac 与 java
        compiler = java_compiler(None)
        java = compiler.with_name('java.exe' if os.name == 'nt' else 'java')
        with tempfile.TemporaryDirectory(prefix='loki-android-widget-') as directory:
            stage = Path(directory)
            # 写出全部替身源码
            for name, source in DOUBLES_ALL.items():
                # 写入
                path = stage / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_text(source)
            # 类输出目录
            classes = stage / 'classes'; classes.mkdir()
            # 被测的生产源码：提供器与视图渲染（WidgetCore 用夹具替身）
            production = [PRODUCTION / 'LokiWidgetProvider.java', PRODUCTION / 'WidgetViews.java']
            # 连同替身一起编译；org.json 来自平台 jar，android.* 以替身源码优先
            compiled = subprocess.run([str(compiler), '--release', '8', '-classpath', str(ANDROID_JAR),
                                       '-d', str(classes),
                                       *[str(path) for path in production],
                                       *[str(stage / name) for name in DOUBLES_ALL]], capture_output=True, text=True)
            # 编译必须成功
            self.assertEqual(compiled.returncode, 0, compiled.stdout + compiled.stderr)
            # 运行渲染检查
            result = subprocess.run([str(java), '-cp', os.pathsep.join([str(classes), str(ANDROID_JAR)]),
                                     'com.manonloki.lokimetrisdisplay.widget_check.WidgetCheck'],
                                    capture_output=True, text=True)
            # 必须全部通过
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            # 四个场景
            self.assertIn('4 scenarios passed', result.stdout)


# 作为脚本运行时
if __name__ == '__main__':
    # 运行全部用例
    unittest.main()
