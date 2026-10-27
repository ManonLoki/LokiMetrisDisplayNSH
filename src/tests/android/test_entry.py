"""Execute the production Activity against host-JVM Android/NSH API doubles."""
from pathlib import Path
import os
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools/build'))
from build_android_bridge import java_compiler


SOURCES = {
    'android/app/Activity.java': '''package android.app;
public class Activity {
    public static final android.view.Window window = new android.view.Window();
    protected void onCreate(android.os.Bundle state) {}
    public void onConfigurationChanged(android.content.res.Configuration config) {}
    public boolean dispatchKeyEvent(android.view.KeyEvent event) { return true; }
    public void onBackPressed() {}
    protected void onDestroy() {}
    public void setContentView(Object view) {}
    public android.view.Window getWindow() { return window; }
    public android.content.pm.PackageManager getPackageManager() { return new android.content.pm.PackageManager(); }
    public String getPackageName() { return "org.example.validation"; }
}''',
    'android/content/pm/ApplicationInfo.java': '''package android.content.pm;
public class ApplicationInfo { public android.os.Bundle metaData; }''',
    'android/content/pm/PackageManager.java': '''package android.content.pm;
public class PackageManager {
    public static final int GET_META_DATA = 128;
    public static ApplicationInfo info;
    public static boolean missing;
    public ApplicationInfo getApplicationInfo(String name, int flags) throws NameNotFoundException {
        if (missing) throw new NameNotFoundException();
        if (flags != GET_META_DATA) throw new AssertionError("metadata not requested");
        return info;
    }
    public static class NameNotFoundException extends Exception {}
}''',
    'android/content/res/Configuration.java': 'package android.content.res; public class Configuration {}',
    'android/graphics/Color.java': 'package android.graphics; public class Color { public static final int TRANSPARENT = 0; }',
    'android/os/Build.java': 'package android.os; public class Build { public static class VERSION { public static int SDK_INT = 35; } }',
    'android/os/Bundle.java': '''package android.os;
public class Bundle {
    private final java.util.Map<String, Boolean> values = new java.util.HashMap<>();
    public void putBoolean(String key, boolean value) { values.put(key, value); }
    public boolean getBoolean(String key, boolean fallback) { return values.containsKey(key) ? values.get(key) : fallback; }
}''',
    'android/view/View.java': '''package android.view;
public class View {
    public static final int SYSTEM_UI_FLAG_LAYOUT_STABLE = 1, SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN = 2,
        SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION = 4, SYSTEM_UI_FLAG_LIGHT_STATUS_BAR = 8,
        SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR = 16;
    public int flags = 24;
    public void setSystemUiVisibility(int value) { flags = value; }
    public int getSystemUiVisibility() { return flags; }
}''',
    'android/view/Window.java': '''package android.view;
public class Window {
    public final View decor = new View();
    public final WindowInsetsController controller = new WindowInsetsController();
    public void setDecorFitsSystemWindows(boolean value) {}
    public View getDecorView() { return decor; }
    public void setStatusBarColor(int color) {}
    public void setNavigationBarColor(int color) {}
    public WindowInsetsController getInsetsController() { return controller; }
}''',
    'android/view/WindowInsetsController.java': '''package android.view;
public class WindowInsetsController {
    public static final int APPEARANCE_LIGHT_STATUS_BARS = 1, APPEARANCE_LIGHT_NAVIGATION_BARS = 2;
    public int appearance = 3;
    public void setSystemBarsAppearance(int value, int mask) { appearance = (appearance & ~mask) | (value & mask); }
}''',
    'android/view/KeyEvent.java': '''package android.view;
public class KeyEvent {
    public static final int KEYCODE_DPAD_CENTER = 23, KEYCODE_BUTTON_A = 96, KEYCODE_ENTER = 66;
    public KeyEvent(long down, long time, int action, int key, int repeat, int meta, int device, int scan, int flags, int source) {}
    public long getDownTime() { return 0; } public long getEventTime() { return 0; }
    public int getAction() { return 0; } public int getKeyCode() { return 0; }
    public int getRepeatCount() { return 0; } public int getMetaState() { return 0; }
    public int getDeviceId() { return 0; } public int getScanCode() { return 0; }
    public int getFlags() { return 0; } public int getSource() { return 0; }
}''',
    'com/longbridge/nsh/NshView.java': '''package com.longbridge.nsh;
public class NshView {
    public static int rustCalls, bundleCalls;
    public NshView(Object context) {}
    public void runRustApp(String name) { if (!name.equals("nsh")) throw new AssertionError(name); rustCalls++; }
    public boolean loadAssetBundle(String name) { if (!name.equals("nsh")) throw new AssertionError(name); bundleCalls++; return true; }
    public boolean handleBack() { return false; }
    public void destroy() {}
}''',
    'com/manonloki/lokimetrisdisplay/bridge/EntryCheck.java': '''package com.manonloki.lokimetrisdisplay.bridge;
import android.app.Activity;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageManager;
import android.os.Build;
import android.os.Bundle;
import com.longbridge.nsh.NshView;
public class EntryCheck {
    public static void main(String[] ignored) {
        int cases = 0;
        for (int sdk : new int[] {28, 35}) {
            for (Boolean marker : new Boolean[] {null, false, true}) { run(sdk, marker, false); cases++; }
            run(sdk, true, true); cases++;
        }
        System.out.println("Production Activity entry and light system-bar foreground: " + cases + " cases passed");
    }
    private static void run(int sdk, Boolean marker, boolean missing) {
        Build.VERSION.SDK_INT = sdk;
        PackageManager.info = new ApplicationInfo();
        PackageManager.missing = missing;
        if (marker != null) {
            PackageManager.info.metaData = new Bundle();
            PackageManager.info.metaData.putBoolean("com.longbridge.nsh.RUST", marker);
        }
        NshView.rustCalls = 0; NshView.bundleCalls = 0;
        Activity.window.decor.flags = 24; Activity.window.controller.appearance = 3;
        new DisplayActivity().onCreate(new Bundle());
        boolean rust = Boolean.TRUE.equals(marker) && !missing;
        if (NshView.rustCalls != (rust ? 1 : 0) || NshView.bundleCalls != (rust ? 0 : 1))
            throw new AssertionError("wrong entry: sdk=" + sdk + " RUST=" + marker + " missing=" + missing);
        if (sdk >= 30 ? Activity.window.controller.appearance != 0 : (Activity.window.decor.flags & 24) != 0)
            throw new AssertionError("dark product requires light system-bar foreground");
    }
}''',
}


class AndroidApplicationEntry(unittest.TestCase):
    def test_production_activity_selects_manifest_entry_and_preserves_light_system_bars(self):
        compiler = java_compiler(None)
        java = compiler.with_name('java.exe' if os.name == 'nt' else 'java')
        with tempfile.TemporaryDirectory(prefix='loki-android-entry-') as directory:
            stage = Path(directory)
            for name, source in SOURCES.items():
                path = stage / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_text(source)
            classes = stage / 'classes'; classes.mkdir()
            production = ROOT / 'src/native/android/java/com/manonloki/lokimetrisdisplay/bridge/DisplayActivity.java'
            compiled = subprocess.run([str(compiler), '--release', '8', '-d', str(classes), str(production),
                                       *[str(stage / name) for name in SOURCES]], capture_output=True, text=True)
            self.assertEqual(compiled.returncode, 0, compiled.stdout + compiled.stderr)
            result = subprocess.run([str(java), '-cp', str(classes),
                                     'com.manonloki.lokimetrisdisplay.bridge.EntryCheck'], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn('8 cases passed', result.stdout)


if __name__ == '__main__':
    unittest.main()
