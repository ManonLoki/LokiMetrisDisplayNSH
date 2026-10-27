//! Secret Service 的进程边界：凭据通过标准输入传递，不进入命令行参数。
use std::{
    io::{Read, Write},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

fn execute(args: &[&str], input: Option<&str>) -> Result<(ExitStatus, Vec<u8>, Vec<u8>), String> {
    let mut child = Command::new("secret-tool")
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| {
            "无法访问系统 Secret Service；请安装并启用 secret-tool 和会话钥匙串".to_string()
        })?;
    let stdout = child.stdout.take().ok_or("无法读取系统钥匙串")?;
    let stderr = child.stderr.take().ok_or("无法读取系统钥匙串状态")?;
    // Drain bounded output concurrently; a large record must not fill the
    // pipe while the owner is waiting for secret-tool to exit.
    let output = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(4_194_305)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let errors = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.take(8193).read_to_end(&mut bytes).map(|_| bytes)
    });
    let writer = input.map(|value| {
        let bytes = value.as_bytes().to_vec();
        let stdin = child.stdin.take();
        thread::spawn(move || {
            stdin
                .ok_or_else(|| std::io::Error::other("credential input unavailable"))
                .and_then(|mut pipe| pipe.write_all(&bytes))
        })
    });
    let deadline = Instant::now() + Duration::from_secs(15);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break Err("系统钥匙串响应超时或不可用，请解锁后重试");
            }
        }
    };
    let wrote = writer
        .map(|writer| writer.join().ok().is_some_and(|result| result.is_ok()))
        .unwrap_or(true);
    let output = output
        .join()
        .map_err(|_| "读取系统钥匙串失败")?
        .map_err(|_| "读取系统钥匙串失败")?;
    let errors = errors
        .join()
        .map_err(|_| "读取系统钥匙串状态失败")?
        .map_err(|_| "读取系统钥匙串状态失败")?;
    let status = status?;
    if !wrote || output.len() > 4_194_304 || errors.len() > 8192 {
        return Err("系统钥匙串内容超出限制或写入失败".into());
    }
    Ok((status, output, errors))
}

fn lookup(service: &str, account: &str) -> Result<Option<String>, String> {
    let (status, output, errors) =
        execute(&["lookup", "service", service, "account", account], None)?;
    if status.success() && errors.is_empty() {
        // With piped stdout secret-tool emits the exact bytes, with no
        // additional newline. Preserve trailing newlines in the secret.
        String::from_utf8(output)
            .map(Some)
            .map_err(|_| "配对凭据编码无效".into())
    } else if status.code() == Some(1) && errors.is_empty() {
        Ok(None)
    } else {
        Err("系统 Secret Service 不可用或已锁定".into())
    }
}
fn store(service: &str, account: &str, value: &str) -> Result<(), String> {
    if value.len() > 6000 || value.contains('\0') {
        return Err("系统钥匙串分块无效".into());
    }
    let (status, _, errors) = execute(
        &[
            "store",
            "--label=LokiMetris Display 配对",
            "service",
            service,
            "account",
            account,
        ],
        Some(value),
    )?;
    // secret-tool's stdin reader truncates oversized input while store can
    // still exit successfully. Bounded chunks, stderr and an exact readback
    // are all required before committing the root manifest.
    if status.success() && errors.is_empty() && lookup(service, account)?.as_deref() == Some(value)
    {
        Ok(())
    } else {
        Err("系统 Secret Service 不可用或已锁定".into())
    }
}
fn clear(service: &str, account: &str) -> Result<(), String> {
    let (status, _, errors) = execute(&["clear", "service", service, "account", account], None)?;
    if (status.success() || status.code() == Some(1)) && errors.is_empty() {
        Ok(())
    } else {
        Err("删除系统钥匙串凭据失败".into())
    }
}
const PREFIX: &str = "loki-store-v1:";
const DELETED: &str = "loki-delete-v1:";
fn manifest(value: &str) -> Result<(&str, usize), String> {
    let (generation, count) = value.split_once(':').ok_or("系统钥匙串索引无效")?;
    let count: usize = count.parse().map_err(|_| "系统钥匙串索引无效")?;
    if generation.is_empty()
        || !generation.bytes().all(|b| b.is_ascii_hexdigit())
        || !(1..=700).contains(&count)
    {
        return Err("系统钥匙串索引无效".into());
    }
    Ok((generation, count))
}
fn cleanup(service: &str, account: &str, index: &str) -> Result<(), String> {
    let (generation, count) = manifest(index)?;
    for chunk in 0..count {
        clear(service, &format!("{account}/{generation}/{chunk}"))?;
    }
    Ok(())
}
pub fn read(service: &str, account: &str) -> Result<Option<String>, String> {
    let Some(value) = lookup(service, account)? else {
        return Ok(None);
    };
    if let Some(index) = value.strip_prefix(DELETED) {
        cleanup(service, account, index)?;
        clear(service, account)?;
        return Ok(None);
    }
    let Some(index) = value.strip_prefix(PREFIX) else {
        return Ok(Some(value));
    };
    let (generation, count) = manifest(index)?;
    let mut value = String::new();
    for chunk in 0..count {
        let text = lookup(service, &format!("{account}/{generation}/{chunk}"))?
            .ok_or("配对凭据分块缺失")?;
        if text.len() > 6000 || value.len() + text.len() > 4_194_304 {
            return Err("配对凭据长度无效".into());
        }
        value.push_str(&text);
    }
    Ok(Some(value))
}
pub fn write(service: &str, account: &str, value: &str) -> Result<(), String> {
    if value.len() > 4_194_304 || value.contains('\0') {
        return Err("配对凭据长度或内容无效".into());
    }
    let old = lookup(service, account)?;
    if let Some(index) = old.as_ref().and_then(|v| v.strip_prefix(DELETED)) {
        cleanup(service, account, index)?;
        clear(service, account)?;
    }
    let mut chunks = Vec::new();
    let mut offset = 0;
    while offset < value.len() {
        let mut end = (offset + 6000).min(value.len());
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        chunks.push(&value[offset..end]);
        offset = end;
    }
    if chunks.is_empty() {
        chunks.push("");
    }
    let generation = format!(
        "{:x}{:x}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "系统时间无效")?
            .as_nanos()
    );
    let index = format!("{generation}:{}", chunks.len());
    for (chunk, text) in chunks.iter().enumerate() {
        if let Err(error) = store(service, &format!("{account}/{generation}/{chunk}"), text) {
            let _ = cleanup(service, account, &index);
            return Err(error);
        }
    }
    if let Err(error) = store(service, account, &format!("{PREFIX}{index}")) {
        // An external store can mutate its item and then report a warning
        // or lose the reply. Restore the old pointer before deleting staged
        // chunks; if rollback is uncertain, retain both generations so an
        // existing pointer can never refer to chunks we just removed.
        let rollback = match old.as_deref() {
            Some(previous) => store(service, account, previous),
            None => clear(service, account),
        };
        if rollback.is_ok() {
            let _ = cleanup(service, account, &index);
        }
        return Err(error);
    }
    if let Some(index) = old.as_ref().and_then(|v| v.strip_prefix(PREFIX)) {
        let _ = cleanup(service, account, index);
    }
    Ok(())
}
pub fn delete(service: &str, account: &str) -> Result<(), String> {
    if let Some(value) = lookup(service, account)?
        && let Some(index) = value
            .strip_prefix(PREFIX)
            .or_else(|| value.strip_prefix(DELETED))
    {
        if value.starts_with(PREFIX) {
            store(service, account, &format!("{DELETED}{index}"))?;
        }
        cleanup(service, account, index)?;
    }
    clear(service, account)
}
