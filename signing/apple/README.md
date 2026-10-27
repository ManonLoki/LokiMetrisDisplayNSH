# Apple 身份与开发签名

`identity.json` 保存 iOS 包装器使用的 bundle ID、开发团队、版本、build 和 application-identifier，不含私钥。当前配置为：

| 字段 | 当前值 |
| --- | --- |
| bundle ID | `com.manonloki.lokimetrisdisplay.LokiMetrisDisplay` |
| team ID | `VV84X2M6C6` |
| 版本 / build | `1.0.10` / `19` |
| application-identifier | `VV84X2M6C6.com.manonloki.lokimetrisdisplay.LokiMetrisDisplay` |
| widgetAppGroup | `group.com.manonloki.lokimetrisdisplay.LokiMetrisDisplay` |

以配置文件实际值为准；Watch 工程默认 companion ID、团队和版本与之匹配。已有安装应沿用原 bundle ID 与团队，包括曾使用的团队后缀。移动包装器支持 `--id`，模拟器的 application-identifier 也必须同步匹配。

## 模拟器

默认 iOS 构建为包含 WidgetKit 扩展的模拟器 `.app`。runner 的 `__TEXT,__entitlements` 保留 application-identifier 和明确配置的共享组；主应用与扩展使用同一个 `LokiWidgetAppGroup`。模拟器采用对应的 ad-hoc 签名，真机仍需 provisioning profiles 授权。

```sh
python3 tools/build/package_app.py --target ios --with-watch --out output/packages/ios-001
```

## 真机开发签名

`--ios-device --with-watch` 生成包含手机、小组件和 Watch 的待签名组合包。使用已有 Apple Development 证书及各自 development profiles 创建副本：

```sh
python3 tools/build/sign_apple_device.py \
  --app "<待签名组合.app路径>" --output "output/packages/signed-001/诡秘神谕.app" \
  --bundle-id "<既有bundle ID>" --team "<Team ID>" \
  --identity "<开发证书SHA-1>" \
  --phone-profile "<手机.mobileprovision路径>" \
  --widget-profile "<小组件.mobileprovision路径>" \
  --app-group "<原App Group标识>" \
  --watch-profile "<手表.mobileprovision路径>" \
  --device "<iPhone UDID>"
```

默认仅校验并写 `signing-plan.json`。核对计划后，用相同参数加 `--execute` 创建签名副本；源 `.app` 保留。工具先签小组件和 Watch，再签手机，核验授权、各可执行代码段及 NSH bundle 摘要，输出 `signed-application.json`。小组件 ID 为手机 ID 加 `.widgets`；手机与扩展均须获授权并签入同一个共享组。工具不负责安装或导出 IPA。

证书须有本机私钥，`--identity` 使用精确 SHA-1，团队取证书 Subject 的 `OU`。profiles 须有效、允许对应标识及私有 Keychain group，并包含所选证书；手机 profile 须覆盖 `--device`。已知 Watch UDID 时加 `--watch-device` 校验手表覆盖。

## 安装与恢复检查

```sh
xcrun devicectl device install app --device "<iPhone UDID>" "<已签名.app路径>"
xcrun devicectl device process launch --device "<iPhone UDID>" \
  --terminate-existing --json-output output/device-launch.json "<既有bundle ID>"
```

启动选项放在 bundle ID 前。验证冷启动时确认旧进程终止且 PID 改变，再检查原配对、设置和图片恢复。同身份覆盖安装可保留数据，卸载不能替代身份兼容检查。组合签名成功还需验证系统实际安装 Watch、两端传输和后台投递。

构建环境见 [构建说明](../../docs/development/build.md)，使用及验收范围见 [移动说明](../../docs/mobile/usage.md) 和 [测试说明](../../docs/development/test.md)。
