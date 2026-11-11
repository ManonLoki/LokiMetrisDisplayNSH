#!/usr/bin/env python3
# 文档字符串：把基线 APK 扩展为带系统小组件的完整 APK
"""Extend the base Android APK with the system app widget.

流程：读取基线清单与资源表 → 还原为文本清单 → 改类名、补权限、
注入小组件接收器 → 与基线资源和小组件资源一起经 aapt2 重新链接 →
生成 R 类并组装最终 APK。官方 NSH 的 Java、资产与本地库字节保持原样。
"""
# 引入路径对象
from pathlib import Path
# 引入子进程
import subprocess
# 引入临时目录
import tempfile
# 引入压缩包
import zipfile
# 引入 XML 处理
import xml.etree.ElementTree as ET

# 引入二进制清单转换与 arsc 解析
from android_binary_xml import to_text
from android_arsc import entries as arsc_entries


# 用 aapt2 解析平台 android.jar 的系统资源 ID 到名字的映射
def android_resource_names(build_tools, platform_jar, log):
    # 文档字符串：aapt2 dump resources 是框架资源表的权威解析器
    """Resolve framework resource names via aapt2 dump resources."""
    # aapt2 全量转储平台资源表
    dump = subprocess.run([str(build_tools / 'aapt2'), 'dump', 'resources', str(platform_jar)],
                          check=True, capture_output=True, text=True)
    # 转储量很大，按日志流的字节模式写入供审计
    log.write(dump.stdout.encode('utf-8'))
    # 映射表
    names = {}
    # 逐行解析：resource 0x0103022e style/Theme.Material.NoActionBar [PUBLIC]
    for line in dump.stdout.splitlines():
        # 只看资源声明行
        if not line.strip().startswith('resource 0x'):
            # 跳过
            continue
        # 字段：resource、ID、type/name、可见性
        fields = line.split()
        # 记录
        names[int(fields[1], 16)] = fields[2]
    # 必须解析到资源
    if not names:
        # 报错
        raise RuntimeError('aapt2 dump produced no framework resource names')
    # 返回映射
    return names

# Android 命名空间
ANDROID_NS = 'http://schemas.android.com/apk/res/android'
# 应用自有的 Application 与 Activity 类名（沿用既有命名）
APPLICATION = 'com.manonloki.lokimetrisdisplay.bridge.DisplayApplication'
ACTIVITY = 'com.manonloki.lokimetrisdisplay.bridge.DisplayActivity'
# 官方运行器的 Activity 类名
RUNNER_ACTIVITY = 'com.longbridge.nsh.AppActivity'
# mDNS 发现需要 Android 多播锁
MULTICAST_PERMISSION = 'android.permission.CHANGE_WIFI_MULTICAST_STATE'
# 小组件提供器与元数据
WIDGET_PROVIDER = 'com.manonloki.lokimetrisdisplay.widget.LokiWidgetProvider'
WIDGET_META_NAME = 'android.appwidget.provider'
WIDGET_UPDATE_ACTION = 'android.appwidget.action.APPWIDGET_UPDATE'
# 小组件资源源目录
WIDGET_RES = Path(__file__).resolve().parents[2] / 'platform/android/widget/res'
# 小组件显示名（与产品名一致）
WIDGET_LABEL = '诡秘神谕'


# 把基线清单还原为文本并扩展：类名、权限与小组件接收器
def extend_manifest(base_manifest, base_arsc, android_names):
    # 文档字符串：输出扩展后的文本清单
    """Restore the binary manifest to text and extend it for the widget."""
    # 解析基线资源表（应用表恰好一个包，解析器自行校验）
    _, app_names = arsc_entries(base_arsc)
    # 还原文本
    root = ET.fromstring(to_text(base_manifest, android_names, app_names))
    # 记录目标 SDK（原有打包流程的校验依赖）；构建平台信息属性由 aapt2 原样保留
    target_sdk = None
    # 找 uses-sdk
    for element in root.iter('uses-sdk'):
        # 读取目标 SDK
        target_sdk = element.get(f'{{{ANDROID_NS}}}targetSdkVersion')
    # application 元素
    applications = root.findall('application')
    # 必须恰好一个
    if len(applications) != 1:
        # 报错
        raise RuntimeError('Expected exactly one application element')
    # 取 application
    application = applications[0]
    # 小组件的 LAN HTTP 通道由 nsh.json 声明，载荷由共享核心加密。
    if application.get(f'{{{ANDROID_NS}}}usesCleartextTraffic') != 'true':
        raise RuntimeError('NSH package must enable android.usesCleartextTraffic for widget refresh')
    # 改 Application 类名（幂等：已是目标名则不变）
    application.set(f'{{{ANDROID_NS}}}name', APPLICATION)
    # 收集活动
    activities = application.findall('activity')
    # 找官方运行器活动
    runner = [element for element in activities
              if element.get(f'{{{ANDROID_NS}}}name') in {RUNNER_ACTIVITY, ACTIVITY}]
    # 必须恰好一个运行器
    if len(runner) != 1:
        # 报错
        raise RuntimeError('Expected one standard NSH application and runner activity')
    # 改运行器类名
    runner[0].set(f'{{{ANDROID_NS}}}name', ACTIVITY)
    # 在 Application 前补入缺失的多播权限。
    existing = {element.get(f'{{{ANDROID_NS}}}name') for element in root.findall('uses-permission')}
    if MULTICAST_PERMISSION not in existing:
        element = ET.Element('uses-permission')
        element.set(f'{{{ANDROID_NS}}}name', MULTICAST_PERMISSION)
        root.insert(list(root).index(application), element)
    # 已有小组件接收器则幂等移除后重建
    for element in list(application):
        # 找到同名接收器
        if element.tag == 'receiver' and element.get(f'{{{ANDROID_NS}}}name') == WIDGET_PROVIDER:
            # 移除旧的
            application.remove(element)
    # 构造小组件接收器
    receiver = ET.SubElement(application, 'receiver')
    # 提供器类名
    receiver.set(f'{{{ANDROID_NS}}}name', WIDGET_PROVIDER)
    # 显示名与产品一致
    receiver.set(f'{{{ANDROID_NS}}}label', WIDGET_LABEL)
    # 只由系统投递小组件广播
    receiver.set(f'{{{ANDROID_NS}}}exported', 'false')
    # 更新意图过滤器
    intent_filter = ET.SubElement(receiver, 'intent-filter')
    # 小组件更新动作
    action = ET.SubElement(intent_filter, 'action')
    # 动作名
    action.set(f'{{{ANDROID_NS}}}name', WIDGET_UPDATE_ACTION)
    # 小组件信息元数据
    meta = ET.SubElement(receiver, 'meta-data')
    # 元数据名
    meta.set(f'{{{ANDROID_NS}}}name', WIDGET_META_NAME)
    # 指向小组件信息资源
    meta.set(f'{{{ANDROID_NS}}}resource', '@xml/widget_info')
    # 序列化：注册 android 命名空间前缀
    ET.register_namespace('android', ANDROID_NS)
    # 返回文本与目标 SDK
    return ET.tostring(root, encoding='unicode'), target_sdk


# 用 aapt2 编译并链接资源，产出新清单、资源表与 R 类
def link_resources(platform_jar, build_tools, res_sources, manifest_text, out, log):
    # 文档字符串：compile + link 两步
    """Compile resource sources and link a new resources APK plus R.java."""
    # aapt2 可执行文件
    aapt2 = build_tools / 'aapt2'
    # 链接输出
    linked = out / 'resources.ap_'
    # 编译输出 zip（aapt2 compile 的 -o 产物是压缩包）
    compiled_zip = out / 'compiled.zip'
    # 编译每个资源目录（基线 res 与小组件 res 都已合并进 res_sources）
    subprocess.run([str(aapt2), 'compile', '--no-crunch', '--dir', str(res_sources),
                    '-o', str(compiled_zip)], check=True, stdout=log, stderr=subprocess.STDOUT)
    # 解开编译产物取得 .flat 文件
    compiled = out / 'compiled'
    # 创建目录
    compiled.mkdir()
    # 解压
    with zipfile.ZipFile(compiled_zip) as archive:
        # 逐条解出
        for name in archive.namelist():
            # 展平存放
            target = compiled / Path(name).name
            # 写入
            target.write_bytes(archive.read(name))
    # 写出文本清单
    manifest = out / 'AndroidManifest.xml'
    # 写入
    manifest.write_text(manifest_text, encoding='utf-8')
    # 链接：平台 jar 决定系统资源；保留基线的编译 SDK 信息
    subprocess.run([str(aapt2), 'link', '-o', str(linked), '-I', str(platform_jar),
                    '--manifest', str(manifest), '--java', str(out / 'java'),
                    *[str(path) for path in sorted(compiled.glob('*.flat'))]],
                   check=True, stdout=log, stderr=subprocess.STDOUT)
    # 返回链接产物与 R.java 位置
    return linked, out / 'java'


# 组装最终未签名 APK：链接资源 + 基线资产与本地库 + 应用 DEX
def assemble(linked_ap, base_apk, dex_paths, out_apk, log):
    # 文档字符串：只搬运不修改的官方条目
    """Write the unsigned APK from linked resources and untouched base entries."""
    # 打开链接产物
    with zipfile.ZipFile(linked_ap) as resources, zipfile.ZipFile(base_apk) as base, \
            zipfile.ZipFile(out_apk, 'w', zipfile.ZIP_DEFLATED) as output:
        # 先写链接出的资源与清单
        for name in sorted(resources.namelist()):
            # 资源表必须不压缩存储，API 30 起安装器强制要求
            stored = name == 'resources.arsc'
            # 写入条目
            output.writestr(zipfile.ZipInfo(name, (2020, 1, 1, 0, 0, 0)),
                            resources.read(name),
                            compress_type=zipfile.ZIP_STORED if stored else zipfile.ZIP_DEFLATED)
        # 再写基线里不变的条目
        for entry in base.infolist():
            # 条目名
            name = entry.filename
            # 签名、旧清单与旧资源表不搬运
            if name.startswith('META-INF/') or name in {'AndroidManifest.xml', 'resources.arsc'}:
                # 跳过
                continue
            # res 由链接产物提供
            if name.startswith('res/'):
                # 跳过
                continue
            # classes*.dex 由新的 DEX 提供
            if name.startswith('classes') and name.endswith('.dex'):
                # 跳过
                continue
            # 原样写入（资产、本地库等）；本地库保持原压缩方式，
            # extractNativeLibs=false 要求它们在 APK 内不压缩
            copied = zipfile.ZipInfo(name, (2020, 1, 1, 0, 0, 0))
            # 保留原压缩方式与外部属性
            copied.compress_type = entry.compress_type
            # 写入
            output.writestr(copied, base.read(name))
        # 写入新 DEX
        for path in dex_paths:
            # 按文件名写入
            output.write(path, path.name, compress_type=zipfile.ZIP_DEFLATED)
