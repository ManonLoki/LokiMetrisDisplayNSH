//! WidgetKit 与主 App 后台任务的唯一 Swift 入口：一个 JSON 请求换一个 JSON 结果。
//! Swift 经 C ABI 调用 [`call`]，只做网络传输、文件目录定位与绘制；主 App 的
//! 连接 owner 直接使用 Rust API，不经过这里。
// 引入存储、摘要、视图与 HTTP 协议
use crate::{store::Store, summary, view, widget_http};
// 引入 JSON
use serde_json::{Value, json};
// 引入 C 字符串类型
use std::ffi::{CStr, CString, c_char};

/// `{"op": ..., ...}` → `{"ok": value}` 或 `{"error": message}`。
// 入口：解析 JSON 请求，调用分发，把结果包成 ok 或 error
pub fn call(request: &str) -> String {
    // 解析请求 JSON
    let result = serde_json::from_str::<Value>(request)
        // 请求不是 JSON
        .map_err(|_| "请求不是 JSON".to_owned())
        // 派发请求
        .and_then(|request| dispatch(&request));
    // 按处理结果包装
    match result {
        // 成功：放入 ok 字段
        Ok(value) => json!({ "ok": value }),
        // 失败：放入 error 字段
        Err(error) => json!({ "error": error }),
    }
    // 序列化为 JSON 文本
    .to_string()
}

// 按 op 分发
fn dispatch(request: &Value) -> Result<Value, String> {
    // 读取必需的字符串参数
    let text = |key: &str| request[key].as_str().ok_or_else(|| format!("缺少 {key}"));
    // 读取必需的整数参数
    let number = |key: &str| request[key].as_i64().ok_or_else(|| format!("缺少 {key}"));
    // 按 dir 参数打开存储
    let store = || text("dir").map(Store::new);
    // 按操作名分发
    match text("op")? {
        // 展示模型中的描述文本
        "view.describe" => Ok(view::describe(request)),
        // 读取缓存的摘要并投影为视图
        "store.load" => {
            // 当前时间
            let now = number("nowMs")?;
            // 读取缓存的摘要，没有则为 null
            Ok(store()?.load_cached(now)?.map_or(Value::Null, |record| {
                // 组装展示结果
                json!({
                    // 接收时间与展示视图
                    "receivedAtMs": record["receivedAtMs"], "view": view::project(&record["snapshot"]),
                })
            }))
        }
        // 租约内的摘要无需重复请求；无凭据时返回空，调用方据此记录未授权。
        // 开始一次小组件刷新
        "widget.begin" => {
            // 打开存储
            let store = store()?;
            // 当前时间
            let now = number("nowMs")?;
            // 租约内的摘要无需重复请求
            if store
                // 读取摘要
                .load()?
                // 仍在实时租约内
                .is_some_and(|record| summary::is_live(&record, now))
            {
                // 仍有实时数据：跳过
                return Ok(json!({ "skip": "live" }));
            }
            // 没有凭据则返回空，调用方据此记录未授权
            let Some(target) = store.load_target()? else {
                // 缺少凭据：跳过
                return Ok(json!({ "skip": "missingTarget" }));
            };
            // 构造带认证的请求
            let request = widget_http::request(&target, now).ok_or("小组件读取凭据无效")?;
            // 返回凭据与请求
            Ok(json!({ "target": target, "request": request }))
        }
        // 结果写入前再核对凭据代次；请求期间主 App 写入的较新摘要同样算成功。
        // 完成一次刷新：写入前再核对凭据代次；请求期间主 App 写入的较新摘要同样算成功
        "widget.finish" => {
            // 打开存储
            let store = store()?;
            // 请求里的凭据
            let target = &request["target"];
            // 响应体
            let body = text("body")?;
            // 响应状态码
            let status = request["status"].as_u64().unwrap_or(0);
            // 校验并解析响应
            let record = widget_http::response(
                // 凭据
                target,
                // 请求
                &request["request"],
                // 状态码
                status,
                // 媒体类型
                request["mediaType"].as_str(),
                // 响应体
                body.as_bytes(),
                // 失败即返回
            )?;
            // 请求开始时间
            let started = request["request"]["startedAtMs"].as_i64().unwrap_or(-1);
            // 写入前由存储再次核对凭据代次
            let saved = store.save_fetched(&record, target, started)?;
            // 未写入但存储里已有更新的摘要
            let newer = !saved
                // 凭据与当前存储的一致
                && store.load_target()?.as_ref() == Some(target)
                // 并且当前摘要的来源与时间
                && store.load()?.as_ref().and_then(summary::stamp).is_some_and(
                    // 条件开始
                    |(source, received)| {
                        // 来源与凭据一致
                        Some(source) == target["sourceId"].as_str()
                            // 新摘要不早于当前摘要
                            && Some(received) >= record["receivedAtMs"].as_i64()
                    },
                );
            // 返回写入结果，并给出结果分类
            Ok(json!({ "saved": saved, "newerCache": newer,
                // 成功、已有更新的缓存、被取代三种结果之一
                "outcome": if saved { "success" } else if newer { "newerCache" } else { "superseded" } }))
        }
        // 刷新间隔上限：900 秒
        "platform.refresh.limits" => Ok(json!({"interval":900})),
        // 诊断来源标识：只接受 1 到 48 位字母数字下划线连字符，否则记为 unknown
        "platform.refresh.diagnosticOrigin" => {
            // 取来源标识
            let value = request["value"].as_str().unwrap_or_default();
            // 长度须在 1 到 48
            Ok(json!(if (1..=48).contains(&value.len())
                // 并且
                && value
                    // 按字节
                    .bytes()
                    // 只含字母数字和 _-
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            {
                // 合法的来源标识
                value
            // 不合法的来源标识
            } else {
                // 统一记为 unknown
                "unknown"
            }))
        }
        // 其余操作不受支持
        _ => Err("不支持的小组件操作".into()),
    }
}

/// # Safety
/// `request` 必须是有效的 NUL 结尾 UTF-8；返回值交给 [`loki_core_free`] 释放。
// C ABI 入口：请求须是 NUL 结尾的 UTF-8，返回值交给 loki_core_free 释放
#[unsafe(no_mangle)]
pub unsafe extern "C" fn loki_core_call(request: *const c_char) -> *mut c_char {
    // 请求为 null
    let request = if request.is_null() {
        // 用空串代替
        ""
    // 否则分支
    } else {
        // SAFETY: C 调用方保证传入的是有效且以 NUL 结尾的输入。
        unsafe { CStr::from_ptr(request) }.to_str().unwrap_or("")
    };
    // 把结果交给 C 调用方，失败则返回空指针
    CString::new(call(request)).map_or(std::ptr::null_mut(), CString::into_raw)
}

/// # Safety
/// 只接受 [`loki_core_call`] 返回且尚未释放的指针。
// 释放 loki_core_call 返回的字符串
#[unsafe(no_mangle)]
pub unsafe extern "C" fn loki_core_free(value: *mut c_char) {
    // 非空才处理
    if !value.is_null() {
        // SAFETY: 调用方只会归还这块分配一次。
        drop(unsafe { CString::from_raw(value) });
    }
}
