//! JS 与业务模块之间的序列化端口；不拥有第二份规则或协议状态。
// 引入 nsh 模块宏
use nsh_core::prelude::*;
// 引入反序列化
use serde::Deserialize;
// 引入 JSON 值
use serde_json::Value;

#[derive(Deserialize)]
// 规则请求
struct Request {
    // 操作名
    op: String,
    #[serde(default)]
    // 参数列表，缺省为空
    args: Vec<Value>,
}

// 声明 nsh 原生模块
#[module]
/// 在每个 NSH 宿主上于 Rust 中执行的业务规则;TS 只负责传参。
// 规则模块：所有 NSH 宿主共用的业务规则
pub struct Rules;

// 模块方法
#[module]
impl Rules {
    /// 参数和返回值均为公开投影；具体规则按业务所属模块分派。
    // 唯一入口：参数与返回值都是公开投影，按业务所属模块分派
    pub fn call(request: String) -> Result<String, String> {
        // 请求不得超过 16 MiB
        if request.len() > 16 * 1024 * 1024 {
            // 展示调用过大
            return Err("展示调用过大".into());
        }
        // 解析请求
        let request: Request = serde_json::from_str(&request).map_err(|_| "展示调用格式无效")?;
        // 依次尝试各模块，没有人处理则报错
        let value = crate::shared::dispatch(&request.op, &request.args)
            // 先试连接分支
            .or_else(|| crate::features::connection::dispatch(&request.op, &request.args))
            // 再试 Hook 分支
            .or_else(|| crate::features::hooks::dispatch(&request.op, &request.args))
            // 再试用量分支
            .or_else(|| crate::features::usage::dispatch(&request.op, &request.args))
            // 都不支持则报错
            .ok_or("不支持的展示调用")??;
        // 序列化结果
        serde_json::to_string(&value).map_err(|_| "展示结果编码失败".into())
    }
}
