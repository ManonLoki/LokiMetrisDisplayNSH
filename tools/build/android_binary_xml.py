# 文档字符串：把二进制 Android XML 清单无损转回文本 XML，供 aapt2 重新链接
"""Convert a binary Android XML manifest back to text XML for aapt2 relinking.

只覆盖清单出现的块类型（字符串池、命名空间、元素与属性值类型）。
转换是结构化的：未知值类型一律报错，不猜测；引用由调用方注入的两张
ID 到名字映射表（android 系统资源与应用资源）解析。
"""
# 引入结构解析
import struct
# 引入 XML 输出
import xml.etree.ElementTree as ET

# 属性/字符串索引的空值
NO_INDEX = 0xFFFFFFFF
# 值类型：引用、属性、字符串、浮点、尺寸、分数、十进制与十六进制整数、布尔
REFERENCE, ATTRIBUTE, STRING, FLOAT, DIMENSION, FRACTION = 0x01, 0x02, 0x03, 0x04, 0x05, 0x06
INT_DEC, INT_HEX, BOOLEAN = 0x10, 0x11, 0x12
# 尺寸单位
DIMENSION_UNITS = ('px', 'dip', 'sp', 'pt', 'in', 'mm')

# NSH 基线清单中出现的旗标属性：位值到符号名（与 aapt2 的属性定义一致）；
# 出现表外旗标属性时失败关闭，提示扩表而不是输出坏清单
FLAG_ATTRIBUTES = {
    # 活动配置变化位
    'configChanges': [
        (0x1, 'mcc'), (0x2, 'mnc'), (0x4, 'locale'), (0x8, 'touchscreen'),
        (0x10, 'keyboard'), (0x20, 'keyboardHidden'), (0x40, 'navigation'),
        (0x80, 'orientation'), (0x100, 'screenLayout'), (0x200, 'uiMode'),
        (0x400, 'screenSize'), (0x800, 'smallestScreenSize'), (0x1000, 'density'),
        (0x2000, 'layoutDirection'), (0x4000, 'colorMode'), (0x8000, 'grammaticalGender'),
        (0x8000000, 'resourcesUnused'), (0x10000000, 'fontWeightAdjustment'),
        (0x40000000, 'fontScale'), (0x80000000, 'assetsPaths'),
    ],
}
# 窗口软输入模式的两个组成部分：状态枚举与调整枚举
SOFT_INPUT_STATE = {0: 'stateUnspecified', 1: 'stateUnchanged', 2: 'stateHidden',
                    3: 'stateAlwaysHidden', 4: 'stateVisible', 5: 'stateAlwaysVisible'}
SOFT_INPUT_ADJUST = {1: 'adjustResize', 2: 'adjustPan', 3: 'adjustNothing'}


# 把旗标属性值还原为符号名组合
def format_flags(attribute, data):
    # 文档字符串：按属性查表还原符号名
    """Render a flag attribute value as symbolic names."""
    # 窗口软输入模式：低 4 位状态枚举加调整枚举
    if attribute == 'windowSoftInputMode':
        # 状态与调整部分
        state = data & 0xF
        adjust = (data >> 4) & 0x3
        # 缺省项可省略
        parts = [SOFT_INPUT_STATE[state]] if state else []
        # 追加调整部分
        parts += [SOFT_INPUT_ADJUST[adjust]] if adjust else []
        # 至少给出缺省
        return '|'.join(parts) if parts else 'stateUnspecified|adjustUnspecified'
    # 表内旗标属性
    table = FLAG_ATTRIBUTES.get(attribute)
    # 表外属性失败关闭
    if table is None:
        # 报错
        raise RuntimeError(f'Hex manifest attribute {attribute!r} needs a flag table entry')
    # 逐位还原并核对全部位都有定义
    names = []
    # 已定义位的并集
    covered = 0
    # 遍历表
    for bit, name in table:
        # 命中位
        if data & bit:
            # 记录名字
            names.append(name)
        # 累计
        covered |= bit
    # 存在未定义位
    if data & ~covered & 0xFFFFFFFF:
        # 报错
        raise RuntimeError(f'Flag attribute {attribute!r} has undefined bits 0x{data & ~covered & 0xFFFFFFFF:08x}')
    # 值为 0 无名字
    if not names:
        # 报错
        raise RuntimeError(f'Flag attribute {attribute!r} has value 0')
    # 组合
    return '|'.join(names)


# 读取字符串池，返回字符串列表与是否 UTF-8
def string_pool(data, offset):
    # 文档字符串：解析 RES_STRING_POOL_TYPE
    """Parse a RES_STRING_POOL_TYPE chunk."""
    # 字符串数、样式数、标志与数据起点
    count, styles, flags, start, _ = struct.unpack_from('<5I', data, offset + 8)
    # 不支持带样式的池
    if styles:
        # 报错：清单字符串池不应带样式
        raise RuntimeError('Unexpected styled manifest string pool')
    # 是否 UTF-8
    utf8 = bool(flags & 0x100)
    # 字符串列表
    values = []
    # 逐个读取
    for index in range(count):
        # 字符串偏移
        position = offset + start + struct.unpack_from('<I', data, offset + 28 + index * 4)[0]
        # 读取 UTF-16 单元数
        units, position = length(data, position, utf8)
        # UTF-8 池：单元数后再读字节数
        if utf8:
            # 再读字节数
            size, position = length(data, position, True)
            # 解码
            values.append(data[position:position + size].decode('utf-8'))
        # 否则按 UTF-16 读取
        else:
            # 解码
            values.append(data[position:position + units * 2].decode('utf-16-le'))
    # 返回字符串表
    return values, utf8


# 读取长度前缀，返回长度与新偏移
def length(data, offset, utf8):
    # 文档字符串：UTF-8 为 1-2 字节长度，UTF-16 为 2-4 字节长度
    """Read a length prefix in either encoding."""
    # UTF-8
    if utf8:
        # 高位为 1 表示还有第二字节
        first = data[offset]
        # 返回长度与新偏移
        return (((first & 0x7F) << 8) | data[offset + 1], offset + 2) if first & 0x80 else (first, offset + 1)
    # UTF-16 首字
    first = struct.unpack_from('<H', data, offset)[0]
    # 返回长度与新偏移
    return (((first & 0x7FFF) << 16) | struct.unpack_from('<H', data, offset + 2)[0], offset + 4) \
        if first & 0x8000 else (first, offset + 2)


# 格式化单个属性值；android_names/app_names 是 ID 到 type/name 的映射
def format_value(attribute, data_type, data, raw, strings, android_names, app_names):
    # 文档字符串：按值类型输出文本清单可接受的写法
    """Render one attribute value as text-XML content."""
    # 字符串
    if data_type == STRING:
        # 优先原始值，其次字符串表
        return raw if raw is not None else strings[data]
    # 十进制整数
    if data_type == INT_DEC:
        # 有符号 32 位
        return str(struct.unpack('<i', struct.pack('<I', data))[0])
    # 十六进制整数：旗标属性还原为符号名，文本清单不接受十六进制字面量
    if data_type == INT_HEX:
        # 按属性查旗标表
        return format_flags(attribute, data)
    # 布尔
    if data_type == BOOLEAN:
        # 零为假
        return 'true' if data else 'false'
    # 引用
    if data_type == REFERENCE:
        # Android 系统资源
        if data >> 24 == 0x01:
            # 查不到名字说明映射表不完整，报错而不是输出坏引用
            return '@android:' + required(android_names, data)
        # 应用资源：映射值为 type/name 文本
        if data >> 24 == 0x7F:
            # 查不到名字说明 arsc 映射不完整
            kind, name = required(app_names, data)
            # 组合为文本引用
            return f'@{kind}/{name}'
        # 其它包引用不支持
        raise RuntimeError(f'Unsupported resource reference 0x{data:08x}')
    # 尺寸
    if data_type == DIMENSION:
        # 复数编码：低 8 位单位，高 24 位尾数（移位表示）
        unit = data & 0xFF
        # 尾数按移位解释
        mantissa = data >> 8
        # 复数到浮点：低 4 位是指数移位
        shift = mantissa & 0x3
        # 数值
        value = (mantissa >> 4) / (1 << (shift * 4)) if shift else (mantissa >> 4)
        # 输出带单位
        return f'{value:g}{DIMENSION_UNITS[unit] if unit < len(DIMENSION_UNITS) else "u" + str(unit)}'
    # 分数：清单不出现，保守不支持
    if data_type == FRACTION:
        # 报错
        raise RuntimeError('Fraction manifest values are not supported')
    # 浮点
    if data_type == FLOAT:
        # 按位还原
        return str(struct.unpack('<f', struct.pack('<I', data))[0])
    # 其它类型一律报错
    raise RuntimeError(f'Unsupported manifest attribute type 0x{data_type:02x}')


# 取映射中必须存在的名字
def required(names, key):
    # 文档字符串：映射缺失即报错
    """Look up a resource name, failing closed."""
    # 缺失即失败
    if key not in names:
        # 报错
        raise RuntimeError(f'Resource name mapping lacks 0x{key:08x}')
    # 返回名字
    return names[key]


# 把二进制清单转为文本 XML；android_names 与 app_names 解析资源引用
def to_text(data, android_names, app_names):
    # 文档字符串：完整的清单块遍历与文本输出
    """Convert binary manifest bytes to a text XML string."""
    # 必须是二进制 Android XML
    if data[:4] != b'\x03\x00\x08\x00':
        # 报错
        raise RuntimeError('Expected binary Android XML')
    # 字符串池与内容块列表
    strings = None
    chunks = []
    # 逐块扫描
    offset = 8
    # 循环
    while offset < len(data):
        # 块类型与大小
        kind, _, size = struct.unpack_from('<HHI', data, offset)
        # 大小越界
        if size < 8 or offset + size > len(data):
            # 报错
            raise RuntimeError('Invalid binary manifest chunk')
        # 字符串池单独解析
        if kind == 0x0001:
            # 解析字符串表
            strings, _ = string_pool(data, offset)
        # 其余内容块按顺序保留
        else:
            # 追加
            chunks.append((kind, data[offset:offset + size]))
        # 前进
        offset += size
    # 必须有字符串表
    if strings is None:
        # 报错
        raise RuntimeError('Manifest has no string pool')
    # 元素栈与根
    stack = []
    root = None
    # 逐块处理
    for kind, chunk in chunks:
        # 资源映射块（0x0180）：aapt2 重新链接时重建，跳过
        # 命名空间开始（0x0100）与结束（0x0101）：文本输出由根元素统一声明，无需处理
        if kind in (0x0100, 0x0101, 0x0180):
            # 跳过
            continue
        # 元素开始
        if kind == 0x0102:
            # 元素命名空间、名称与属性区描述
            element_ns, name_index, attribute_start, attribute_size, attribute_count = \
                struct.unpack_from('<II3H', chunk, 16)
            # 元素名（清单元素都不带命名空间）
            element = ET.Element(strings[name_index])
            # 根元素
            if root is None:
                # 保存根
                root = element
            # 有父元素则挂载
            if stack:
                # 挂到父元素
                stack[-1].append(element)
            # 入栈
            stack.append(element)
            # 逐个属性
            for index in range(attribute_count):
                # 属性偏移：属性区从块头 16 字节后开始
                position = 16 + attribute_start + index * attribute_size
                # 命名空间、名称、原始值、大小、保留、类型与数据
                attribute_ns, attribute_name, raw_value, _, _, value_type, value = \
                    struct.unpack_from('<IIIHBBI', chunk, position)
                # 属性命名空间（无命名空间用空键）
                namespace = strings[attribute_ns] if attribute_ns != NO_INDEX else None
                # 属性名：android 命名空间加前缀
                key = f'android:{strings[attribute_name]}' if namespace else strings[attribute_name]
                # 旗标表查找用的裸属性名
                attribute = strings[attribute_name]
                # 字符串值：raw_value 是字符串表下标，缺失时用 data 下标
                if value_type == STRING:
                    # 原始值优先
                    raw = None if raw_value == NO_INDEX else strings[raw_value]
                    # 格式化
                    text_value = format_value(attribute, STRING, value, raw, strings, android_names, app_names)
                # 其它类型没有原始值
                else:
                    # 格式化
                    text_value = format_value(attribute, value_type, value, None, strings, android_names, app_names)
                # 写入属性
                element.set(key, text_value)
            # 继续下一块
            continue
        # 元素结束
        if kind == 0x0103:
            # 出栈
            stack.pop()
            # 继续下一块
            continue
        # 其它块不支持
        raise RuntimeError(f'Unsupported manifest chunk 0x{kind:04x}')
    # 必须恰好闭合
    if root is None or stack:
        # 报错
        raise RuntimeError('Manifest elements did not close')
    # 根元素声明 android 命名空间
    root.set('xmlns:android', 'http://schemas.android.com/apk/res/android')
    # 输出文本（带换行缩进便于审阅）
    ET.indent(root)
    # 返回文本
    return ET.tostring(root, encoding='unicode')


# 把文本 XML 编码回二进制清单
def from_text(text):
    # 文档字符串：交由 aapt2 完成二进制编码，这里不实现
    """Binary encoding is delegated to aapt2; this module never writes AXML."""
    # 明确不支持
    raise RuntimeError('Binary manifest encoding is delegated to aapt2')
