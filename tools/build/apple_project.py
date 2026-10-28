"""Xcode project readers used to generate the companion project."""
import hashlib
import json
from pathlib import Path
import plistlib
import subprocess


# 由名称稳定地生成 24 位十六进制标识，用作工程对象的键
def identifier(name):
    # 由名称生成 24 位大写十六进制标识
    return hashlib.sha256(name.encode()).hexdigest()[:24].upper()


# 把 Python 值序列化为 pbxproj（旧式属性列表）文本
def project_value(value):
    # 字典
    if isinstance(value, dict):
        # 字典序列化为 pbxproj 的花括号写法
        return '{' + ''.join(json.dumps(str(key), ensure_ascii=False) + ' = ' + project_value(item) + ';'
                             # 逐个键值对拼接
                             for key, item in value.items()) + '}'
    # 列表
    if isinstance(value, list):
        # 列表序列化为圆括号写法
        return '(' + ','.join(project_value(item) for item in value) + ')'
    # 整数原样输出
    if isinstance(value, int):
        # 其他类型直接转字符串
        return str(value)
    # 其余当作字符串
    return json.dumps(str(value), ensure_ascii=False)


# 从工程中取出唯一同名的原生目标，返回目标 ID、产物 ID 与产物名
def native_target(project, name):
    # 用 plutil 把 pbxproj 转成 JSON
    data = subprocess.check_output(['plutil', '-convert', 'json', '-o', '-', str(project / 'project.pbxproj')])
    # 解析工程 JSON
    contents = json.loads(data)
    # 找出指定名称的原生目标
    targets = [(key, value) for key, value in contents['objects'].items()
               # 须是 PBXNativeTarget 且名称相符
               if value.get('isa') == 'PBXNativeTarget' and value.get('name') == name]
    # 目标数量不为一
    if len(targets) != 1:
        # 报错
        raise RuntimeError('Expected one independent ' + name + ' native target')
    # 取出目标标识与内容
    target_id, target = targets[0]
    # 产物引用标识
    product_id = target['productReference']
    # 返回目标、产物及路径
    return target_id, product_id, contents['objects'][product_id]['path']


# 手表目标
def watch_target(project):
    # 手表目标
    return native_target(project, 'LokiMetrisWatch')


# 小组件目标
def widget_target(project):
    # 小组件目标
    return native_target(project, 'LokiMetrisWidgets')


# 取 SDK 要求的最低部署版本与调用方给出的版本中较大者
def deployment_target(simulator, supplied):
    # 按是否模拟器选择 SDK
    sdk = 'iphonesimulator' if simulator else 'iphoneos'
    # 查询 SDK 路径
    path = Path(subprocess.check_output(['xcrun', '--sdk', sdk, '--show-sdk-path'], text=True).strip())
    # 读取 SDK 设置
    settings = plistlib.loads((path / 'SDKSettings.plist').read_bytes())
    # 该 SDK 的最低部署版本
    minimum = settings['SupportedTargets'][sdk]['MinimumDeploymentTarget']
    # 把版本号转成元组以便比较
    version = lambda value: tuple(int(part) for part in value.split('.'))
    # 取版本更高的一个
    return max([minimum, supplied], key=version)
