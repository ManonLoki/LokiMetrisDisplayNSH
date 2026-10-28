/* 在 UIApplication 完成启动之前注册后台处理器。 */
// 由 Swift 垫片导出的注册函数
extern void loki_widget_host_bootstrap(void);

// 构造函数属性：在 main 之前由加载器自动调用
__attribute__((constructor))
// 在 UIApplication 完成启动前就注册好后台处理器
static void loki_widget_host_initialize(void) {
    // 调用小组件宿主引导
    loki_widget_host_bootstrap();
}
