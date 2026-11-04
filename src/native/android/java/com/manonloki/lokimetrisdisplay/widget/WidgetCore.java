// 小组件包：Java 与 Rust 小组件核心之间唯一的 JSON 通道
package com.manonloki.lokimetrisdisplay.widget;

/** 小组件提供器与 Rust 小组件核心之间唯一的 JSON 通道；请求解析、校验与加密全部在原生核心内完成。
 *  动态库由 DisplayApplication 在进程创建时加载；本类不重复加载，原生生效前调用一律降级为 null。 */
// Java 只做传输与绘制，不复制任何校验或加密规则
public final class WidgetCore {
    // 工具类不实例化
    private WidgetCore() {
    }

    /** 发送一次 JSON 请求并取回 JSON 结果；核心未就绪或失败时返回 null，调用方按未同步处理。 */
    // 原生实现的小组件核心入口
    static String coreCall(String request) {
        // 原生调用失败不能击穿小组件渲染
        try {
            // 委托原生实现
            return nativeCall(request);
        // 原生库未加载或符号缺失
        } catch (Throwable error) {
            // 降级为无结果
            return null;
        }
    }

    /** 返回与主应用连接 owner 相同的规范化小组件共享目录；未就绪时返回 null。 */
    // 原生实现的共享目录查询
    static String widgetDir() {
        // 原生调用失败不能击穿小组件渲染
        try {
            // 委托原生实现
            return nativeWidgetDir();
        // 原生库未加载或符号缺失
        } catch (Throwable error) {
            // 降级为无目录
            return null;
        }
    }

    // 原生核心调用；由 libnsh.so 导出
    private static native String nativeCall(String request);

    // 原生共享目录查询；由 libnsh.so 导出
    private static native String nativeWidgetDir();
}
