# 文档字符串：补充局域网权限与应用自有入口
"""Preserve runner metadata while adding LAN permissions and app-owned Android entry points."""
# 引入二进制打包工具
import struct

# 属性/字符串索引的空值
NO_INDEX = 0xffffffff
# 需要补充的局域网相关权限
PERMISSIONS = ['android.permission.ACCESS_NETWORK_STATE', 'android.permission.CHANGE_WIFI_MULTICAST_STATE']
# 应用自有的 Application 类名
APPLICATION = 'com.manonloki.lokimetrisdisplay.bridge.DisplayApplication'
# 应用自有的 Activity 类名
ACTIVITY = 'com.manonloki.lokimetrisdisplay.bridge.DisplayActivity'
# 官方运行器的 Activity 类名
RUNNER_ACTIVITY = 'com.longbridge.nsh.AppActivity'


# 读取字符串长度前缀（UTF-8 为 1 或 2 字节，UTF-16 为 2 或 4 字节），返回长度与新偏移
def length(data, offset, utf8):
    # UTF-8 编码的长度
    if utf8:
        # 高位为 1 表示长度占两个字节
        first = data[offset]
        # 单字节或双字节长度
        return (((first & 0x7f) << 8) | data[offset + 1], offset + 2) if first & 0x80 else (first, offset + 1)
    # UTF-16 的 16 位首字
    first = struct.unpack_from('<H', data, offset)[0]
    # 单字或双字长度
    return (((first & 0x7fff) << 16) | struct.unpack_from('<H', data, offset + 2)[0], offset + 4) if first & 0x8000 else (first, offset + 2)


# 把长度编码回前缀形式
def encoded_length(value, utf8):
    # UTF-8 编码长度
    if utf8:
        # 小于 0x80 用一字节，否则两字节
        return bytes([value]) if value < 0x80 else bytes([(value >> 8) | 0x80, value & 0xff])
    # 小于 0x8000 用一字，否则两字
    return struct.pack('<H', value) if value < 0x8000 else struct.pack('<HH', (value >> 16) | 0x8000, value & 0xffff)


# 解析字符串池，返回字符串列表与是否为 UTF-8
def strings(chunk):
    # 字符串数、样式数、标志与字符串数据起点
    count, styles, flags, start, _ = struct.unpack_from('<5I', chunk, 8)
    # 不处理带样式的字符串池
    if styles:
        # 不支持带样式的字符串池
        raise RuntimeError('Unexpected styled manifest string pool')
    # 是否 UTF-8
    utf8 = bool(flags & 0x100)
    # 字符串列表
    values = []
    # 逐个读取字符串
    for index in range(count):
        # 字符串偏移
        offset = start + struct.unpack_from('<I', chunk, 28 + index * 4)[0]
        # 读取字符数
        units, offset = length(chunk, offset, utf8)
        # UTF-8 时
        if utf8:
            # 再读字节数
            size, offset = length(chunk, offset, True)
            # 按字节数解码
            values.append(chunk[offset:offset + size].decode('utf-8'))
        # UTF-16 时
        else:
            # 按字符数乘二解码
            values.append(chunk[offset:offset + units * 2].decode('utf-16-le'))
    # 返回字符串表与是否 UTF-8
    return values, utf8


# 把字符串列表重新编码为字符串池块
def pool(values, utf8):
    # 字符串体与偏移表
    body = bytearray(); offsets = []
    # 逐个字符串
    for value in values:
        # 记下偏移
        offsets.append(len(body))
        # UTF-16 单元数
        units = len(value.encode('utf-16-le')) // 2
        # UTF-8 时
        if utf8:
            # 编码为 UTF-8
            encoded = value.encode('utf-8')
            # 写入两段长度和内容
            body += encoded_length(units, True) + encoded_length(len(encoded), True) + encoded + b'\0'
        # 否则
        else:
            # 追加长度、UTF-16 内容和结尾
            body += encoded_length(units, False) + value.encode('utf-16-le') + b'\0\0'
    # 补齐到 4 字节对齐
    body += bytes((-len(body)) % 4)
    # 偏移表起点
    start = 28 + len(values) * 4
    # 拼出完整的块：头、偏移表与字符串数据
    return (struct.pack('<HHI5I', 1, 28, start + len(body), len(values), 0, 0x100 if utf8 else 0, start, 0)
            # 偏移表与字符串体
            + struct.pack('<' + 'I' * len(offsets), *offsets) + body)


# 在清单中补上局域网权限，返回新清单与目标 SDK 版本
def lan_permissions(data):
    # 必须是二进制 Android XML
    if data[:4] != b'\x03\0\x08\0':
        # 报错：需要二进制 Android XML
        raise RuntimeError('Expected binary Android XML')
    # 块列表、偏移、字符串表与目标 SDK
    chunks = []; offset = 8; values = None; target_sdk = None
    # 切分顶层块
    while offset < len(data):
        # 读取块类型与大小
        kind, _, size = struct.unpack_from('<HHI', data, offset)
        # 块大小越界
        if size < 8 or offset + size > len(data):
            # 报错
            raise RuntimeError('Invalid binary manifest chunk')
        # 取出该块
        chunk = data[offset:offset + size]
        # 记录并前进
        chunks.append((kind, chunk)); offset += size
        # 字符串池
        if kind == 1:
            # 解析字符串表
            values, utf8 = strings(chunk)
    # 没有字符串表
    if values is None:
        # 报错
        raise RuntimeError('Manifest has no string pool')
    # 确保需要的字符串都在池里
    for value in ['uses-permission', 'name', 'http://schemas.android.com/apk/res/android'] + PERMISSIONS:
        # 字符串不在表中
        if value not in values:
            # 追加
            values.append(value)
    # 新增的权限节点
    nodes = bytearray()
    # 标签名与属性名的索引
    tag = values.index('uses-permission'); name = values.index('name')
    # 命名空间索引
    namespace = values.index('http://schemas.android.com/apk/res/android')
    # 已声明的权限
    existing = set()
    # 扫描所有元素：收集已有权限并读取目标 SDK
    for kind, chunk in chunks:
        # 只看起始元素块
        if kind != 0x102:
            # 跳过
            continue
        # 元素名
        element = values[struct.unpack_from('<I', chunk, 20)[0]]
        # 属性起点、大小与数量
        start, size, count = struct.unpack_from('<3H', chunk, 24)
        # 逐个属性
        for i in range(count):
            # 属性偏移
            attribute = 16 + start + i * size
            # 属性名
            key = values[struct.unpack_from('<I', chunk, attribute + 4)[0]]
            # 属性值类型
            value_type = chunk[attribute + 15]
            # 读取属性值
            value = struct.unpack_from('<I', chunk, attribute + 16)[0]
            # 已声明的权限
            if element == 'uses-permission' and key == 'name' and value_type == 3:
                # 加入集合
                existing.add(values[value])
            # 目标 SDK 版本
            if element == 'uses-sdk' and key == 'targetSdkVersion':
                # 按类型取值
                target_sdk = int(values[value]) if value_type == 3 else value
    # 为缺失的权限生成起止元素
    for permission in PERMISSIONS:
        # 已声明
        if permission in existing:
            # 跳过
            continue
        # 权限字符串的索引
        index = values.index(permission)
        # 起始元素
        nodes += struct.pack('<HHI4I6H', 0x102, 16, 56, 0, NO_INDEX, NO_INDEX, tag, 20, 20, 1, 0, 0, 0)
        # name 属性
        nodes += struct.pack('<IIIHBBI', namespace, name, index, 8, 0, 3, index)
        # 结束元素
        nodes += struct.pack('<HHI4I', 0x103, 16, 24, 0, NO_INDEX, NO_INDEX, tag)
    # 结果与是否已插入
    result = bytearray(); inserted = False
    # 重新拼接：字符串池换成新的，新节点插到 application 元素之前
    for kind, chunk in chunks:
        # 字符串表块
        if kind == 1:
            # 用新字符串表重写
            result += pool(values, utf8)
        # 其他块
        else:
            # 在 application 元素之前插入新节点
            if kind == 0x102 and values[struct.unpack_from('<I', chunk, 20)[0]] == 'application':
                # 插入节点
                result += nodes; inserted = True
            # 保留原块
            result += chunk
    # 没有 application 元素
    if not inserted:
        # 没有 application 元素
        raise RuntimeError('Manifest has no application element')
    # 重新打包并返回目标 SDK
    return struct.pack('<HHI', 3, 8, 8 + len(result)) + result, target_sdk


# 只改 Application 与运行器 Activity 的类名，保留导出状态、ID 与 intent 过滤器
def component_manifest(data):
    # 文档说明：只改 Application 与运行器 Activity 的类名
    """Change only Application/runner Activity class names; keep exported state, IDs and intent filters."""
    # 先补权限
    data, target = lan_permissions(data)
    # 块列表与偏移
    chunks = []; offset = 8
    # 逐块读取
    while offset < len(data):
        # 块类型与大小
        kind, _, size = struct.unpack_from('<HHI', data, offset)
        # 记录块
        chunks.append((kind, data[offset:offset + size]))
        # 前进
        offset += size
    # 取字符串表块
    string_chunk = next(chunk for kind, chunk in chunks if kind == 1)
    # 解析字符串表
    values, utf8 = strings(string_chunk)
    # 把新类名加入字符串池
    for value in [APPLICATION, ACTIVITY]:
        # 字符串不在表中
        if value not in values:
            # 追加
            values.append(value)
    # name 属性索引
    name = values.index('name')
    # 命名空间索引
    namespace = values.index('http://schemas.android.com/apk/res/android')
    # 资源 ID 映射块
    resources = []
    # 找资源映射块
    for kind, chunk in chunks:
        # 类型为资源映射
        if kind == 0x180:
            # 读出资源编号列表
            resources = list(struct.unpack('<' + 'I' * ((len(chunk) - 8) // 4), chunk[8:]))
    # 保证 name 属性有对应的资源 ID
    if len(resources) <= name:
        # 补齐到 name 的位置
        resources.extend([0] * (name + 1 - len(resources)))
    # name 必须对应 android:name
    if resources[name] not in {0, 0x01010003}:
        # 报错：name 属性的资源编号异常
        raise RuntimeError('Unexpected Android name attribute resource ID')
    # 设置为 android:name 的资源编号
    resources[name] = 0x01010003
    # 重建资源映射块头
    resource_chunk = struct.pack('<HHI', 0x180, 8, 8 + 4 * len(resources))
    # 追加资源编号
    resource_chunk += struct.pack('<' + 'I' * len(resources), *resources)
    # 结果、计数与是否已写资源映射
    result = bytearray(); application_count = 0; activity_count = 0; map_written = False
    # 重新拼接各块
    for kind, chunk in chunks:
        # 字符串池后紧跟资源映射块
        if kind == 1:
            # 写入字符串表
            result += pool(values, utf8)
            # 写入资源映射
            result += resource_chunk
            # 标记已写
            map_written = True
            # 处理下一块
            continue
        # 跳过旧的资源映射块
        if kind == 0x180:
            # 跳过
            continue
        # 起始元素
        if kind == 0x102:
            # 元素名
            element = values[struct.unpack_from('<I', chunk, 20)[0]]
            # 改 Application 类名
            if element == 'application':
                # application 计数加一
                application_count += 1
                # 改写 Application 的类名
                chunk = component_name(chunk, values, resources, namespace, name, APPLICATION)
            # 改运行器 Activity 类名
            elif element == 'activity' and attribute_text(chunk, values, namespace, name) in {RUNNER_ACTIVITY, ACTIVITY}:
                # activity 计数加一
                activity_count += 1
                # 改写运行器 Activity 的类名
                chunk = component_name(chunk, values, resources, namespace, name, ACTIVITY)
        # 追加该块
        result += chunk
    # 必须恰好各有一个 application 和运行器 activity
    if not map_written or application_count != 1 or activity_count != 1:
        # 报错：应恰有一个标准 application 与运行器 activity
        raise RuntimeError('Expected one standard NSH application and runner activity')
    # 重新打包并返回目标
    return struct.pack('<HHI', 3, 8, 8 + len(result)) + result, target


# 读取元素上 android:name 的字符串值
def attribute_text(chunk, values, namespace, name):
    # 属性起点、大小与数量
    start, size, count = struct.unpack_from('<3H', chunk, 24)
    # 逐个属性
    for position in range(count):
        # 属性偏移
        offset = 16 + start + position * size
        # 解出命名空间、名称、类型与值
        ns, key, _, _, _, kind, value = struct.unpack_from('<IIIHBBI', chunk, offset)
        # 是 android:name 且为字符串
        if ns == namespace and key == name and kind == 3:
            # 返回字符串值
            return values[value]
    # 没有该属性
    return None


# 把元素的 android:name 改为指定类名，没有则按资源 ID 顺序插入
def component_name(chunk, values, resources, namespace, name, value):
    # 读取属性起点、大小、数量与三个索引
    start, size, count, id_index, class_index, style_index = struct.unpack_from('<6H', chunk, 24)
    # 属性大小必须为 20
    if size != 20:
        # 报错
        raise RuntimeError('Unexpected Android attribute size')
    # 属性区起点
    begin = 16 + start
    # 逐个取出属性
    attributes = [chunk[begin + i * size:begin + (i + 1) * size] for i in range(count)]
    # 新的 name 属性
    replacement = struct.pack('<IIIHBBI', namespace, name, values.index(value), 8, 0, 3, values.index(value))
    # 已有 name 属性则替换
    for position, attribute in enumerate(attributes):
        # 命名空间与名称
        ns, key = struct.unpack_from('<II', attribute)
        # 找到 android:name
        if ns == namespace and key == name:
            # 替换为新属性
            attributes[position] = replacement
            # 结束
            break
    # 没有则插入；二进制属性按资源 ID 排序，同时调整 id/class/style 特殊属性的索引
    else:
        # 二进制 Android 属性按资源 ID 排序;保留特殊属性的索引。
        position = len(attributes)
        # 按资源编号排序，找插入位置
        for index, attribute in enumerate(attributes):
            # 属性名
            key = struct.unpack_from('<I', attribute, 4)[0]
            # 名称的资源编号大于 name 则插在其前
            if key < len(resources) and resources[key] > resources[name]:
                # 记录位置
                position = index
                # 结束
                break
        # 插入新属性
        attributes.insert(position, replacement)
        # id 下标随插入位置调整
        id_index = id_index + 1 if id_index > position else id_index
        # class 下标随插入位置调整
        class_index = class_index + 1 if class_index > position else class_index
        # style 下标随插入位置调整
        style_index = style_index + 1 if style_index > position else style_index
    # 复制块头
    header = bytearray(chunk[:begin])
    # 更新属性数量与特殊索引
    struct.pack_into('<4H', header, 28, len(attributes), id_index, class_index, style_index)
    # 重组属性区与剩余部分
    result = header + b''.join(attributes) + chunk[begin + count * size:]
    # 更新块长度
    struct.pack_into('<I', result, 4, len(result))
    # 返回新块
    return result
