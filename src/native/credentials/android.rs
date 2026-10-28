//! Android 凭据：宿主以 Android Keystore 包裹的随机密钥交给本进程，
//! 记录以 ChaCha20-Poly1305 加密后写入应用私有目录。未配置密钥时失败关闭。
// 引入 ChaCha20-Poly1305 认证加密
use chacha20poly1305::{
    // 加密算法、密钥与随机数类型
    ChaCha20Poly1305,
    // 密钥
    Key,
    // 密钥初始化
    KeyInit,
    // 随机数
    Nonce,
    // 加解密与载荷
    aead::{Aead, Payload},
};
// 引入文件系统、路径与一次性初始化
use std::{fs, path::PathBuf, sync::OnceLock};

// 保险库：存放目录与加密器
struct Vault {
    // 保险库目录
    dir: PathBuf,
    // 加密器
    cipher: ChaCha20Poly1305,
}
// 全局保险库，只设置一次
static VAULT: OnceLock<Vault> = OnceLock::new();

/// 设置一次；重复配置被忽略，避免换钥后读不出既有记录。
// 设置一次；重复配置被忽略，避免换钥后读不出既有记录
pub fn configure(dir: PathBuf, key: [u8; 32]) -> Result<(), String> {
    // 创建目录，失败则报目录不可用
    fs::create_dir_all(&dir).map_err(|_| "凭据目录不可用".to_owned())?;
    // 登记全局保险库，重复登记忽略
    let _ = VAULT.set(Vault {
        // 目录
        dir,
        // 由密钥构造加密器
        cipher: ChaCha20Poly1305::new(&Key::from(key)),
    });
    // 完成
    Ok(())
}

// 取得保险库；未配置时失败关闭
fn vault() -> Result<&'static Vault, String> {
    // 取全局保险库
    VAULT
        // 取全局保险库
        .get()
        // 尚未就绪则报错
        .ok_or_else(|| "系统安全存储尚未就绪，请重新打开应用".into())
}

// 记录文件路径：服务与账户拼接，非安全字符替换为下划线
fn file(vault: &Vault, service: &str, account: &str) -> PathBuf {
    // 文件名：服务加账户
    let name: String = format!("{service}.{account}")
        // 逐字符
        .chars()
        // 映射
        .map(|c| {
            // 字母数字与 -_. 保持原样
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                // 原字符
                c
            // 否则分支
            } else {
                // 其余替换为下划线
                '_'
            }
        })
        // 收集为字符串
        .collect();
    // 拼出保险库中的文件路径
    vault.dir.join(name)
}

// 关联数据：把密文绑定到服务与账户，防止被挪用
fn associated(service: &str, account: &str) -> Vec<u8> {
    // 关联数据：版本、服务与账户
    format!("LokiMetris/vault/1/{service}/{account}").into_bytes()
}

// 读取并解密；文件不存在返回 None
pub fn read(service: &str, account: &str) -> Result<Option<String>, String> {
    // 取保险库
    let vault = vault()?;
    // 读取密文文件
    let bytes = match fs::read(file(vault, service, account)) {
        // 读取成功
        Ok(bytes) => bytes,
        // 文件不存在则返回 None
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        // 其他错误则报读取失败
        Err(_) => return Err("安全存储读取失败".into()),
    };
    // 至少要有 12 字节 nonce 和 16 字节认证标签
    if bytes.len() < 12 + 16 {
        // 记录损坏
        return Err("安全存储记录损坏".into());
    }
    // 前 12 字节是 nonce，其余是密文
    let (nonce, sealed) = bytes.split_at(12);
    // 取随机数，长度不对则报损坏
    let nonce = Nonce::try_from(nonce).map_err(|_| "安全存储记录损坏".to_owned())?;
    // 关联数据
    let aad = associated(service, account);
    // 解密并校验关联数据
    let plain = vault
        // 加密器
        .cipher
        // 解密
        .decrypt(
            // 随机数
            &nonce,
            // 载荷
            Payload {
                // 密文
                msg: sealed,
                // 关联数据
                aad: &aad,
            },
        )
        // 损坏则报错
        .map_err(|_| "安全存储记录损坏".to_owned())?;
    // 明文转为字符串
    String::from_utf8(plain)
        // 包装为 Some
        .map(Some)
        // 编码损坏则报错
        .map_err(|_| "安全存储记录损坏".into())
}

// 加密并原子写入
pub fn write(service: &str, account: &str, value: &str) -> Result<(), String> {
    // 取保险库
    let vault = vault()?;
    // 两个随机 UUID 的前 6 字节没有版本位，拼成 96 位随机 nonce。
    // 两个随机 UUID 的前 6 字节没有版本位，拼成 96 位随机 nonce
    let mut nonce = [0u8; 12];
    // 随机数前 6 字节
    nonce[..6].copy_from_slice(&uuid::Uuid::new_v4().as_bytes()[..6]);
    // 随机数后 6 字节
    nonce[6..].copy_from_slice(&uuid::Uuid::new_v4().as_bytes()[..6]);
    // 关联数据
    let aad = associated(service, account);
    // 加密
    let sealed = vault
        // 加密器
        .cipher
        // 加密
        .encrypt(
            // 随机数
            &Nonce::from(nonce),
            // 加密载荷
            Payload {
                // 明文消息
                msg: value.as_bytes(),
                // 关联数据
                aad: &aad,
            },
        )
        // 加密失败则报错
        .map_err(|_| "安全存储写入失败".to_owned())?;
    // 目标文件路径
    let path = file(vault, service, account);
    // 先写临时文件
    let temp = path.with_extension("pending");
    // nonce 在前，密文在后
    let mut bytes = nonce.to_vec();
    // 追加密文
    bytes.extend_from_slice(&sealed);
    // 写入、同步到磁盘，再改名覆盖
    let write = || -> std::io::Result<()> {
        // 写特征
        use std::io::Write;
        // 创建临时文件
        let mut f = fs::File::create(&temp)?;
        // 写入全部字节
        f.write_all(&bytes)?;
        // 刷盘
        f.sync_all()?;
        // 原子改名到目标位置
        fs::rename(&temp, &path)
    };
    // 失败统一报写入失败
    write().map_err(|_| "安全存储写入失败".into())
}

// 删除记录；不存在也视为成功
pub fn delete(service: &str, account: &str) -> Result<(), String> {
    // 取保险库
    let vault = vault()?;
    // 删除文件
    match fs::remove_file(file(vault, service, account)) {
        // 成功
        Ok(()) => Ok(()),
        // 文件不存在也算成功
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        // 其他错误则报删除失败
        Err(_) => Err("安全存储删除失败".into()),
    }
}
