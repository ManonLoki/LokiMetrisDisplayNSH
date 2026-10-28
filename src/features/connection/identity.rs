//! 设备身份与名称的唯一规则，含旧版移动端恢复名称的空白语义；UTF-8 字节上限不能用字符数量替代。

// 设备名称的 UTF-8 字节上限
pub(crate) const MAX_NAME_BYTES: usize = 160;
// 默认设备名称
pub(crate) const DEFAULT_NAME: &str = "LokiMetris Display";

/// 规范的小写非零 UUID;配对核心为两端共同持有这条规则。
// 设备 ID 校验由配对核心提供，两端共用
pub(crate) use loki_metris_pairing::valid_device_id;

// 判定名称问题：空白、含控制字符、超长，依次检查
fn issue(value: &str, blank: bool) -> Option<&'static str> {
    // 空白名称
    if blank {
        // 请输入设备名称
        Some("请输入设备名称")
    // 含有 C0 或 C1 控制字符
    } else if value
        // 逐字符
        .chars()
        // 含控制字符
        .any(|ch| ch <= '\u{1f}' || ('\u{7f}'..='\u{9f}').contains(&ch))
    {
        // 不能含控制字符
        Some("设备名称不能包含控制字符")
    // 超过字节上限
    } else if value.len() > MAX_NAME_BYTES {
        // 不能超过 160 个 UTF-8 字节
        Some("设备名称不能超过 160 个 UTF-8 字节")
    // 合法
    } else {
        // 合法
        None
    }
}

/// 校验通过共享 NSH 应用输入的名称。
// 校验共享界面输入的名称，空白按统一的比较空白规则判定
pub(crate) fn name_error(value: &str) -> Option<&'static str> {
    // 按条件给出问题提示
    issue(
        // 值
        value,
        // 值
        value
            // 去掉比较用的空白
            .trim_matches(crate::shared::domain::text::comparison_whitespace)
            // 结果为空
            .is_empty(),
    )
}

#[cfg(any(target_os = "android", test))]
// Kotlin/JVM 的空白判定：含 0x1c 到 0x1f，不含 NEL
fn kotlin_whitespace(ch: char) -> bool {
    // 按 Kotlin 的空白规则判断
    ch.is_whitespace() && ch != '\u{85}' || ('\u{1c}'..='\u{1f}').contains(&ch)
}

/// 保留 Kotlin/JVM 对已保存 Android 名称的空白名称行为。
#[cfg(any(target_os = "android", test))]
// 沿用 Kotlin 的空白语义校验 Android 已保存名称
pub(crate) fn android_name_error(value: &str) -> Option<&'static str> {
    // 去除空白后为空则有问题
    issue(value, value.trim_matches(kotlin_whitespace).is_empty())
}

/// 新装 Android 时,若设备型号是有效名称则使用它。
#[cfg(any(target_os = "android", test))]
// 新安装的 Android 使用设备型号作为名称，非法则回落默认值
pub(crate) fn android_default_name(model: &str) -> String {
    // 先去掉首尾空白
    let model = model.trim_matches(kotlin_whitespace);
    // 型号合法则直接使用
    if android_name_error(model).is_none() {
        // 型号转为字符串
        model.into()
    // 否则分支
    } else {
        // 回落名称
        "Android 显示端".into()
    }
}

/// 保留 Foundation 对已保存 Apple 名称的空白与零宽空格处理行为。
#[cfg(any(target_os = "ios", test))]
// 沿用 Foundation 的空白语义（含零宽空格）校验 Apple 已保存名称
pub(crate) fn apple_name_error(value: &str) -> Option<&'static str> {
    // 按条件给出问题提示
    issue(
        // 值
        value,
        // 值
        value
            // 逐字符
            .chars()
            // 全是空白或零宽空格
            .all(|ch| ch.is_whitespace() || ch == '\u{200b}'),
    )
}

// 测试模块
#[cfg(test)]
#[path = "identity_test.rs"]
mod identity_test;
