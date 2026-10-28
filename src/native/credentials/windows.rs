//! Windows Credential Manager 的分块事务；先提交索引，再清理上一代数据。
// 引入 C 类型与指针
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
    // 目标名
    target: *mut u16,
    // 备注
    comment: *mut u16,
    // 最后写入时间
    last_written: [u32; 2],
    // 数据长度
    size: u32,
    // 数据指针
    blob: *mut u8,
    // 持久化范围（2 为本机）
    persist: u32,
    // 属性数量
    attributes_count: u32,
    // 属性指针
    attributes: *mut c_void,
    // 别名
    alias: *mut u16,
    // 用户名
    username: *mut u16,
}
// 凭据管理器接口
#[link(name = "Advapi32")]
// Advapi32 的凭据接口
unsafe extern "system" {
    // 读取
    fn CredReadW(
        // 目标名
        target: *const u16,
        // 类型
        kind: u32,
        // 标志
        flags: u32,
        // 输出：系统分配的凭据结构
        credential: *mut *mut Credential,
        // 返回状态
    ) -> i32;
    // 写入
    fn CredWriteW(credential: *const Credential, flags: u32) -> i32;
    // 删除
    fn CredDeleteW(target: *const u16, kind: u32, flags: u32) -> i32;
    // 释放系统分配的缓冲区
    fn CredFree(buffer: *mut c_void);
}
#[link(name = "Kernel32")]
unsafe extern "system" {
    // 最近错误码
    fn GetLastError() -> u32;
}
// 每块 2400 字节，低于单条 2560 字节的限制
const CHUNK: usize = 2400;
// 最多分块数
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
    // 目标名转宽字符
    let name = wide(name);
    // 接收系统分配的结构
    let mut raw = ptr::null_mut();
    // 读取失败
    if unsafe { CredReadW(name.as_ptr(), 1, 0, &mut raw) } == 0 {
        // 1168 = 不存在
        return if unsafe { GetLastError() } == 1168 {
            // 不存在
            Ok(None)
        // 否则分支
        } else {
            // 读取失败
            Err("读取 Windows 凭据管理器失败".into())
        };
    }
    // 指针为空
    if raw.is_null() {
        // 返回无效内容错误
        return Err("Windows 凭据管理器返回无效内容".into());
    }
    // 凭据在 `CredFree` 之前就被复制出来;之后没有任何东西再借用 `raw`。
    // 在 CredFree 之前先把内容复制出来，之后不再借用 raw
    unsafe {
        // 还原为引用
        let credential = &*raw;
        // 长度超限或指针为空视为无效
        let result = if credential.size > 2560 || (credential.size > 0 && credential.blob.is_null())
        {
            // 长度超过 2560 或指针为空
            Err("Windows 凭据长度无效".into())
        // 空值
        } else if credential.size == 0 {
            // 空值
            Ok(Some(Vec::new()))
        // 否则分支
        } else {
            // 否则复制数据
            Ok(Some(
                // 按长度复制数据
                std::slice::from_raw_parts(credential.blob, credential.size as usize).to_vec(),
            ))
        };
        // 释放系统分配的缓冲区
        CredFree(raw.cast());
        // 返回结果
        result
    }
}
// 写入一个凭据（通用类型，本机持久）
fn put(name: &str, bytes: &[u8]) -> Result<(), String> {
    // 目标名转宽字符
    let mut name = wide(name);
    // 构造凭据结构
    let credential = Credential {
        // 标志
        flags: 0,
        // 通用凭据
        kind: 1,
        // 目标名
        target: name.as_mut_ptr(),
        // 无备注
        comment: ptr::null_mut(),
        // 最后写入时间
        last_written: [0; 2],
        // 数据长度
        size: bytes.len() as u32,
        // 数据指针
        blob: bytes.as_ptr().cast_mut(),
        // 本机持久
        persist: 2,
        // 属性数量为 0
        attributes_count: 0,
        // 无属性
        attributes: ptr::null_mut(),
        // 无别名
        alias: ptr::null_mut(),
        // 无用户名
        username: ptr::null_mut(),
    };
    // 写入成功
    if unsafe { CredWriteW(&credential, 0) } != 0 {
        // 写入成功
        Ok(())
    // 否则分支
    } else {
        // 写入失败
        Err("写入 Windows 凭据管理器失败".into())
    }
}
// 删除一个凭据；不存在也视为成功
fn remove(name: &str) -> Result<(), String> {
    // 目标名转宽字符
    let name = wide(name);
    // 删除成功或本就不存在
    if unsafe { CredDeleteW(name.as_ptr(), 1, 0) } != 0 || unsafe { GetLastError() } == 1168 {
        // 删除成功
        Ok(())
    // 否则分支
    } else {
        // 删除失败
        Err("删除 Windows 配对凭据失败".into())
    }
}
// 解析索引 "代:分块数"
fn manifest(bytes: &[u8]) -> Result<(&str, usize), String> {
    // 索引必须是 UTF-8
    let text = std::str::from_utf8(bytes).map_err(|_| "Windows 凭据索引无效")?;
    // 拆成代号与分块数
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
        // 索引无效
        return Err("Windows 凭据索引无效".into());
    }
    // 返回代号与分块数
    Ok((generation, count))
}
// 清除某一代的全部分块
fn clear_generation(name: &str, bytes: &[u8]) -> Result<(), String> {
    // 解析索引
    let (generation, count) = manifest(bytes)?;
    // 逐块删除
    for index in 0..count {
        // 删除该分块
        remove(&format!("{name}/{generation}/{index}"))?;
    }
    // 完成
    Ok(())
}
// 读取：根条目是索引或墓碑
pub fn read(service: &str, account: &str) -> Result<Option<String>, String> {
    // 根条目目标名
    let name = target(service, account);
    // 读取根条目
    let Some(index) = get(&name)? else {
        // 不存在
        return Ok(None);
    };
    // 墓碑：完成未完成的清理并视为不存在
    if let Some(deleting) = index.strip_prefix(b"deleted:") {
        // 墓碑：完成清理并视为不存在
        clear_generation(&name, deleting)?;
        // 删除墓碑条目
        remove(&name)?;
        // 视为不存在
        return Ok(None);
    }
    // 解析索引
    let (generation, count) = manifest(&index)?;
    // 依次读出分块并拼接
    let mut bytes = Vec::new();
    // 逐块读取
    for index in 0..count {
        // 依次读出分块拼接
        bytes
            // 读取分块并追加
            .extend(get(&format!("{name}/{generation}/{index}"))?.ok_or("Windows 配对凭据不完整")?);
    }
    // 转为字符串
    String::from_utf8(bytes)
        // 包装为 Some
        .map(Some)
        // 编码损坏则报错
        .map_err(|_| "配对凭据编码无效".into())
}
// 分块事务写入：先写新一代，再提交索引，最后清理旧一代
pub fn write(service: &str, account: &str, value: &str) -> Result<(), String> {
    // 根条目目标名
    let name = target(service, account);
    // 记下旧根条目
    let old = get(&name)?;
    // 旧条目是墓碑则先清理
    if let Some(deleting) = old.as_ref().and_then(|old| old.strip_prefix(b"deleted:")) {
        // 旧条目是墓碑则先清理
        clear_generation(&name, deleting)?;
        // 删除旧根条目
        remove(&name)?;
    }
    // 分块数
    let count = value.len().max(1).div_ceil(CHUNK);
    // 分块数超限
    if count > MAX_CHUNKS {
        // 配对凭据过大
        return Err("配对凭据过大".into());
    }
    // 用进程号与纳秒时间生成代号
    let generation = format!(
        // 代号 = 进程号 + 纳秒时间
        "{:x}{:x}",
        // 进程号
        std::process::id(),
        // 当前系统时间
        std::time::SystemTime::now()
            // 距纪元的时长
            .duration_since(std::time::UNIX_EPOCH)
            // 时间无效则报错
            .map_err(|_| "系统时间无效")?
            // 纳秒数
            .as_nanos()
    );
    // 新的索引
    let index = format!("{generation}:{count}");
    // 写入各分块，失败则清理本代
    for chunk in 0..count {
        // 本块的字节范围
        let begin = (chunk * CHUNK).min(value.len());
        // 本块结束位置
        let end = (begin + CHUNK).min(value.len());
        // 写入分块，失败则清理本代
        if let Err(error) = put(
            // 分块目标名
            &format!("{name}/{generation}/{chunk}"),
            // 分块内容
            &value.as_bytes()[begin..end],
            // 失败则进入错误处理
        ) {
            // 清理本代分块
            let _ = clear_generation(&name, index.as_bytes());
            // 返回错误
            return Err(error);
        }
    }
    // 提交索引，失败则清理本代
    if let Err(error) = put(&name, index.as_bytes()) {
        // 提交索引失败：同样清理
        let _ = clear_generation(&name, index.as_bytes());
        // 返回错误
        return Err(error);
    }
    // 提交成功。旧分块不再是应用使用的凭据。
    // 提交成功后旧分块不再被应用使用，清理之
    if let Some(old) = old.filter(|old| !old.starts_with(b"deleted:")) {
        // 提交成功后清理旧分块
        let _ = clear_generation(&name, &old);
    }
    // 完成
    Ok(())
}
// 删除：先写墓碑，再清分块，最后清根条目
pub fn delete(service: &str, account: &str) -> Result<(), String> {
    // 根条目目标名
    let name = target(service, account);
    // 根条目存在时
    if let Some(index) = get(&name)? {
        // 取出索引内容（去掉墓碑前缀）
        let payload = index.strip_prefix(b"deleted:").unwrap_or(&index);
        // 墓碑记录在部分删除后依然保留。下一次读取会继续
        // 清理,而不是把缺失的分块解释为损坏的密钥。
        // 墓碑能撑过部分删除：下次读取会接着清理，而不是把缺失的分块当成损坏的密钥
        if !index.starts_with(b"deleted:") {
            // 先写墓碑，保证中途失败可继续
            let mut tombstone = b"deleted:".to_vec();
            // 墓碑追加索引内容
            tombstone.extend_from_slice(payload);
            // 写入墓碑
            put(&name, &tombstone)?;
        }
        // 清理各分块
        clear_generation(&name, payload)?;
    }
    // 最后删除根条目
    remove(&name)
}
