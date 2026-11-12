//! Android 凭据：宿主以 Android Keystore 包裹的随机密钥交给本进程，
//! 记录以 ChaCha20-Poly1305 加密后写入应用私有目录。未配置密钥时失败关闭。
use chacha20poly1305::{
    // 加密算法、密钥与随机数类型
    ChaCha20Poly1305,
    Key,
    KeyInit,
    Nonce,
    aead::{Aead, Payload},
};
use std::{fs, path::PathBuf, sync::OnceLock};

// 保险库：存放目录与加密器
struct Vault {
    dir: PathBuf,
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
        dir,
        cipher: ChaCha20Poly1305::new(&Key::from(key)),
    });
    Ok(())
}

// 取得保险库；未配置时失败关闭
fn vault() -> Result<&'static Vault, String> {
    VAULT
        .get()
        .ok_or_else(|| "系统安全存储尚未就绪，请重新打开应用".into())
}

// 记录文件路径：服务与账户拼接，非安全字符替换为下划线
fn file(vault: &Vault, service: &str, account: &str) -> PathBuf {
    // 文件名：服务加账户
    let name: String = format!("{service}.{account}")
        .chars()
        .map(|c| {
            // 字母数字与 -_. 保持原样
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            // 否则分支
            } else {
                '_'
            }
        })
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
    let vault = vault()?;
    let bytes = match fs::read(file(vault, service, account)) {
        Ok(bytes) => bytes,
        // 文件不存在则返回 None
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        // 其他错误则报读取失败
        Err(_) => return Err("安全存储读取失败".into()),
    };
    // 至少要有 12 字节 nonce 和 16 字节认证标签
    if bytes.len() < 12 + 16 {
        return Err("安全存储记录损坏".into());
    }
    // 前 12 字节是 nonce，其余是密文
    let (nonce, sealed) = bytes.split_at(12);
    // 取随机数，长度不对则报损坏
    let nonce = Nonce::try_from(nonce).map_err(|_| "安全存储记录损坏".to_owned())?;
    let aad = associated(service, account);
    // 解密并校验关联数据
    let plain = vault
        .cipher
        .decrypt(
            &nonce,
            Payload {
                msg: sealed,
                aad: &aad,
            },
        )
        .map_err(|_| "安全存储记录损坏".to_owned())?;
    String::from_utf8(plain)
        .map(Some)
        .map_err(|_| "安全存储记录损坏".into())
}

pub fn write(service: &str, account: &str, value: &str) -> Result<(), String> {
    let vault = vault()?;
    // 两个随机 UUID 的前 6 字节没有版本位，拼成 96 位随机 nonce。
    // 两个随机 UUID 的前 6 字节没有版本位，拼成 96 位随机 nonce
    let mut nonce = [0u8; 12];
    // 随机数前 6 字节
    nonce[..6].copy_from_slice(&uuid::Uuid::new_v4().as_bytes()[..6]);
    // 随机数后 6 字节
    nonce[6..].copy_from_slice(&uuid::Uuid::new_v4().as_bytes()[..6]);
    let aad = associated(service, account);
    let sealed = vault
        .cipher
        .encrypt(
            &Nonce::from(nonce),
            Payload {
                msg: value.as_bytes(),
                aad: &aad,
            },
        )
        .map_err(|_| "安全存储写入失败".to_owned())?;
    let path = file(vault, service, account);
    // 先写临时文件
    let temp = path.with_extension("pending");
    // nonce 在前，密文在后
    let mut bytes = nonce.to_vec();
    bytes.extend_from_slice(&sealed);
    // 写入、同步到磁盘，再改名覆盖
    let write = || -> std::io::Result<()> {
        use std::io::Write;
        let mut f = fs::File::create(&temp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        // 原子改名到目标位置
        fs::rename(&temp, &path)
    };
    // 失败统一报写入失败
    write().map_err(|_| "安全存储写入失败".into())
}

// 删除记录；不存在也视为成功
pub fn delete(service: &str, account: &str) -> Result<(), String> {
    let vault = vault()?;
    match fs::remove_file(file(vault, service, account)) {
        Ok(()) => Ok(()),
        // 文件不存在也算成功
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        // 其他错误则报删除失败
        Err(_) => Err("安全存储删除失败".into()),
    }
}
