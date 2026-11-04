# 文档字符串：解析应用 resources.arsc 的资源项名，用于清单引用还原
"""Parse the app resources.arsc to map resource IDs to type/name.

只提取把二进制清单还原为文本所需的最小信息：包名与每个资源项的
(type, name)。值与配置不是目标，遇到不认识的结构一律报错。
"""
# 引入结构解析
import struct

# 表与包块类型
TABLE, PACKAGE = 0x0002, 0x0200
# 类型规格与类型块
TYPE_SPEC, TYPE = 0x0202, 0x0201
# 字符串池
STRING_POOL = 0x0001



# 读取字符串池（与清单格式一致），返回字符串列表
def strings(data, offset):
    # 文档字符串：解析字符串池块
    """Parse a string pool chunk and return its strings."""
    # 字符串数、样式数、标志与数据起点
    count, styles, flags, start, _ = struct.unpack_from('<5I', data, offset + 8)
    # 带样式的池不支持
    if styles:
        # 报错
        raise RuntimeError('Unexpected styled arsc string pool')
    # 是否 UTF-8
    utf8 = bool(flags & 0x100)
    # 结果
    values = []
    # 逐项读取
    for index in range(count):
        # 项偏移
        position = offset + start + struct.unpack_from('<I', data, offset + 28 + index * 4)[0]
        # UTF-16 单元数
        units, position = length(data, position, utf8)
        # UTF-8 池：单元数后再读字节数
        if utf8:
            # 字节数
            size, position = length(data, position, True)
            # 解码
            values.append(data[position:position + size].decode('utf-8'))
        # 否则按 UTF-16
        else:
            # 解码
            values.append(data[position:position + units * 2].decode('utf-16-le'))
    # 返回字符串
    return values


# 读取长度前缀，返回长度与新偏移
def length(data, offset, utf8):
    # 文档字符串：UTF-8 为 1-2 字节长度，UTF-16 为 2-4 字节长度
    """Read a length prefix in either encoding."""
    # UTF-8
    if utf8:
        # 高位为 1 表示还有第二字节
        first = data[offset]
        # 返回
        return (((first & 0x7F) << 8) | data[offset + 1], offset + 2) if first & 0x80 else (first, offset + 1)
    # UTF-16 首字
    first = struct.unpack_from('<H', data, offset)[0]
    # 返回
    return (((first & 0x7FFF) << 16) | struct.unpack_from('<H', data, offset + 2)[0], offset + 4) \
        if first & 0x8000 else (first, offset + 2)


# 解析 arsc，返回 (包名, 资源ID 到 type/name 的映射)
def entries(data):
    # 文档字符串：遍历表块并聚合全部资源项名
    """Parse an app resources.arsc into (package name, {id: (type, name)})."""
    # 必须是资源表
    if struct.unpack_from('<H', data, 0)[0] != TABLE:
        # 报错
        raise RuntimeError('Expected a resources.arsc table')
    # 应用资源表恰好一个包
    if struct.unpack_from('<I', data, 8)[0] != 1:
        # 报错
        raise RuntimeError('Expected exactly one arsc package')
    # 逐块扫描：表头之后开始（头大小位于偏移 2）
    offset = struct.unpack_from('<H', data, 2)[0]
    # 收集器
    package_id = None
    package_name = None
    type_names = None
    key_names = None
    mapping = {}
    # 循环
    while offset < len(data):
        # 块类型与大小
        kind, header_size, size = struct.unpack_from('<HHI', data, offset)
        # 大小越界
        if size < 8 or offset + size > len(data):
            # 报错
            raise RuntimeError('Invalid arsc chunk')
        # 字符串池：跳过（包内字符串池按偏移单独解析）
        if kind == STRING_POOL:
            # 前进
            offset += size
            # 下一块
            continue
        # 包块
        if kind == PACKAGE:
            # 包 ID（应用包固定 0x7f）
            package_id = struct.unpack_from('<I', data, offset + 8)[0]
            # 包名占位
            package_name = None
            # 包名：256 字节 UTF-16，NUL 结尾
            raw_name = data[offset + 12:offset + 12 + 256]
            # 解码并截断
            package_name = raw_name.decode('utf-16-le').split('\x00')[0]
            # 类型与键字符串池的相对偏移
            type_strings, _, key_strings, _ = struct.unpack_from('<4I', data, offset + 268)
            # 解析两个池
            type_names = strings(data, offset + type_strings)
            key_names = strings(data, offset + key_strings)
            # 遍历包内子块（从包头大小开始）
            inner = offset + header_size
            # 子块循环
            while inner < offset + size:
                # 子块类型与大小
                inner_kind, inner_header, inner_size = struct.unpack_from('<HHI', data, inner)
                # 大小越界
                if inner_size < 8 or inner + inner_size > offset + size:
                    # 报错
                    raise RuntimeError('Invalid arsc package chunk')
                # 类型块：头部之后是 config，config 之后是项偏移表，最后是项数据
                if inner_kind == TYPE:
                    # 类型 ID、项数与项区起点（相对类型块起点）
                    type_id, _, _, entry_count, entries_start = \
                        struct.unpack_from('<BBHII', data, inner + 8)
                    # 类型名必须存在
                    if type_id < 1 or type_id > len(type_names):
                        # 报错
                        raise RuntimeError('Arsc type id outside type string pool')
                    # 类型名
                    type_name = type_names[type_id - 1]
                    # 项偏移表位于包头之后（headerSize 覆盖 config）
                    offsets = inner + inner_header
                    # 逐项解析；应用资源表只使用标准 32 位字节偏移
                    for entry_index in range(entry_count):
                        # 项偏移（相对 entries_start）
                        entry_offset = struct.unpack_from('<I', data, offsets + entry_index * 4)[0]
                        # 偏移为 NO_ENTRY 表示该项在此配置下不存在
                        if entry_offset == 0xFFFFFFFF:
                            # 跳过
                            continue
                        # 项起点
                        entry = inner + entries_start + entry_offset
                        # 项大小、标志与键索引
                        _, _, key_index = struct.unpack_from('<HHI', data, entry)
                        # 键名必须存在
                        if key_index >= len(key_names):
                            # 报错
                            raise RuntimeError('Arsc entry key outside key string pool')
                        # 记录资源 ID 到类型与键名
                        resource_id = (package_id << 24) | (type_id << 16) | entry_index
                        # 记录
                        mapping[resource_id] = (type_name, key_names[key_index])
                # 类型规格块与其余子块：不需要
                # 前进
                inner += inner_size
        # 其它顶层块不支持
        elif kind not in (TYPE_SPEC, TYPE):
            # 报错
            raise RuntimeError(f'Unsupported top-level arsc chunk 0x{kind:04x}')
        # 前进
        offset += size
    # 必须解析出包名与资源项
    if package_name is None or not mapping:
        # 报错
        raise RuntimeError('Arsc has no package or entries')
    # 返回
    return package_name, mapping
