use super::*;

#[derive(Clone, Debug, PartialEq, Serialize)]
// 设备身份
pub(super) struct Device {
    /// 在连接所有者确认身份之前为空;UI 将其显示为不可用。
    // 设备 ID；尚未确认时为空，界面显示“不可用”
    pub(super) id: String,
    // 设备名称
    pub(super) name: String,
}

// 设备的方法
impl Device {
    // 从存储文本恢复：ID 必须规范，名称不能为空，否则使用默认值
    pub(super) fn restored(raw: Option<&str>) -> Self {
        // 读取存储的原始值
        let value = raw
            // 解析为 JSON
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            // 失败则为 null
            .unwrap_or(Value::Null);
        // 构造设备
        Self {
            // 设备标识
            id: value["id"]
                // 取字符串
                .as_str()
                // 须是合法设备标识
                .filter(|id| valid_device_id(id))
                // 缺省为空
                .unwrap_or_default()
                // 转为字符串
                .into(),
            // 设备名
            name: value["name"]
                // 取字符串
                .as_str()
                // 空名不采用
                .filter(|name| !name.is_empty())
                // 否则用默认名
                .unwrap_or(DEFAULT_NAME)
                // 转为字符串
                .into(),
        }
    }

    // 从连接视图取得身份，不合法则报错
    pub(super) fn projected(view: &Value) -> Result<Self, String> {
        // 读取视图里的设备标识
        let id = view["deviceId"]
            // 取字符串
            .as_str()
            // 须是合法设备标识
            .filter(|id| valid_device_id(id))
            // 否则报设备身份不可用
            .ok_or("设备身份不可用")?;
        // 读取视图里的设备名
        let name = view["deviceName"].as_str().ok_or("设备名称不可用")?;
        // 构造设备
        Ok(Self {
            // 设备标识
            id: id.into(),
            // 设备名
            name: name.into(),
        })
    }

    // 写入存储
    pub(super) fn persist(&self, store: &mut impl Store) -> Result<(), String> {
        // 写入存储
        store.set(
            // 存储键
            DEVICE,
            // 序列化设备
            serde_json::to_string(self).map_err(|error| error.to_string())?,
        )
    }
}
