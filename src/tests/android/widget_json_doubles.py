"""有界的 org.json 替身源码：只为小组件夹具的 JSON 子集服务。

宿主 JVM 上没有可用的 org.json 实现（平台 jar 是编译期桩），而小组件提供器的
渲染路径依赖 JSON 解析。这里按夹具需要的语法子集（对象、数组、字符串、数值、
布尔与 null）实现最小解析，语义以小组件响应的实际形状为准；超出子集的输入
一律报错，不猜测。
"""

# 替身源码：与 AndroidApplicationEntry 的替身字典同构
SOURCES = {
    'org/json/JSONException.java': '''package org.json;
// JSONException 替身
public class JSONException extends Exception {
    // 携带原因构造
    public JSONException(String message) { super(message); }
}
''',
    'org/json/JSONArray.java': '''package org.json;
import java.util.ArrayList;
import java.util.List;
// JSONArray 替身：包装解析出的值列表
public class JSONArray {
    // 元素列表
    final List<Object> values = new ArrayList<>();
    // 元素数
    public int length() { return values.size(); }
    // 取下标的对象；越界或类型不符返回 null
    public JSONObject optJSONObject(int index) {
        return index >= 0 && index < values.size() && values.get(index) instanceof JSONObject
            ? (JSONObject) values.get(index) : null;
    }
}
''',
    'org/json/JSONObject.java': '''package org.json;
import java.util.LinkedHashMap;
import java.util.Map;
// JSONObject 替身：覆盖小组件提供器用到的取值接口
public class JSONObject {
    // 键值表
    final Map<String, Object> values = new LinkedHashMap<>();
    // 空对象
    public JSONObject() {}
    // 解析文本
    public JSONObject(String source) throws JSONException {
        new Parser(source).parse(this);
    }
    // 写入键值
    public JSONObject put(String key, Object value) { values.put(key, value); return this; }
    // 字符串取值；非字符串回空串
    public String optString(String key) {
        Object value = values.get(key);
        return value instanceof String ? (String) value : "";
    }
    // 字符串取值；缺失回退
    public String optString(String key, String fallback) {
        Object value = values.get(key);
        return value instanceof String ? (String) value : fallback;
    }
    // 必须存在的字符串
    public String getString(String key) throws JSONException {
        Object value = values.get(key);
        if (!(value instanceof String)) throw new JSONException("missing " + key);
        return (String) value;
    }
    // 取子对象；缺失或类型不符返回 null
    public JSONObject optJSONObject(String key) {
        return values.get(key) instanceof JSONObject ? (JSONObject) values.get(key) : null;
    }
    // 取数组；缺失或类型不符返回 null
    public JSONArray optJSONArray(String key) {
        return values.get(key) instanceof JSONArray ? (JSONArray) values.get(key) : null;
    }
    // 长整型取值；缺失回退 0
    public long optLong(String key) { return optLong(key, 0); }
    // 长整型取值；缺失或不可解析回退
    public long optLong(String key, long fallback) {
        Object value = values.get(key);
        if (value instanceof Number) return ((Number) value).longValue();
        if (value instanceof String) {
            try { return Long.parseLong((String) value); } catch (NumberFormatException error) { return fallback; }
        }
        return fallback;
    }
    // 是否包含键
    public boolean has(String key) { return values.containsKey(key); }
    // 值是否为 null
    public boolean isNull(String key) { return values.get(key) == null; }
    // 诊断用序列化
    @Override public String toString() { return values.toString(); }

    // 极简递归下降解析器：对象、数组、字符串（常见转义）、数值、布尔与 null
    private static final class Parser {
        // 文本
        private final String text;
        // 位置
        private int position;
        // 
        Parser(String text) { this.text = text; }
        // 解析整个文本为对象
        void parse(JSONObject root) throws JSONException {
            skip();
            parseInto(root);
            skip();
            if (position != text.length()) throw new JSONException("trailing content at " + position);
        }
        // 解析对象正文
        private void parseInto(JSONObject target) throws JSONException {
            expect('{');
            skip();
            if (peek() == '}') { position++; return; }
            while (true) {
                skip();
                String key = parseString();
                skip();
                expect(':');
                skip();
                target.values.put(key, parseValue());
                skip();
                if (peek() == '}') { position++; return; }
                expect(',');
            }
        }
        // 解析数组正文
        private JSONArray parseArray() throws JSONException {
            expect('[');
            skip();
            JSONArray array = new JSONArray();
            if (peek() == ']') { position++; return array; }
            while (true) {
                skip();
                array.values.add(parseValue());
                skip();
                if (peek() == ']') { position++; return array; }
                expect(',');
            }
        }
        // 解析任意值
        private Object parseValue() throws JSONException {
            char current = peek();
            if (current == '{') {
                JSONObject object = new JSONObject();
                parseInto(object);
                return object;
            }
            if (current == '[') return parseArray();
            if (current == '"') return parseString();
            if (match("true")) return Boolean.TRUE;
            if (match("false")) return Boolean.FALSE;
            if (match("null")) return null;
            return parseNumber();
        }
        // 解析数值为长整型
        private Long parseNumber() throws JSONException {
            int start = position;
            while (position < text.length() && "+-0123456789.eE".indexOf(text.charAt(position)) >= 0) position++;
            try {
                return (long) Double.parseDouble(text.substring(start, position));
            } catch (NumberFormatException error) {
                throw new JSONException("bad number at " + start);
            }
        }
        // 解析字符串与常见转义
        private String parseString() throws JSONException {
            expect('"');
            StringBuilder builder = new StringBuilder();
            while (true) {
                char current = text.charAt(position++);
                if (current == '"') return builder.toString();
                if (current == '\\\\') {
                    char escape = text.charAt(position++);
                    if (escape == 'n') builder.append('\\n');
                    else if (escape == 't') builder.append('\\t');
                    else if (escape == 'r') builder.append('\\r');
                    else builder.append(escape);
                } else {
                    builder.append(current);
                }
            }
        }
        // 跳过空白
        private void skip() { while (position < text.length() && Character.isWhitespace(text.charAt(position))) position++; }
        // 当前字符
        private char peek() throws JSONException {
            if (position >= text.length()) throw new JSONException("unexpected end");
            return text.charAt(position);
        }
        // 期望字符
        private void expect(char expected) throws JSONException {
            if (peek() != expected) throw new JSONException("expected " + expected + " at " + position);
            position++;
        }
        // 匹配字面量
        private boolean match(String literal) {
            if (text.startsWith(literal, position)) { position += literal.length(); return true; }
            return false;
        }
    }
}
''',
}
