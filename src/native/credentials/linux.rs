//! Secret Service 的进程边界：凭据通过标准输入传递，不进入命令行参数。
// 引入 IO、进程、线程与时间
use std::{
    // 读写特征
    io::{Read, Write},
    // 进程类型
    process::{Command, ExitStatus, Stdio},
    // 线程
    thread,
    // 时长与时刻
    time::{Duration, Instant},
};

// 运行 secret-tool：输出有界并发读取，整体 15 秒超时；返回状态、标准输出与标准错误
fn execute(args: &[&str], input: Option<&str>) -> Result<(ExitStatus, Vec<u8>, Vec<u8>), String> {
    // 有输入时用管道传入，凭据不进命令行参数
    let mut child = Command::new("secret-tool")
        // 传入参数
        .args(args)
        // 有输入才接管道，否则关闭标准输入
        .stdin(if input.is_some() {
            // 标准输入用管道
            Stdio::piped()
        // 否则分支
        } else {
            // 标准输入关闭
            Stdio::null()
        })
        // 捕获标准输出
        .stdout(Stdio::piped())
        // 捕获标准错误
        .stderr(Stdio::piped())
        // 启动子进程
        .spawn()
        // 找不到 secret-tool 时提示安装
        .map_err(|_| {
            // 启动失败时的提示
            "无法访问系统 Secret Service；请安装并启用 secret-tool 和会话钥匙串".to_string()
            // 创建成功
        })?;
    // 取出管道句柄
    let stdout = child.stdout.take().ok_or("无法读取系统钥匙串")?;
    // 取出标准错误管道
    let stderr = child.stderr.take().ok_or("无法读取系统钥匙串状态")?;
    // 并发地排空有界输出;大记录不得在所有者等待 secret-tool 退出时
    // 填满管道。
    // 并发读出有界输出；大记录不能在 owner 等待退出时塞满管道
    let output = thread::spawn(move || {
        // 标准输出读取线程：最多读 4 MiB 加 1 字节
        let mut bytes = Vec::new();
        // 读取标准输出
        stdout
            // 最多读 4 MiB 加一字节
            .take(4_194_305)
            // 读入缓冲区
            .read_to_end(&mut bytes)
            // 返回缓冲区
            .map(|_| bytes)
    });
    // 标准错误读取线程：最多 8 KiB 加 1 字节
    let errors = thread::spawn(move || {
        // 读取缓冲区
        let mut bytes = Vec::new();
        // 最多读 8 KiB 加一字节
        stderr.take(8193).read_to_end(&mut bytes).map(|_| bytes)
    });
    // 有输入时在线程中写入标准输入
    let writer = input.map(|value| {
        // 写线程：把输入复制出来
        let bytes = value.as_bytes().to_vec();
        // 取出标准输入管道
        let stdin = child.stdin.take();
        // 写线程
        thread::spawn(move || {
            // 把输入写进标准输入管道
            stdin
                // 标准输入不可用则报错
                .ok_or_else(|| std::io::Error::other("credential input unavailable"))
                // 写入全部字节
                .and_then(|mut pipe| pipe.write_all(&bytes))
        })
    });
    // 15 秒期限
    let deadline = Instant::now() + Duration::from_secs(15);
    // 轮询子进程直到退出，超时则终止
    let status = loop {
        // 轮询子进程状态
        match child.try_wait() {
            // 已退出
            Ok(Some(status)) => break Ok(status),
            // 未退出且未超时：休眠 25 毫秒再查
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            // 超时或出错：杀掉子进程
            _ => {
                // 超时：杀掉子进程
                let _ = child.kill();
                // 回收进程
                let _ = child.wait();
                // 报响应超时或不可用
                break Err("系统钥匙串响应超时或不可用，请解锁后重试");
            }
        }
    };
    // 输入是否写入成功
    let wrote = writer
        // 写线程是否成功
        .map(|writer| writer.join().ok().is_some_and(|result| result.is_ok()))
        // 没有输入则视为成功
        .unwrap_or(true);
    // 收回读取线程的结果
    let output = output
        // 等待读线程结束
        .join()
        // 线程崩溃与读取失败分别映射为错误
        .map_err(|_| "读取系统钥匙串失败")?
        // 读取失败
        .map_err(|_| "读取系统钥匙串失败")?;
    // 等待标准错误读线程
    let errors = errors
        // 等待线程结束
        .join()
        // 线程失败
        .map_err(|_| "读取系统钥匙串状态失败")?
        // 读取失败
        .map_err(|_| "读取系统钥匙串状态失败")?;
    // 子进程超时的错误在此抛出
    let status = status?;
    // 输出超限或写入失败
    if !wrote || output.len() > 4_194_304 || errors.len() > 8192 {
        // 内容超限或写入失败
        return Err("系统钥匙串内容超出限制或写入失败".into());
    }
    // 返回状态、输出与错误
    Ok((status, output, errors))
}

// 查询：成功且无错误输出则返回内容，退出码 1 表示不存在
fn lookup(service: &str, account: &str) -> Result<Option<String>, String> {
    // 调用 secret-tool lookup
    let (status, output, errors) =
        // 查询凭据
        execute(&["lookup", "service", service, "account", account], None)?;
    // 管道输出时 secret-tool 原样输出字节，不加换行；保留秘密末尾的换行
    if status.success() && errors.is_empty() {
        // stdout 为管道时,secret-tool 输出精确的字节,不会
        // 附加换行。要保留机密末尾的换行符。
        String::from_utf8(output)
            // 内容必须是合法 UTF-8
            .map(Some)
            // 编码无效
            .map_err(|_| "配对凭据编码无效".into())
    // 退出码 1 表示条目不存在
    } else if status.code() == Some(1) && errors.is_empty() {
        // 条目不存在
        Ok(None)
    // 否则分支
    } else {
        // 服务不可用或已锁定
        Err("系统 Secret Service 不可用或已锁定".into())
    }
}
// 存储单个分块：写入后必须原样读回才算成功
fn store(service: &str, account: &str, value: &str) -> Result<(), String> {
    // 分块长度有界且不含 NUL
    if value.len() > 6000 || value.contains('\0') {
        // 分块无效
        return Err("系统钥匙串分块无效".into());
    }
    // 调用 secret-tool store，值通过标准输入传入
    let (status, _, errors) = execute(
        // 参数
        &[
            // 操作：存储
            "store",
            // 钥匙串里显示的标签
            "--label=LokiMetris Display 配对",
            // 以服务名和账户名作为属性
            "service",
            // 服务名
            service,
            // 账户标记
            "account",
            // 账户名
            account,
        ],
        // 通过标准输入传入值
        Some(value),
        // 失败即返回
    )?;
    // secret-tool 的 stdin 读取器会截断超长输入,而 store 仍可能
    // 成功退出。在提交根清单之前,必须同时具备有界分块、stderr
    // 和精确的回读。
    // secret-tool 读取输入时会截断超长内容却仍可能成功退出；因此要求分块有界、无错误输出，并精确读回后才提交根索引
    if status.success() && errors.is_empty() && lookup(service, account)?.as_deref() == Some(value)
    {
        // 写入成功
        Ok(())
    // 否则分支
    } else {
        // 服务不可用或已锁定
        Err("系统 Secret Service 不可用或已锁定".into())
    }
}
// 清除条目；退出码 1（不存在）也算成功
fn clear(service: &str, account: &str) -> Result<(), String> {
    // 调用 secret-tool clear
    let (status, _, errors) = execute(&["clear", "service", service, "account", account], None)?;
    // 成功或本就不存在
    if (status.success() || status.code() == Some(1)) && errors.is_empty() {
        // 清除成功
        Ok(())
    // 否则分支
    } else {
        // 删除失败
        Err("删除系统钥匙串凭据失败".into())
    }
}
// 分块存储的根索引前缀
const PREFIX: &str = "loki-store-v1:";
// 墓碑前缀：删除未完成时留下标记
const DELETED: &str = "loki-delete-v1:";
// 解析索引 "代:分块数"，代必须是十六进制，分块数在 1 到 700
fn manifest(value: &str) -> Result<(&str, usize), String> {
    // 按冒号拆成代号与分块数
    let (generation, count) = value.split_once(':').ok_or("系统钥匙串索引无效")?;
    // 分块数解析失败则报索引无效
    let count: usize = count.parse().map_err(|_| "系统钥匙串索引无效")?;
    // 代号须为十六进制，分块数 1 到 700
    if generation.is_empty()
        // 或者代号含非十六进制字符
        || !generation.bytes().all(|b| b.is_ascii_hexdigit())
        // 或者分块数不在 1 到 700
        || !(1..=700).contains(&count)
    {
        // 索引无效
        return Err("系统钥匙串索引无效".into());
    }
    // 返回代号与分块数
    Ok((generation, count))
}
// 清除某一代的全部分块
fn cleanup(service: &str, account: &str, index: &str) -> Result<(), String> {
    // 解析索引
    let (generation, count) = manifest(index)?;
    // 逐块清除
    for chunk in 0..count {
        // 清除该分块
        clear(service, &format!("{account}/{generation}/{chunk}"))?;
    }
    // 完成
    Ok(())
}
// 读取：根条目可能是旧版明文、分块索引或墓碑
pub fn read(service: &str, account: &str) -> Result<Option<String>, String> {
    // 不存在则返回 None
    let Some(value) = lookup(service, account)? else {
        // 条目不存在
        return Ok(None);
    };
    // 墓碑：继续完成上次未完成的清理并视为不存在
    if let Some(index) = value.strip_prefix(DELETED) {
        // 墓碑：继续清理后视为不存在
        cleanup(service, account, index)?;
        // 清除墓碑条目
        clear(service, account)?;
        // 视为不存在
        return Ok(None);
    }
    // 不是分块索引：按旧版明文直接返回
    let Some(index) = value.strip_prefix(PREFIX) else {
        // 旧版明文条目：原样返回
        return Ok(Some(value));
    };
    // 解析索引
    let (generation, count) = manifest(index)?;
    // 依次读出并拼接各分块，限制单块与总长
    let mut value = String::new();
    // 依次读出并拼接分块
    for chunk in 0..count {
        // 读取该分块
        let text = lookup(service, &format!("{account}/{generation}/{chunk}"))?
            // 分块缺失视为损坏
            .ok_or("配对凭据分块缺失")?;
        // 单块与总长都有上限
        if text.len() > 6000 || value.len() + text.len() > 4_194_304 {
            // 长度无效
            return Err("配对凭据长度无效".into());
        }
        // 拼接分块
        value.push_str(&text);
    }
    // 返回完整值
    Ok(Some(value))
}
// 分块写入：先写新一代所有分块，再提交根索引，最后清理上一代
pub fn write(service: &str, account: &str, value: &str) -> Result<(), String> {
    // 总长不超过 4 MiB 且不含 NUL
    if value.len() > 4_194_304 || value.contains('\0') {
        // 长度或内容无效
        return Err("配对凭据长度或内容无效".into());
    }
    // 记录旧的根条目
    let old = lookup(service, account)?;
    // 旧条目是墓碑则先清理
    if let Some(index) = old.as_ref().and_then(|v| v.strip_prefix(DELETED)) {
        // 旧条目是墓碑：先清理
        cleanup(service, account, index)?;
        // 清除旧的根条目
        clear(service, account)?;
    }
    // 按 6000 字节切分，保证落在字符边界上
    let mut chunks = Vec::new();
    // 按 6000 字节切块，保证落在字符边界上
    let mut offset = 0;
    // 按字节偏移切块
    while offset < value.len() {
        // 每块最多 6000 字节
        let mut end = (offset + 6000).min(value.len());
        // 落在字符边界之前回退
        while !value.is_char_boundary(end) {
            // 回退到字符边界
            end -= 1;
        }
        // 记录该分块
        chunks.push(&value[offset..end]);
        // 前进偏移
        offset = end;
    }
    // 空值也要有一个分块
    if chunks.is_empty() {
        // 空值也要有一个分块
        chunks.push("");
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
    // 新的索引：代号与分块数
    let index = format!("{generation}:{}", chunks.len());
    // 写入各分块，失败则清理本代
    for (chunk, text) in chunks.iter().enumerate() {
        // 分块写失败：清理本代并返回错误
        if let Err(error) = store(service, &format!("{account}/{generation}/{chunk}"), text) {
            // 清理本代分块
            let _ = cleanup(service, account, &index);
            // 返回错误
            return Err(error);
        }
    }
    // 提交根索引；外部存储可能已修改条目却报告警告或丢失回复，所以先恢复旧指针再删暂存分块；若回滚不确定则两代都保留，保证既有指针不会指向刚删除的分块
    if let Err(error) = store(service, account, &format!("{PREFIX}{index}")) {
        // 外部存储可能先改动条目,然后报告警告
        // 或丢失应答。在删除暂存分块之前先恢复旧指针;
        // 若回滚结果不确定,就同时保留两个代次,这样
        // 现有指针绝不会指向我们刚删除的分块。
        let rollback = match old.as_deref() {
            // 回滚：旧指针存在则写回
            Some(previous) => store(service, account, previous),
            // 否则清除
            None => clear(service, account),
        };
        // 回滚成功才清理暂存分块
        if rollback.is_ok() {
            // 清理本代分块
            let _ = cleanup(service, account, &index);
        }
        // 返回错误
        return Err(error);
    }
    // 提交成功后清理上一代
    if let Some(index) = old.as_ref().and_then(|v| v.strip_prefix(PREFIX)) {
        // 提交成功后清理上一代
        let _ = cleanup(service, account, index);
    }
    // 完成
    Ok(())
}
// 删除：先写墓碑再清分块，最后清根条目；中途失败时下次读取会接着清理
pub fn delete(service: &str, account: &str) -> Result<(), String> {
    // 删除：根条目存在时处理
    if let Some(value) = lookup(service, account)?
        // 并且
        && let Some(index) = value
            // 去掉索引前缀
            .strip_prefix(PREFIX)
            // 或去掉墓碑前缀
            .or_else(|| value.strip_prefix(DELETED))
    {
        // 先写墓碑，保证中途失败可继续
        if value.starts_with(PREFIX) {
            // 先写入墓碑
            store(service, account, &format!("{DELETED}{index}"))?;
        }
        // 清理分块
        cleanup(service, account, index)?;
    }
    // 最后清除根条目
    clear(service, account)
}
