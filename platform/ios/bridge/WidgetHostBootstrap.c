/* Register background handlers before UIApplication finishes launching. */
extern void loki_widget_host_bootstrap(void);

__attribute__((constructor))
static void loki_widget_host_initialize(void) {
    loki_widget_host_bootstrap();
}
