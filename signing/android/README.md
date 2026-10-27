# Android 签名

此目录保存从旧工程迁入的原 Android 签名身份。默认应用标识为 `com.manonloki.lokimetrisdisplay`，覆盖安装须沿用已有标识及证书。

| 文件 | 用途 |
| --- | --- |
| `lokimetris.keystore` | 原签名私钥 |
| `signing.properties` | `keyAlias`、`storePassword` 及可选 `keyPassword` |

两份文件必须一起安全备份，已被根 `.gitignore` 排除。不要在文档、命令行或构建日志中填写密码；公开证书或 PEM 只能识别身份，不能替代私钥。

```sh
python3 tools/build/package_app.py --target android --out output/packages/android-001
```

包装器读取以上文件，通过子进程环境传递密码，完成 APK 对齐、签名和签名核验。缺少文件或必要属性时停止，不生成替代证书。安装使用同证书的覆盖升级，环境与完整步骤见 [构建说明](../../docs/development/build.md)，身份与配对影响见 [移动说明](../../docs/mobile/usage.md)。
