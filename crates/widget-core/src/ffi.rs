//! 系统扩展唯一入口：一个 JSON 请求换一个 JSON 结果。Swift 经 C ABI、Android 经
//! 宿主 JNI 调用 [`call`]；原生层只做网络传输、文件目录定位与绘制。
use crate::{snapshot, store::Store, summary, view, widget_http};
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
        "view.project" => Ok(view::project(&request["snapshot"])),
        "view.describe" => Ok(view::describe(request)),
        // 完整快照（含正文与图片摘要）的结构校验，与网络入口同一规则。
        "snapshot.valid" => Ok(Value::Bool(
            snapshot::project_snapshot(&request["snapshot"], &Value::Null).is_some(),
        )),
        "target.make" => Ok(widget_http::target(
            text("sourceId")?,
            text("deviceId")?,
            text("host")?,
            request["port"].as_u64().unwrap_or(0),
            text("key")?,
        )
        .unwrap_or(Value::Null)),
        "store.target" => Ok(store()?.load_target()?.unwrap_or(Value::Null)),
        "target.valid" => Ok(Value::Bool(widget_http::valid_target(&request["target"]))),
        "target.same" => Ok(Value::Bool(widget_http::same_connection(
            &request["left"],
            &request["right"],
        ))),
        // 不经共享存储的单次请求（watchOS 自有缓存）。
        "widget.request" => {
            Ok(widget_http::request(&request["target"], number("nowMs")?).unwrap_or(Value::Null))
        }
        "widget.response" => widget_http::response(
            &request["target"],
            &request["request"],
            request["status"].as_u64().unwrap_or(0),
            request["mediaType"].as_str(),
            text("body")?.as_bytes(),
        ),
        "summary.fromMessage" => Ok(summary::from_message(
            &request["message"],
            text("sourceId")?,
            number("receivedAtMs")?,
        )
        .unwrap_or(Value::Null)),
        "widget.target" => Ok(widget_http::target_from_message(
            &request["message"],
            text("sourceId")?,
            text("deviceId")?,
            text("peer")?,
        )
        .unwrap_or(Value::Null)),
        "store.load" => {
            let now = number("nowMs")?;
            Ok(store()?.load_cached(now)?.map_or(Value::Null, |record| {
                json!({
                    "sourceId": record["sourceId"], "receivedAtMs": record["receivedAtMs"],
                    "live": summary::is_live(&record, now), "view": view::project(&record["snapshot"]),
                    "record": record,
                })
            }))
        }
        "store.saveAuthenticated" => {
            let target = Some(&request["target"]).filter(|value| !value.is_null());
            store()?
                .save_authenticated(&request["summary"], target)
                .map(Value::Bool)
        }
        "store.renew" => store()?
            .renew(text("sourceId")?, number("nowMs")?)
            .map(Value::Bool),
        "store.select" => store()?.select(text("sourceId")?).map(|_| Value::Null),
        "store.forget" => store()?.forget(text("sourceId")?).map(|_| Value::Null),
        "store.clear" => store()?.clear().map(|_| Value::Null),
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
        "platform.refresh.limits" => Ok(json!({"interval":900,"backgroundBudget":12})),
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
