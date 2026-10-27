//! Windows Credential Manager 的分块事务；先提交索引，再清理上一代数据。
use std::{ffi::c_void, ptr};
// Generic credentials are scoped to the signed-in Windows user. Chunking
// keeps the complete record set within Credential Manager's 2560-byte item
// limit; a generation pointer commits a new record set only after all its
// chunks were written successfully.
#[repr(C)]
struct Credential {
    flags: u32,
    kind: u32,
    target: *mut u16,
    comment: *mut u16,
    last_written: [u32; 2],
    size: u32,
    blob: *mut u8,
    persist: u32,
    attributes_count: u32,
    attributes: *mut c_void,
    alias: *mut u16,
    username: *mut u16,
}
#[link(name = "Advapi32")]
unsafe extern "system" {
    fn CredReadW(
        target: *const u16,
        kind: u32,
        flags: u32,
        credential: *mut *mut Credential,
    ) -> i32;
    fn CredWriteW(credential: *const Credential, flags: u32) -> i32;
    fn CredDeleteW(target: *const u16, kind: u32, flags: u32) -> i32;
    fn CredFree(buffer: *mut c_void);
}
#[link(name = "Kernel32")]
unsafe extern "system" {
    fn GetLastError() -> u32;
}
const CHUNK: usize = 2400;
const MAX_CHUNKS: usize = 1748;
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
fn target(service: &str, account: &str) -> String {
    format!("{service}/{account}")
}
fn get(name: &str) -> Result<Option<Vec<u8>>, String> {
    let name = wide(name);
    let mut raw = ptr::null_mut();
    if unsafe { CredReadW(name.as_ptr(), 1, 0, &mut raw) } == 0 {
        return if unsafe { GetLastError() } == 1168 {
            Ok(None)
        } else {
            Err("读取 Windows 凭据管理器失败".into())
        };
    }
    if raw.is_null() {
        return Err("Windows 凭据管理器返回无效内容".into());
    }
    // The credential is copied out before `CredFree`; nothing borrows `raw` afterwards.
    unsafe {
        let credential = &*raw;
        let result = if credential.size > 2560 || (credential.size > 0 && credential.blob.is_null())
        {
            Err("Windows 凭据长度无效".into())
        } else if credential.size == 0 {
            Ok(Some(Vec::new()))
        } else {
            Ok(Some(
                std::slice::from_raw_parts(credential.blob, credential.size as usize).to_vec(),
            ))
        };
        CredFree(raw.cast());
        result
    }
}
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
    } else {
        Err("写入 Windows 凭据管理器失败".into())
    }
}
fn remove(name: &str) -> Result<(), String> {
    let name = wide(name);
    if unsafe { CredDeleteW(name.as_ptr(), 1, 0) } != 0 || unsafe { GetLastError() } == 1168 {
        Ok(())
    } else {
        Err("删除 Windows 配对凭据失败".into())
    }
}
fn manifest(bytes: &[u8]) -> Result<(&str, usize), String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "Windows 凭据索引无效")?;
    let (generation, count) = text.split_once(':').ok_or("Windows 凭据索引无效")?;
    let count: usize = count.parse().map_err(|_| "Windows 凭据索引无效")?;
    if generation.is_empty()
        || !generation.bytes().all(|b| b.is_ascii_hexdigit())
        || !(1..=MAX_CHUNKS).contains(&count)
    {
        return Err("Windows 凭据索引无效".into());
    }
    Ok((generation, count))
}
fn clear_generation(name: &str, bytes: &[u8]) -> Result<(), String> {
    let (generation, count) = manifest(bytes)?;
    for index in 0..count {
        remove(&format!("{name}/{generation}/{index}"))?;
    }
    Ok(())
}
pub fn read(service: &str, account: &str) -> Result<Option<String>, String> {
    let name = target(service, account);
    let Some(index) = get(&name)? else {
        return Ok(None);
    };
    if let Some(deleting) = index.strip_prefix(b"deleted:") {
        clear_generation(&name, deleting)?;
        remove(&name)?;
        return Ok(None);
    }
    let (generation, count) = manifest(&index)?;
    let mut bytes = Vec::new();
    for index in 0..count {
        bytes
            .extend(get(&format!("{name}/{generation}/{index}"))?.ok_or("Windows 配对凭据不完整")?);
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| "配对凭据编码无效".into())
}
pub fn write(service: &str, account: &str, value: &str) -> Result<(), String> {
    let name = target(service, account);
    let old = get(&name)?;
    if let Some(deleting) = old.as_ref().and_then(|old| old.strip_prefix(b"deleted:")) {
        clear_generation(&name, deleting)?;
        remove(&name)?;
    }
    let count = value.len().max(1).div_ceil(CHUNK);
    if count > MAX_CHUNKS {
        return Err("配对凭据过大".into());
    }
    let generation = format!(
        "{:x}{:x}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "系统时间无效")?
            .as_nanos()
    );
    let index = format!("{generation}:{count}");
    for chunk in 0..count {
        let begin = (chunk * CHUNK).min(value.len());
        let end = (begin + CHUNK).min(value.len());
        if let Err(error) = put(
            &format!("{name}/{generation}/{chunk}"),
            &value.as_bytes()[begin..end],
        ) {
            let _ = clear_generation(&name, index.as_bytes());
            return Err(error);
        }
    }
    if let Err(error) = put(&name, index.as_bytes()) {
        let _ = clear_generation(&name, index.as_bytes());
        return Err(error);
    }
    // Commit succeeded. Old chunks are no longer credentials used by the app.
    if let Some(old) = old.filter(|old| !old.starts_with(b"deleted:")) {
        let _ = clear_generation(&name, &old);
    }
    Ok(())
}
pub fn delete(service: &str, account: &str) -> Result<(), String> {
    let name = target(service, account);
    if let Some(index) = get(&name)? {
        let payload = index.strip_prefix(b"deleted:").unwrap_or(&index);
        // A tombstone survives partial deletion. The next read resumes
        // cleanup rather than interpreting missing chunks as corrupt keys.
        if !index.starts_with(b"deleted:") {
            let mut tombstone = b"deleted:".to_vec();
            tombstone.extend_from_slice(payload);
            put(&name, &tombstone)?;
        }
        clear_generation(&name, payload)?;
    }
    remove(&name)
}
