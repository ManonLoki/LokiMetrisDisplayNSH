use super::*;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(super) struct Device {
    /// 在连接所有者确认身份之前为空;UI 将其显示为不可用。
    // 设备 ID；尚未确认时为空，界面显示“不可用”
    pub(super) id: String,
    pub(super) name: String,
}

impl Device {
    // 从存储文本恢复：ID 必须规范，名称不能为空，否则使用默认值
    pub(super) fn restored(raw: Option<&str>) -> Self {
        let value = raw
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            // 失败则为 null
            .unwrap_or(Value::Null);
        Self {
            id: value["id"]
                .as_str()
                .filter(|id| valid_device_id(id))
                .unwrap_or_default()
                .into(),
            name: value["name"]
                .as_str()
                .filter(|name| !name.is_empty())
                // 否则用默认名
                .unwrap_or(DEFAULT_NAME)
                .into(),
        }
    }

    // 从连接视图取得身份，不合法则报错
    pub(super) fn projected(view: &Value) -> Result<Self, String> {
        // 读取视图里的设备标识
        let id = view["deviceId"]
            .as_str()
            .filter(|id| valid_device_id(id))
            // 否则报设备身份不可用
            .ok_or("设备身份不可用")?;
        // 读取视图里的设备名
        let name = view["deviceName"].as_str().ok_or("设备名称不可用")?;
        Ok(Self {
            id: id.into(),
            name: name.into(),
        })
    }

    pub(super) fn persist(&self, store: &mut impl Store) -> Result<(), String> {
        store.set(
            DEVICE,
            serde_json::to_string(self).map_err(|error| error.to_string())?,
        )
    }
}
