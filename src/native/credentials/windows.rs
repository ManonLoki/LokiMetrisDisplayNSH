//! Windows Credential Manager 的分块事务；先提交索引，再清理上一代数据。
use std::{ffi::c_void, ptr};
// 通用凭据以当前登录的 Windows 用户为作用域。分块
// 让完整的记录集保持在 Credential Manager 2560 字节的条目
// 上限之内;只有在所有分块都写入成功之后,代次指针
// 才会提交新的记录集。
#[repr(C)]
// Windows CREDENTIALW 结构
struct Credential {
    // CREDENTIALW 的字段：标志
    flags: u32,
    // 类型（1 为通用凭据）
    kind: u32,
    target: *mut u16,
    comment: *mut u16,
    last_written: [u32; 2],
    size: u32,
    blob: *mut u8,
    // 持久化范围（2 为本机）
    persist: u32,
    attributes_count: u32,
    attributes: *mut c_void,
    alias: *mut u16,
    username: *mut u16,
}
#[link(name = "Advapi32")]
// Advapi32 的凭据接口
unsafe extern "system" {
    fn CredReadW(
        target: *const u16,
        kind: u32,
        flags: u32,
        // 输出：系统分配的凭据结构
        credential: *mut *mut Credential,
    ) -> i32;
    fn CredWriteW(credential: *const Credential, flags: u32) -> i32;
    fn CredDeleteW(target: *const u16, kind: u32, flags: u32) -> i32;
    // 释放系统分配的缓冲区
    fn CredFree(buffer: *mut c_void);
}
#[link(name = "Kernel32")]
unsafe extern "system" {
    fn GetLastError() -> u32;
}
// 每块 2400 字节，低于单条 2560 字节的限制
const CHUNK: usize = 2400;
const MAX_CHUNKS: usize = 1748;
// 转为以 0 结尾的 UTF-16
fn wide(value: &str) -> Vec<u16> {
    // 转为以零结尾的 UTF-16
    value.encode_utf16().chain(Some(0)).collect()
}
// 凭据目标名：服务/账户
fn target(service: &str, account: &str) -> String {
    // 目标名：服务加账户
    format!("{service}/{account}")
}
// 读取一个凭据；1168（不存在）返回 None
fn get(name: &str) -> Result<Option<Vec<u8>>, String> {
    let name = wide(name);
    // 接收系统分配的结构
    let mut raw = ptr::null_mut();
    if unsafe { CredReadW(name.as_ptr(), 1, 0, &mut raw) } == 0 {
        // 1168 = 不存在
        return if unsafe { GetLastError() } == 1168 {
            Ok(None)
        // 否则分支
        } else {
            Err("读取 Windows 凭据管理器失败".into())
        };
    }
    if raw.is_null() {
        return Err("Windows 凭据管理器返回无效内容".into());
    }
    // 凭据在 `CredFree` 之前就被复制出来;之后没有任何东西再借用 `raw`。
    // 在 CredFree 之前先把内容复制出来，之后不再借用 raw
    unsafe {
        let credential = &*raw;
        // 长度超限或指针为空视为无效
        let result = if credential.size > 2560 || (credential.size > 0 && credential.blob.is_null())
        {
            // 长度超过 2560 或指针为空
            Err("Windows 凭据长度无效".into())
        } else if credential.size == 0 {
            Ok(Some(Vec::new()))
        // 否则分支
        } else {
            // 否则复制数据
            Ok(Some(
                std::slice::from_raw_parts(credential.blob, credential.size as usize).to_vec(),
            ))
        };
        // 释放系统分配的缓冲区
        CredFree(raw.cast());
        result
    }
}
// 写入一个凭据（通用类型，本机持久）
fn put(name: &str, bytes: &[u8]) -> Result<(), String> {
    let mut name = wide(name);
    let credential = Credential {
        flags: 0,
        kind: 1,
        target: name.as_mut_ptr(),
        comment: ptr::null_mut(),
        last_written: [0; 2],
        size: bytes.len() as u32,
        blob: bytes.as_ptr().cast_mut(),
        persist: 2,
        attributes_count: 0,
        attributes: ptr::null_mut(),
        alias: ptr::null_mut(),
        username: ptr::null_mut(),
    };
    if unsafe { CredWriteW(&credential, 0) } != 0 {
        Ok(())
    // 否则分支
    } else {
        Err("写入 Windows 凭据管理器失败".into())
    }
}
// 删除一个凭据；不存在也视为成功
fn remove(name: &str) -> Result<(), String> {
    let name = wide(name);
    // 删除成功或本就不存在
    if unsafe { CredDeleteW(name.as_ptr(), 1, 0) } != 0 || unsafe { GetLastError() } == 1168 {
        Ok(())
    // 否则分支
    } else {
        Err("删除 Windows 配对凭据失败".into())
    }
}
// 解析索引 "代:分块数"
fn manifest(bytes: &[u8]) -> Result<(&str, usize), String> {
    // 索引必须是 UTF-8
    let text = std::str::from_utf8(bytes).map_err(|_| "Windows 凭据索引无效")?;
    let (generation, count) = text.split_once(':').ok_or("Windows 凭据索引无效")?;
    // 分块数解析失败则报索引无效
    let count: usize = count.parse().map_err(|_| "Windows 凭据索引无效")?;
    // 代号须为十六进制，分块数在范围内
    if generation.is_empty()
        // 或者代号含非十六进制字符
        || !generation.bytes().all(|b| b.is_ascii_hexdigit())
        // 或者分块数超出范围
        || !(1..=MAX_CHUNKS).contains(&count)
    {
        return Err("Windows 凭据索引无效".into());
    }
    Ok((generation, count))
}
// 清除某一代的全部分块
fn clear_generation(name: &str, bytes: &[u8]) -> Result<(), String> {
    let (generation, count) = manifest(bytes)?;
    for index in 0..count {
        remove(&format!("{name}/{generation}/{index}"))?;
    }
    Ok(())
}
// 读取：根条目是索引或墓碑
pub fn read(service: &str, account: &str) -> Result<Option<String>, String> {
    let name = target(service, account);
    let Some(index) = get(&name)? else {
        return Ok(None);
    };
    // 墓碑：完成未完成的清理并视为不存在
    if let Some(deleting) = index.strip_prefix(b"deleted:") {
        // 墓碑：完成清理并视为不存在
        clear_generation(&name, deleting)?;
        remove(&name)?;
        return Ok(None);
    }
    let (generation, count) = manifest(&index)?;
    // 依次读出分块并拼接
    let mut bytes = Vec::new();
    for index in 0..count {
        bytes
            .extend(get(&format!("{name}/{generation}/{index}"))?.ok_or("Windows 配对凭据不完整")?);
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| "配对凭据编码无效".into())
}
// 分块事务写入：先写新一代，再提交索引，最后清理旧一代
pub fn write(service: &str, account: &str, value: &str) -> Result<(), String> {
    let name = target(service, account);
    let old = get(&name)?;
    // 旧条目是墓碑则先清理
    if let Some(deleting) = old.as_ref().and_then(|old| old.strip_prefix(b"deleted:")) {
        // 旧条目是墓碑则先清理
        clear_generation(&name, deleting)?;
        remove(&name)?;
    }
    let count = value.len().max(1).div_ceil(CHUNK);
    if count > MAX_CHUNKS {
        return Err("配对凭据过大".into());
    }
    // 用进程号与纳秒时间生成代号
    let generation = format!(
        // 代号 = 进程号 + 纳秒时间
        "{:x}{:x}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "系统时间无效")?
            .as_nanos()
    );
    let index = format!("{generation}:{count}");
    // 写入各分块，失败则清理本代
    for chunk in 0..count {
        let begin = (chunk * CHUNK).min(value.len());
        let end = (begin + CHUNK).min(value.len());
        // 写入分块，失败则清理本代
        if let Err(error) = put(
            &format!("{name}/{generation}/{chunk}"),
            &value.as_bytes()[begin..end],
            // 失败则进入错误处理
        ) {
            let _ = clear_generation(&name, index.as_bytes());
            return Err(error);
        }
    }
    // 提交索引，失败则清理本代
    if let Err(error) = put(&name, index.as_bytes()) {
        // 提交索引失败：同样清理
        let _ = clear_generation(&name, index.as_bytes());
        return Err(error);
    }
    // 提交成功。旧分块不再是应用使用的凭据。
    // 提交成功后旧分块不再被应用使用，清理之
    if let Some(old) = old.filter(|old| !old.starts_with(b"deleted:")) {
        // 提交成功后清理旧分块
        let _ = clear_generation(&name, &old);
    }
    Ok(())
}
// 删除：先写墓碑，再清分块，最后清根条目
pub fn delete(service: &str, account: &str) -> Result<(), String> {
    let name = target(service, account);
    if let Some(index) = get(&name)? {
        // 取出索引内容（去掉墓碑前缀）
        let payload = index.strip_prefix(b"deleted:").unwrap_or(&index);
        // 墓碑记录在部分删除后依然保留。下一次读取会继续
        // 清理,而不是把缺失的分块解释为损坏的密钥。
        // 墓碑能撑过部分删除：下次读取会接着清理，而不是把缺失的分块当成损坏的密钥
        if !index.starts_with(b"deleted:") {
            // 先写墓碑，保证中途失败可继续
            let mut tombstone = b"deleted:".to_vec();
            tombstone.extend_from_slice(payload);
            put(&name, &tombstone)?;
        }
        clear_generation(&name, payload)?;
    }
    remove(&name)
}
