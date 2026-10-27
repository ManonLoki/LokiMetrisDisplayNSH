//! JS 与业务模块之间的序列化端口；不拥有第二份规则或协议状态。
use nsh_core::prelude::*;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Request {
    op: String,
    #[serde(default)]
    args: Vec<Value>,
}

#[module]
/// Business rules executed in Rust for every NSH host; TS only passes arguments.
pub struct Rules;

#[module]
impl Rules {
    /// 参数和返回值均为公开投影；具体规则按业务所属模块分派。
    pub fn call(request: String) -> Result<String, String> {
        if request.len() > 16 * 1024 * 1024 {
            return Err("展示调用过大".into());
        }
        let request: Request = serde_json::from_str(&request).map_err(|_| "展示调用格式无效")?;
        let value = crate::shared::dispatch(&request.op, &request.args)
            .or_else(|| crate::features::connection::dispatch(&request.op, &request.args))
            .or_else(|| crate::features::hooks::dispatch(&request.op, &request.args))
            .or_else(|| crate::features::usage::dispatch(&request.op, &request.args))
            .ok_or("不支持的展示调用")??;
        serde_json::to_string(&value).map_err(|_| "展示结果编码失败".into())
    }
}
