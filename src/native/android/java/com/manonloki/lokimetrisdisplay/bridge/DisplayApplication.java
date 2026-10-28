// 桥接包
package com.manonloki.lokimetrisdisplay.bridge;

// 引入应用与上下文类型
import android.app.Application;
// 上下文
import android.content.Context;

/** 在官方 NSH 视图启动应用之前,持有进程上下文。 */
// 在官方 NSH 视图启动应用之前持有进程级上下文
public final class DisplayApplication extends Application {
    // 重写父类方法
    @Override
    // 进程创建时调用
    public void onCreate() {
        // 调用父类创建
        super.onCreate();
        // 加载包含 Rust 应用的原生库
        System.loadLibrary("nsh");
        // 把应用上下文交给 Rust 侧保存
        nativeInit(this);
    }

    // 由 Rust 实现的 JNI 入口
    private static native void nativeInit(Context application);
}
