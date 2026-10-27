//! Android 凭据：宿主以 Android Keystore 包裹的随机密钥交给本进程，
//! 记录以 ChaCha20-Poly1305 加密后写入应用私有目录。未配置密钥时失败关闭。
use chacha20poly1305::{
    ChaCha20Poly1305, Key, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use std::{fs, path::PathBuf, sync::OnceLock};

struct Vault {
    dir: PathBuf,
    cipher: ChaCha20Poly1305,
}
static VAULT: OnceLock<Vault> = OnceLock::new();

/// 设置一次；重复配置被忽略，避免换钥后读不出既有记录。
pub fn configure(dir: PathBuf, key: [u8; 32]) -> Result<(), String> {
    fs::create_dir_all(&dir).map_err(|_| "凭据目录不可用".to_owned())?;
    let _ = VAULT.set(Vault {
        dir,
        cipher: ChaCha20Poly1305::new(&Key::from(key)),
    });
    Ok(())
}

fn vault() -> Result<&'static Vault, String> {
    VAULT
        .get()
        .ok_or_else(|| "系统安全存储尚未就绪，请重新打开应用".into())
}

fn file(vault: &Vault, service: &str, account: &str) -> PathBuf {
    let name: String = format!("{service}.{account}")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    vault.dir.join(name)
}

fn associated(service: &str, account: &str) -> Vec<u8> {
    format!("LokiMetris/vault/1/{service}/{account}").into_bytes()
}

pub fn read(service: &str, account: &str) -> Result<Option<String>, String> {
    let vault = vault()?;
    let bytes = match fs::read(file(vault, service, account)) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("安全存储读取失败".into()),
    };
    if bytes.len() < 12 + 16 {
        return Err("安全存储记录损坏".into());
    }
    let (nonce, sealed) = bytes.split_at(12);
    let nonce = Nonce::try_from(nonce).map_err(|_| "安全存储记录损坏".to_owned())?;
    let aad = associated(service, account);
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
    let mut nonce = [0u8; 12];
    nonce[..6].copy_from_slice(&uuid::Uuid::new_v4().as_bytes()[..6]);
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
    let temp = path.with_extension("pending");
    let mut bytes = nonce.to_vec();
    bytes.extend_from_slice(&sealed);
    let write = || -> std::io::Result<()> {
        use std::io::Write;
        let mut f = fs::File::create(&temp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        fs::rename(&temp, &path)
    };
    write().map_err(|_| "安全存储写入失败".into())
}

pub fn delete(service: &str, account: &str) -> Result<(), String> {
    let vault = vault()?;
    match fs::remove_file(file(vault, service, account)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("安全存储删除失败".into()),
    }
}
