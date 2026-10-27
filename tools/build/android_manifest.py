"""Preserve runner metadata while adding LAN permissions and app-owned Android entry points."""
import struct

NO_INDEX = 0xffffffff
PERMISSIONS = ['android.permission.ACCESS_NETWORK_STATE', 'android.permission.CHANGE_WIFI_MULTICAST_STATE']
APPLICATION = 'com.manonloki.lokimetrisdisplay.bridge.DisplayApplication'
ACTIVITY = 'com.manonloki.lokimetrisdisplay.bridge.DisplayActivity'
RUNNER_ACTIVITY = 'com.longbridge.nsh.AppActivity'


def length(data, offset, utf8):
    if utf8:
        first = data[offset]
        return (((first & 0x7f) << 8) | data[offset + 1], offset + 2) if first & 0x80 else (first, offset + 1)
    first = struct.unpack_from('<H', data, offset)[0]
    return (((first & 0x7fff) << 16) | struct.unpack_from('<H', data, offset + 2)[0], offset + 4) if first & 0x8000 else (first, offset + 2)


def encoded_length(value, utf8):
    if utf8:
        return bytes([value]) if value < 0x80 else bytes([(value >> 8) | 0x80, value & 0xff])
    return struct.pack('<H', value) if value < 0x8000 else struct.pack('<HH', (value >> 16) | 0x8000, value & 0xffff)


def strings(chunk):
    count, styles, flags, start, _ = struct.unpack_from('<5I', chunk, 8)
    if styles:
        raise RuntimeError('Unexpected styled manifest string pool')
    utf8 = bool(flags & 0x100)
    values = []
    for index in range(count):
        offset = start + struct.unpack_from('<I', chunk, 28 + index * 4)[0]
        units, offset = length(chunk, offset, utf8)
        if utf8:
            size, offset = length(chunk, offset, True)
            values.append(chunk[offset:offset + size].decode('utf-8'))
        else:
            values.append(chunk[offset:offset + units * 2].decode('utf-16-le'))
    return values, utf8


def pool(values, utf8):
    body = bytearray(); offsets = []
    for value in values:
        offsets.append(len(body))
        units = len(value.encode('utf-16-le')) // 2
        if utf8:
            encoded = value.encode('utf-8')
            body += encoded_length(units, True) + encoded_length(len(encoded), True) + encoded + b'\0'
        else:
            body += encoded_length(units, False) + value.encode('utf-16-le') + b'\0\0'
    body += bytes((-len(body)) % 4)
    start = 28 + len(values) * 4
    return (struct.pack('<HHI5I', 1, 28, start + len(body), len(values), 0, 0x100 if utf8 else 0, start, 0)
            + struct.pack('<' + 'I' * len(offsets), *offsets) + body)


def lan_permissions(data):
    if data[:4] != b'\x03\0\x08\0':
        raise RuntimeError('Expected binary Android XML')
    chunks = []; offset = 8; values = None; target_sdk = None
    while offset < len(data):
        kind, _, size = struct.unpack_from('<HHI', data, offset)
        if size < 8 or offset + size > len(data):
            raise RuntimeError('Invalid binary manifest chunk')
        chunk = data[offset:offset + size]
        chunks.append((kind, chunk)); offset += size
        if kind == 1:
            values, utf8 = strings(chunk)
    if values is None:
        raise RuntimeError('Manifest has no string pool')
    for value in ['uses-permission', 'name', 'http://schemas.android.com/apk/res/android'] + PERMISSIONS:
        if value not in values:
            values.append(value)
    nodes = bytearray()
    tag = values.index('uses-permission'); name = values.index('name')
    namespace = values.index('http://schemas.android.com/apk/res/android')
    existing = set()
    for kind, chunk in chunks:
        if kind != 0x102:
            continue
        element = values[struct.unpack_from('<I', chunk, 20)[0]]
        start, size, count = struct.unpack_from('<3H', chunk, 24)
        for i in range(count):
            attribute = 16 + start + i * size
            key = values[struct.unpack_from('<I', chunk, attribute + 4)[0]]
            value_type = chunk[attribute + 15]
            value = struct.unpack_from('<I', chunk, attribute + 16)[0]
            if element == 'uses-permission' and key == 'name' and value_type == 3:
                existing.add(values[value])
            if element == 'uses-sdk' and key == 'targetSdkVersion':
                target_sdk = int(values[value]) if value_type == 3 else value
    for permission in PERMISSIONS:
        if permission in existing:
            continue
        index = values.index(permission)
        nodes += struct.pack('<HHI4I6H', 0x102, 16, 56, 0, NO_INDEX, NO_INDEX, tag, 20, 20, 1, 0, 0, 0)
        nodes += struct.pack('<IIIHBBI', namespace, name, index, 8, 0, 3, index)
        nodes += struct.pack('<HHI4I', 0x103, 16, 24, 0, NO_INDEX, NO_INDEX, tag)
    result = bytearray(); inserted = False
    for kind, chunk in chunks:
        if kind == 1:
            result += pool(values, utf8)
        else:
            if kind == 0x102 and values[struct.unpack_from('<I', chunk, 20)[0]] == 'application':
                result += nodes; inserted = True
            result += chunk
    if not inserted:
        raise RuntimeError('Manifest has no application element')
    return struct.pack('<HHI', 3, 8, 8 + len(result)) + result, target_sdk


def component_manifest(data):
    """Change only Application/runner Activity class names; keep exported state, IDs and intent filters."""
    data, target = lan_permissions(data)
    chunks = []; offset = 8
    while offset < len(data):
        kind, _, size = struct.unpack_from('<HHI', data, offset)
        chunks.append((kind, data[offset:offset + size]))
        offset += size
    string_chunk = next(chunk for kind, chunk in chunks if kind == 1)
    values, utf8 = strings(string_chunk)
    for value in [APPLICATION, ACTIVITY]:
        if value not in values:
            values.append(value)
    name = values.index('name')
    namespace = values.index('http://schemas.android.com/apk/res/android')
    resources = []
    for kind, chunk in chunks:
        if kind == 0x180:
            resources = list(struct.unpack('<' + 'I' * ((len(chunk) - 8) // 4), chunk[8:]))
    if len(resources) <= name:
        resources.extend([0] * (name + 1 - len(resources)))
    if resources[name] not in {0, 0x01010003}:
        raise RuntimeError('Unexpected Android name attribute resource ID')
    resources[name] = 0x01010003
    resource_chunk = struct.pack('<HHI', 0x180, 8, 8 + 4 * len(resources))
    resource_chunk += struct.pack('<' + 'I' * len(resources), *resources)
    result = bytearray(); application_count = 0; activity_count = 0; map_written = False
    for kind, chunk in chunks:
        if kind == 1:
            result += pool(values, utf8)
            result += resource_chunk
            map_written = True
            continue
        if kind == 0x180:
            continue
        if kind == 0x102:
            element = values[struct.unpack_from('<I', chunk, 20)[0]]
            if element == 'application':
                application_count += 1
                chunk = component_name(chunk, values, resources, namespace, name, APPLICATION)
            elif element == 'activity' and attribute_text(chunk, values, namespace, name) in {RUNNER_ACTIVITY, ACTIVITY}:
                activity_count += 1
                chunk = component_name(chunk, values, resources, namespace, name, ACTIVITY)
        result += chunk
    if not map_written or application_count != 1 or activity_count != 1:
        raise RuntimeError('Expected one standard NSH application and runner activity')
    return struct.pack('<HHI', 3, 8, 8 + len(result)) + result, target


def attribute_text(chunk, values, namespace, name):
    start, size, count = struct.unpack_from('<3H', chunk, 24)
    for position in range(count):
        offset = 16 + start + position * size
        ns, key, _, _, _, kind, value = struct.unpack_from('<IIIHBBI', chunk, offset)
        if ns == namespace and key == name and kind == 3:
            return values[value]
    return None


def component_name(chunk, values, resources, namespace, name, value):
    start, size, count, id_index, class_index, style_index = struct.unpack_from('<6H', chunk, 24)
    if size != 20:
        raise RuntimeError('Unexpected Android attribute size')
    begin = 16 + start
    attributes = [chunk[begin + i * size:begin + (i + 1) * size] for i in range(count)]
    replacement = struct.pack('<IIIHBBI', namespace, name, values.index(value), 8, 0, 3, values.index(value))
    for position, attribute in enumerate(attributes):
        ns, key = struct.unpack_from('<II', attribute)
        if ns == namespace and key == name:
            attributes[position] = replacement
            break
    else:
        # Binary Android attributes are ordered by resource ID; retain special-attribute indices.
        position = len(attributes)
        for index, attribute in enumerate(attributes):
            key = struct.unpack_from('<I', attribute, 4)[0]
            if key < len(resources) and resources[key] > resources[name]:
                position = index
                break
        attributes.insert(position, replacement)
        id_index = id_index + 1 if id_index > position else id_index
        class_index = class_index + 1 if class_index > position else class_index
        style_index = style_index + 1 if style_index > position else style_index
    header = bytearray(chunk[:begin])
    struct.pack_into('<4H', header, 28, len(attributes), id_index, class_index, style_index)
    result = header + b''.join(attributes) + chunk[begin + count * size:]
    struct.pack_into('<I', result, 4, len(result))
    return result
