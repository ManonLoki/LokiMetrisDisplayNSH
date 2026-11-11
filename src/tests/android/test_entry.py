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
    public static android.view.View content;
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
    public void setContentView(android.view.View view) { content = view; }
    // 
    public android.view.Window getWindow() { return window; }
}''',
    # 其余最小替身类：配置、颜色、构建信息
    'android/content/res/Configuration.java': 'package android.content.res; public class Configuration {}',
    'android/graphics/Color.java': 'package android.graphics; public class Color { public static final int TRANSPARENT = 0; }',
    'android/os/Build.java': 'package android.os; public class Build { public static class VERSION { public static int SDK_INT = 35; } }',
    'android/os/Bundle.java': 'package android.os; public class Bundle {}',
    'android/view/View.java': '''package android.view;
public class View {
    // View 替身：系统界面标志位
    public static final int SYSTEM_UI_FLAG_LAYOUT_STABLE = 1, SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN = 2,
        SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION = 4, SYSTEM_UI_FLAG_LIGHT_STATUS_BAR = 8,
        SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR = 16;
    // 初始带有两个“浅色栏”标志
    public int flags = 24;
    public interface OnLayoutChangeListener {
        void onLayoutChange(View view, int left, int top, int right, int bottom,
                int oldLeft, int oldTop, int oldRight, int oldBottom);
    }
    private OnLayoutChangeListener layoutListener;
    public void addOnLayoutChangeListener(OnLayoutChangeListener listener) { layoutListener = listener; }
    public void removeOnLayoutChangeListener(OnLayoutChangeListener listener) {
        if (layoutListener == listener) layoutListener = null;
    }
    public void layoutForTest(int width, int height) {
        if (layoutListener != null) layoutListener.onLayoutChange(this, 0, 0, width, height, 0, 0, 0, 0);
    }
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
public class NshView extends android.view.View {
    // NshView 替身：记录资源包加载次数
    public static int bundleCalls;
    // 
    public NshView(Object context) {}
    // 资源包入口
    public boolean loadAssetBundle(String name) { if (!name.equals("nsh")) throw new AssertionError(name); bundleCalls++; return true; }
    // 
    public boolean handleBack() { return false; }
    // 
    public void destroy() {}
}''',
    'com/manonloki/lokimetrisdisplay/bridge/EntryCheck.java': '''package com.manonloki.lokimetrisdisplay.bridge;
import android.app.Activity;
import android.os.Build;
import android.os.Bundle;
import com.longbridge.nsh.NshView;
// 入口检查覆盖旧版与新版 SDK 的系统栏实现
public class EntryCheck {
    public static void main(String[] ignored) {
        run(26);
        run(35);
    }

    private static void run(int sdk) {
        Build.VERSION.SDK_INT = sdk;
        NshView.bundleCalls = 0;
        Activity.window.decor.flags = 24; Activity.window.controller.appearance = 3;
        // 创建生产环境的 Activity
        new DisplayActivity().onCreate(new Bundle());
        Activity.content.layoutForTest(0, 0);
        if (NshView.bundleCalls != 0)
            throw new AssertionError("runner started before the view had its first layout");
        Activity.content.layoutForTest(1280, 720);
        Activity.content.layoutForTest(720, 1280);
        if (NshView.bundleCalls != 1)
            throw new AssertionError("expected one bundle load: sdk=" + sdk);
        // 系统栏的浅色前景标志必须被清除（深色产品需要浅色前景）
        if (sdk >= 30 ? Activity.window.controller.appearance != 0 : (Activity.window.decor.flags & 24) != 0)
            // 
            throw new AssertionError("dark product requires light system-bar foreground");
    }
}''',
}


# 生产环境 Activity 的入口测试
class AndroidApplicationEntry(unittest.TestCase):
    # 生产 Activity 在第一次有效布局后加载 TSX 应用，并保持浅色系统栏前景
    def test_production_activity_loads_bundle_after_layout_and_preserves_light_system_bars(self):
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


# 作为脚本运行时
if __name__ == '__main__':
    # 运行全部用例
    unittest.main()
