// 桥接包：窗口与输入适配
package com.manonloki.lokimetrisdisplay.bridge;

// 引入 Android 的活动、权限、窗口与按键相关类型
import android.app.Activity;
// 应用信息
import android.content.pm.ApplicationInfo;
// 包管理器
import android.content.pm.PackageManager;
// 配置
import android.content.res.Configuration;
// 颜色
import android.graphics.Color;
// 系统版本信息
import android.os.Build;
// 数据包
import android.os.Bundle;
// 按键事件
import android.view.KeyEvent;
// 视图
import android.view.View;
// 窗口
import android.view.Window;
// 窗口边衬控制器
import android.view.WindowInsetsController;
// 引入官方 NSH 视图
import com.longbridge.nsh.NshView;

/** 窗口与输入适配器;官方 NshView 持有全部应用 UI。 */
// 窗口与输入适配；界面全部由官方 NshView 负责
public final class DisplayActivity extends Activity {
    // 承载应用界面的视图
    private NshView view;

    // 重写父类方法
    @Override
    // 创建活动：配置窗口、挂载视图并启动应用
    protected void onCreate(Bundle state) {
        // 调用父类创建
        super.onCreate(state);
        // 设置沉浸式窗口与透明系统栏
        configureWindow();
        // 创建 NSH 视图
        view = new NshView(this);
        // 设为内容视图
        setContentView(view);
        // 带 Rust 模块的应用从原生入口启动，否则加载资源包
        if (rustApp()) view.runRustApp("nsh");
        // 否则加载资源包
        else view.loadAssetBundle("nsh");
    }

    /** TSX 应用可以注册 Rust 模块,而不必是纯 Rust UI 应用。 */
    // TSX 应用可以注册 Rust 模块而不必是纯 Rust 界面应用
    private boolean rustApp() {
        // 从清单元数据读取标志
        try {
            // 读取应用元数据
            ApplicationInfo info = getPackageManager().getApplicationInfo(getPackageName(), PackageManager.GET_META_DATA);
            // 元数据声明使用 Rust 入口则为真
            return info.metaData != null && info.metaData.getBoolean("com.longbridge.nsh.RUST", false);
        // 找不到包信息则视为非 Rust 应用
        } catch (PackageManager.NameNotFoundException error) {
            // 找不到包信息则按否处理
            return false;
        }
    }

    // 重写父类方法
    @Override
    // 把遥控器的确认键与手柄 A 键映射为回车
    public boolean dispatchKeyEvent(KeyEvent event) {
        // 按键码
        int key = event.getKeyCode();
        // 确认键与 A 键
        if (key == KeyEvent.KEYCODE_DPAD_CENTER || key == KeyEvent.KEYCODE_BUTTON_A) {
            // 构造等价的回车事件，保留时间与设备信息
            event = new KeyEvent(event.getDownTime(), event.getEventTime(), event.getAction(),
                    // 按键码、重复次数与元状态
                    KeyEvent.KEYCODE_ENTER, event.getRepeatCount(), event.getMetaState(),
                    // 设备、扫描码与标志
                    event.getDeviceId(), event.getScanCode(), event.getFlags(), event.getSource());
        }
        // 其余交给父类
        return super.dispatchKeyEvent(event);
    }

    // 重写父类方法
    @Override
    // 配置变化（如深浅色切换）后重设系统栏外观
    public void onConfigurationChanged(Configuration configuration) {
        // 调用父类处理
        super.onConfigurationChanged(configuration);
        // 重新配置系统栏
        configureSystemBars();
    }

    // 重写父类方法
    @Override
    // 使用已弃用接口以兼容旧系统
    @SuppressWarnings("deprecation")
    // 返回键：先让 NSH 视图处理（如关闭对话框），未处理才交给系统
    public void onBackPressed() {
        // 视图未处理返回键时交给父类
        if (view == null || !view.handleBack()) super.onBackPressed();
    }

    // 重写父类方法
    @Override
    // 销毁时释放视图
    protected void onDestroy() {
        // 销毁视图
        if (view != null) view.destroy();
        // 调用父类销毁
        super.onDestroy();
    }

    // 使用已弃用接口以兼容旧系统
    @SuppressWarnings("deprecation")
    // 窗口配置：内容延伸到系统栏下方，系统栏透明
    private void configureWindow() {
        // 取窗口
        Window window = getWindow();
        // Android 11 及以上使用新 API
        if (Build.VERSION.SDK_INT >= 30) {
            // 内容延伸到系统栏下
            window.setDecorFitsSystemWindows(false);
        // 旧版本用 systemUiVisibility 标志实现同样效果
        } else {
            // 旧系统用标志位实现稳定布局
            window.getDecorView().setSystemUiVisibility(View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                    // 稳定布局与全屏布局标志
                    | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION);
        }
        // 状态栏与导航栏透明
        window.setStatusBarColor(Color.TRANSPARENT);
        // 导航栏透明
        window.setNavigationBarColor(Color.TRANSPARENT);
        // 配置系统栏样式
        configureSystemBars();
    }

    // 使用已弃用接口以兼容旧系统
    @SuppressWarnings("deprecation")
    // nsh.json 声明了深色产品与浅色系统栏前景，与系统模式无关
    private void configureSystemBars() {
        // nsh.json 声明了深色产品和浅色系统栏前景,与操作系统模式无关。
        View decor = getWindow().getDecorView();
        // 新系统用窗口边衬控制器
        if (Build.VERSION.SDK_INT >= 30) {
            // 取控制器
            WindowInsetsController controller = getWindow().getInsetsController();
            // 没有则返回
            if (controller == null) return;
            // 清除“浅色栏”标志，使栏上的图标为浅色
            int light = WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS
                    // 浅色导航栏外观
                    | WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS;
            // 清除浅色栏外观
            controller.setSystemBarsAppearance(0, light);
        // 旧版本同理清除浅色标志
        } else {
            // 旧系统的浅色栏标志
            int light = View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR | View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR;
            // 当前标志位
            int flags = decor.getSystemUiVisibility();
            // 清除浅色栏标志
            decor.setSystemUiVisibility(flags & ~light);
        }
    }
}
