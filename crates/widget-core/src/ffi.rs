//! WidgetKit 与主 App 后台任务的唯一 Swift 入口：一个 JSON 请求换一个 JSON 结果。
//! Swift 经 C ABI 调用 [`call`]，只做网络传输、文件目录定位与绘制；主 App 的
//! 连接 owner 直接使用 Rust API，不经过这里。
use crate::{store::Store, summary, view, widget_http};
use serde_json::{Value, json};
use std::ffi::{CStr, CString, c_char};

/// `{"op": ..., ...}` → `{"ok": value}` 或 `{"error": message}`。
pub fn call(request: &str) -> String {
    let result = serde_json::from_str::<Value>(request)
        .map_err(|_| "请求不是 JSON".to_owned())
        .and_then(|request| dispatch(&request));
    match result {
        Ok(value) => json!({ "ok": value }),
        Err(error) => json!({ "error": error }),
    }
    .to_string()
}

fn dispatch(request: &Value) -> Result<Value, String> {
    let text = |key: &str| request[key].as_str().ok_or_else(|| format!("缺少 {key}"));
    let number = |key: &str| request[key].as_i64().ok_or_else(|| format!("缺少 {key}"));
    let store = || text("dir").map(Store::new);
    match text("op")? {
        "view.describe" => Ok(view::describe(request)),
        "store.load" => {
            let now = number("nowMs")?;
            Ok(store()?.load_cached(now)?.map_or(Value::Null, |record| {
                json!({
                    "receivedAtMs": record["receivedAtMs"], "view": view::project(&record["snapshot"]),
                })
            }))
        }
        // 租约内的摘要无需重复请求；无凭据时返回空，调用方据此记录未授权。
        "widget.begin" => {
            let store = store()?;
            let now = number("nowMs")?;
            if store
                .load()?
                .is_some_and(|record| summary::is_live(&record, now))
            {
                return Ok(json!({ "skip": "live" }));
            }
            let Some(target) = store.load_target()? else {
                return Ok(json!({ "skip": "missingTarget" }));
            };
            let request = widget_http::request(&target, now).ok_or("小组件读取凭据无效")?;
            Ok(json!({ "target": target, "request": request }))
        }
        // 结果写入前再核对凭据代次；请求期间主 App 写入的较新摘要同样算成功。
        "widget.finish" => {
            let store = store()?;
            let target = &request["target"];
            let body = text("body")?;
            let status = request["status"].as_u64().unwrap_or(0);
            let record = widget_http::response(
                target,
                &request["request"],
                status,
                request["mediaType"].as_str(),
                body.as_bytes(),
            )?;
            let started = request["request"]["startedAtMs"].as_i64().unwrap_or(-1);
            let saved = store.save_fetched(&record, target, started)?;
            let newer = !saved
                && store.load_target()?.as_ref() == Some(target)
                && store.load()?.as_ref().and_then(summary::stamp).is_some_and(
                    |(source, received)| {
                        Some(source) == target["sourceId"].as_str()
                            && Some(received) >= record["receivedAtMs"].as_i64()
                    },
                );
            Ok(json!({ "saved": saved, "newerCache": newer,
                "outcome": if saved { "success" } else if newer { "newerCache" } else { "superseded" } }))
        }
        "platform.refresh.limits" => Ok(json!({"interval":900})),
        "platform.refresh.diagnosticOrigin" => {
            let value = request["value"].as_str().unwrap_or_default();
            Ok(json!(if (1..=48).contains(&value.len())
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            {
                value
            } else {
                "unknown"
            }))
        }
        _ => Err("不支持的小组件操作".into()),
    }
}

/// # Safety
/// `request` 必须是有效的 NUL 结尾 UTF-8；返回值交给 [`loki_core_free`] 释放。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn loki_core_call(request: *const c_char) -> *mut c_char {
    let request = if request.is_null() {
        ""
    } else {
        // SAFETY: the C caller guarantees a live NUL-terminated input.
        unsafe { CStr::from_ptr(request) }.to_str().unwrap_or("")
    };
    CString::new(call(request)).map_or(std::ptr::null_mut(), CString::into_raw)
}

/// # Safety
/// 只接受 [`loki_core_call`] 返回且尚未释放的指针。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn loki_core_free(value: *mut c_char) {
    if !value.is_null() {
        // SAFETY: the caller returns this allocation exactly once.
        drop(unsafe { CString::from_raw(value) });
    }
}
