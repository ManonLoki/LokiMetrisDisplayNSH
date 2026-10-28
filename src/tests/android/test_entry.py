"""Execute the production Activity against host-JVM Android/NSH API doubles."""
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
# 复用 javac 定位逻辑
from build_android_bridge import java_compiler


# 在宿主 JVM 上用的最小 Android/NSH 接口替身（Java 源码），以及一个入口检查类
SOURCES = {
    'android/app/Activity.java': '''package android.app;
// 最小的 Activity 替身：记录窗口，其余方法为空实现
public class Activity {
    // 
    public static final android.view.Window window = new android.view.Window();
    // 
    protected void onCreate(android.os.Bundle state) {}
    // 
    public void onConfigurationChanged(android.content.res.Configuration config) {}
    // 
    public boolean dispatchKeyEvent(android.view.KeyEvent event) { return true; }
    // 
    public void onBackPressed() {}
    // 
    protected void onDestroy() {}
    // 
    public void setContentView(Object view) {}
    // 
    public android.view.Window getWindow() { return window; }
    // 
    public android.content.pm.PackageManager getPackageManager() { return new android.content.pm.PackageManager(); }
    // 
    public String getPackageName() { return "org.example.validation"; }
}''',
    'android/content/pm/ApplicationInfo.java': '''package android.content.pm;
// 应用信息替身：只有元数据
public class ApplicationInfo { public android.os.Bundle metaData; }''',
    'android/content/pm/PackageManager.java': '''package android.content.pm;
public class PackageManager {
    // 
    public static final int GET_META_DATA = 128;
    // 测试设置的应用信息
    public static ApplicationInfo info;
    // 是否模拟“找不到包”
    public static boolean missing;
    // 
    public ApplicationInfo getApplicationInfo(String name, int flags) throws NameNotFoundException {
        // 
        if (missing) throw new NameNotFoundException();
        // 必须请求元数据
        if (flags != GET_META_DATA) throw new AssertionError("metadata not requested");
        // 
        return info;
    }
    // 
    public static class NameNotFoundException extends Exception {}
}''',
    # 其余最小替身类：配置、颜色、构建信息
    'android/content/res/Configuration.java': 'package android.content.res; public class Configuration {}',
    'android/graphics/Color.java': 'package android.graphics; public class Color { public static final int TRANSPARENT = 0; }',
    'android/os/Build.java': 'package android.os; public class Build { public static class VERSION { public static int SDK_INT = 35; } }',
    'android/os/Bundle.java': '''package android.os;
public class Bundle {
    // Bundle 替身：只存布尔值
    private final java.util.Map<String, Boolean> values = new java.util.HashMap<>();
    // 
    public void putBoolean(String key, boolean value) { values.put(key, value); }
    // 
    public boolean getBoolean(String key, boolean fallback) { return values.containsKey(key) ? values.get(key) : fallback; }
}''',
    'android/view/View.java': '''package android.view;
public class View {
    // View 替身：系统界面标志位
    public static final int SYSTEM_UI_FLAG_LAYOUT_STABLE = 1, SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN = 2,
        SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION = 4, SYSTEM_UI_FLAG_LIGHT_STATUS_BAR = 8,
        SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR = 16;
    // 初始带有两个“浅色栏”标志
    public int flags = 24;
    // 
    public void setSystemUiVisibility(int value) { flags = value; }
    // 
    public int getSystemUiVisibility() { return flags; }
}''',
    'android/view/Window.java': '''package android.view;
public class Window {
    // Window 替身：持有装饰视图与控制器
    public final View decor = new View();
    // 
    public final WindowInsetsController controller = new WindowInsetsController();
    // 
    public void setDecorFitsSystemWindows(boolean value) {}
    // 
    public View getDecorView() { return decor; }
    // 
    public void setStatusBarColor(int color) {}
    // 
    public void setNavigationBarColor(int color) {}
    // 
    public WindowInsetsController getInsetsController() { return controller; }
}''',
    'android/view/WindowInsetsController.java': '''package android.view;
public class WindowInsetsController {
    // 控制器替身：浅色栏外观位
    public static final int APPEARANCE_LIGHT_STATUS_BARS = 1, APPEARANCE_LIGHT_NAVIGATION_BARS = 2;
    // 
    public int appearance = 3;
    // 按掩码更新外观位
    public void setSystemBarsAppearance(int value, int mask) { appearance = (appearance & ~mask) | (value & mask); }
}''',
    'android/view/KeyEvent.java': '''package android.view;
public class KeyEvent {
    // KeyEvent 替身：按键码常量
    public static final int KEYCODE_DPAD_CENTER = 23, KEYCODE_BUTTON_A = 96, KEYCODE_ENTER = 66;
    // 
    public KeyEvent(long down, long time, int action, int key, int repeat, int meta, int device, int scan, int flags, int source) {}
    // 各个取值方法返回 0
    public long getDownTime() { return 0; } public long getEventTime() { return 0; }
    // 
    public int getAction() { return 0; } public int getKeyCode() { return 0; }
    // 
    public int getRepeatCount() { return 0; } public int getMetaState() { return 0; }
    // 
    public int getDeviceId() { return 0; } public int getScanCode() { return 0; }
    // 
    public int getFlags() { return 0; } public int getSource() { return 0; }
}''',
    'com/longbridge/nsh/NshView.java': '''package com.longbridge.nsh;
public class NshView {
    // NshView 替身：记录两种入口各被调用几次
    public static int rustCalls, bundleCalls;
    // 
    public NshView(Object context) {}
    // Rust 入口：名称必须是 nsh
    public void runRustApp(String name) { if (!name.equals("nsh")) throw new AssertionError(name); rustCalls++; }
    // 资源包入口
    public boolean loadAssetBundle(String name) { if (!name.equals("nsh")) throw new AssertionError(name); bundleCalls++; return true; }
    // 
    public boolean handleBack() { return false; }
    // 
    public void destroy() {}
}''',
    'com/manonloki/lokimetrisdisplay/bridge/EntryCheck.java': '''package com.manonloki.lokimetrisdisplay.bridge;
import android.app.Activity;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageManager;
import android.os.Build;
import android.os.Bundle;
import com.longbridge.nsh.NshView;
// 入口检查：遍历 SDK 与元数据标志的组合
public class EntryCheck {
    // 
    public static void main(String[] ignored) {
        // 
        int cases = 0;
        // 两个 SDK 版本
        for (int sdk : new int[] {28, 35}) {
            // 元数据：缺失、假、真
            for (Boolean marker : new Boolean[] {null, false, true}) { run(sdk, marker, false); cases++; }
            // 标志为真但找不到包信息
            run(sdk, true, true); cases++;
        }
        // 
        System.out.println("Production Activity entry and light system-bar foreground: " + cases + " cases passed");
    }
    // 执行一种组合并断言
    private static void run(int sdk, Boolean marker, boolean missing) {
        // 
        Build.VERSION.SDK_INT = sdk;
        // 
        PackageManager.info = new ApplicationInfo();
        // 
        PackageManager.missing = missing;
        // 
        if (marker != null) {
            // 
            PackageManager.info.metaData = new Bundle();
            PackageManager.info.metaData.putBoolean("com.longbridge.nsh.RUST", marker);
        }
        // 重置计数与窗口状态
        NshView.rustCalls = 0; NshView.bundleCalls = 0;
        // 
        Activity.window.decor.flags = 24; Activity.window.controller.appearance = 3;
        // 创建生产环境的 Activity
        new DisplayActivity().onCreate(new Bundle());
        // 只有标志为真且找得到包信息才走 Rust 入口
        boolean rust = Boolean.TRUE.equals(marker) && !missing;
        // 入口必须符合预期
        if (NshView.rustCalls != (rust ? 1 : 0) || NshView.bundleCalls != (rust ? 0 : 1))
            // 
            throw new AssertionError("wrong entry: sdk=" + sdk + " RUST=" + marker + " missing=" + missing);
        // 系统栏的浅色前景标志必须被清除（深色产品需要浅色前景）
        if (sdk >= 30 ? Activity.window.controller.appearance != 0 : (Activity.window.decor.flags & 24) != 0)
            // 
            throw new AssertionError("dark product requires light system-bar foreground");
    }
}''',
}


# 生产环境 Activity 的入口测试
class AndroidApplicationEntry(unittest.TestCase):
    # 生产 Activity 按清单标志选择入口，并保持浅色系统栏前景
    def test_production_activity_selects_manifest_entry_and_preserves_light_system_bars(self):
        # 定位 javac 与 java
        compiler = java_compiler(None)
        java = compiler.with_name('java.exe' if os.name == 'nt' else 'java')
        with tempfile.TemporaryDirectory(prefix='loki-android-entry-') as directory:
            stage = Path(directory)
            # 写出所有替身源码
            for name, source in SOURCES.items():
                path = stage / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_text(source)
            classes = stage / 'classes'; classes.mkdir()
            # 被测的生产 Activity
            production = ROOT / 'src/native/android/java/com/manonloki/lokimetrisdisplay/bridge/DisplayActivity.java'
            # 连同替身一起编译
            compiled = subprocess.run([str(compiler), '--release', '8', '-d', str(classes), str(production),
                                       # 把全部替身源码一起编译
                                       *[str(stage / name) for name in SOURCES]], capture_output=True, text=True)
            self.assertEqual(compiled.returncode, 0, compiled.stdout + compiled.stderr)
            # 运行入口检查类
            result = subprocess.run([str(java), '-cp', str(classes),
                                     # 运行入口检查类
                                     'com.manonloki.lokimetrisdisplay.bridge.EntryCheck'], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            # 应覆盖 8 种组合
            self.assertIn('8 cases passed', result.stdout)


# 作为脚本运行时
if __name__ == '__main__':
    # 运行全部用例
    unittest.main()
