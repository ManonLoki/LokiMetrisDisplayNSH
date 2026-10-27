# Android 签名

此目录保存 Android 正式发布的签名配置及公共证书。默认应用标识为 `com.manonloki.lokimetrisdisplay`；覆盖安装需要应用标识和签名证书与设备上的已有安装匹配。

| 文件 | 用途 |
| --- | --- |
| `lokimetris.keystore` | 当前正式发布使用的私钥库 |
| `signing.properties` | `keyAlias`、`storePassword` 及可选 `keyPassword` |
| `lokimetris-release-certificate.pem` | 当前正式私钥库对应的公共证书 |
| `lokimetris-certificate.pem` | 保留的旧 Android Debug 公共证书，用于识别既有开发安装 |

私钥库与 `signing.properties` 必须一起安全备份，均被根 `.gitignore` 排除。不要在文档、命令行或构建日志中填写密码；公共证书可以随源码保存，只能识别签名身份，不能替代私钥。

| 签名身份 | 证书 SHA-256 |
| --- | --- |
| 正式发布，`CN=LokiMetris, O=ManonLoki` | `876bda7254339183c62b542af5ebc579f415f7dee343d4ec81b32d71b5ad675c` |
| 旧开发安装，Android Debug | `a0eb4b795b1cd64e17bf6358bd0298242fc38abfd1ead0405975e2fe97482d1c` |

```sh
python3 tools/build/package_app.py --target android --out output/packages/android-001
```

包装器读取以上文件，通过子进程环境传递密码，完成 APK 对齐、签名和签名核验。缺少文件或必要属性时停止，不生成替代证书。安装使用同证书的覆盖升级，环境与完整步骤见 [构建说明](../../docs/development/build.md)，身份与配对影响见 [移动说明](../../docs/mobile/usage.md)。

正式发布包与旧开发安装可能使用不同证书；包名相同并不代表可以覆盖安装。旧 Debug 安装可使用本机已有、证书完全匹配的 Debug 私钥，将正式包的相同应用内容重签为独立兼容副本，以保留既有数据与配对。该副本只用于对应设备覆盖升级，GitHub 发布资产仍使用正式签名。
