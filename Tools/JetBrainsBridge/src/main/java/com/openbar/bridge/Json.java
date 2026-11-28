// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

package com.openbar.bridge;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Just enough JSON for the island's one-line requests and the answers to
 * them: objects, arrays, strings, numbers, true, false and null. The bridge
 * keeps to the JDK so it never depends on a library an IDE may move.
 */
final class Json {
    private final String text;
    private int at;

    private Json(String text) {
        this.text = text;
    }

    /** The value in {@code text}, or an exception naming where it stopped making sense. */
    static Object parse(String text) {
        Json json = new Json(text);
        json.space();
        Object value = json.value(0);
        json.space();
        if (json.at != text.length()) throw json.error("trailing text");
        return value;
    }

    private IllegalArgumentException error(String what) {
        return new IllegalArgumentException(what + " at " + at);
    }

    private void space() {
        while (at < text.length() && " \t\r\n".indexOf(text.charAt(at)) >= 0) at++;
    }

    private Object value(int depth) {
        if (depth > 32) throw error("too deep");
        if (at >= text.length()) throw error("end of text");
        char c = text.charAt(at);
        switch (c) {
            case '{': return object(depth);
            case '[': return array(depth);
            case '"': return string();
            case 't': return word("true", Boolean.TRUE);
            case 'f': return word("false", Boolean.FALSE);
            case 'n': return word("null", null);
            default:
                if (c == '-' || (c >= '0' && c <= '9')) return number();
                throw error("unexpected character");
        }
    }

    private Object word(String word, Object value) {
        if (!text.startsWith(word, at)) throw error("unexpected word");
        at += word.length();
        return value;
    }

    private Map<String, Object> object(int depth) {
        Map<String, Object> object = new LinkedHashMap<>();
        at++;
        space();
        if (at < text.length() && text.charAt(at) == '}') {
            at++;
            return object;
        }
        while (true) {
            space();
            if (at >= text.length() || text.charAt(at) != '"') throw error("expected a key");
            String key = string();
            space();
            if (at >= text.length() || text.charAt(at) != ':') throw error("expected ':'");
            at++;
            space();
            object.put(key, value(depth + 1));
            space();
            if (at >= text.length()) throw error("end of text");
            char c = text.charAt(at++);
            if (c == '}') return object;
            if (c != ',') throw error("expected ',' or '}'");
        }
    }

    private List<Object> array(int depth) {
        List<Object> array = new ArrayList<>();
        at++;
        space();
        if (at < text.length() && text.charAt(at) == ']') {
            at++;
            return array;
        }
        while (true) {
            space();
            array.add(value(depth + 1));
            space();
            if (at >= text.length()) throw error("end of text");
            char c = text.charAt(at++);
            if (c == ']') return array;
            if (c != ',') throw error("expected ',' or ']'");
        }
    }

    private String string() {
        StringBuilder out = new StringBuilder();
        at++;
        while (true) {
            if (at >= text.length()) throw error("unterminated string");
            char c = text.charAt(at++);
            if (c == '"') return out.toString();
            if (c != '\\') {
                out.append(c);
                continue;
            }
            if (at >= text.length()) throw error("unterminated escape");
            char e = text.charAt(at++);
            switch (e) {
                case '"': out.append('"'); break;
                case '\\': out.append('\\'); break;
                case '/': out.append('/'); break;
                case 'b': out.append('\b'); break;
                case 'f': out.append('\f'); break;
                case 'n': out.append('\n'); break;
                case 'r': out.append('\r'); break;
                case 't': out.append('\t'); break;
                case 'u':
                    if (at + 4 > text.length()) throw error("short \\u escape");
                    try {
                        out.append((char) Integer.parseInt(text.substring(at, at + 4), 16));
                    } catch (NumberFormatException bad) {
                        throw error("bad \\u escape");
                    }
                    at += 4;
                    break;
                default: throw error("unknown escape");
            }
        }
    }

    private Object number() {
        int start = at;
        if (text.charAt(at) == '-') at++;
        while (at < text.length() && "0123456789.eE+-".indexOf(text.charAt(at)) >= 0) at++;
        String number = text.substring(start, at);
        try {
            if (number.indexOf('.') < 0 && number.indexOf('e') < 0 && number.indexOf('E') < 0) return Long.parseLong(number);
            return Double.parseDouble(number);
        } catch (NumberFormatException bad) {
            throw error("bad number");
        }
    }

    /** One line of JSON for a reply: maps, lists, strings, numbers, booleans and null. */
    static String write(Object value) {
        StringBuilder out = new StringBuilder();
        write(value, out);
        return out.toString();
    }

    private static void write(Object value, StringBuilder out) {
        if (value == null) {
            out.append("null");
        } else if (value instanceof String) {
            quote((String) value, out);
        } else if (value instanceof Boolean || value instanceof Integer || value instanceof Long) {
            out.append(value);
        } else if (value instanceof Number) {
            double number = ((Number) value).doubleValue();
            out.append(Double.isFinite(number) ? String.valueOf(number) : "null");
        } else if (value instanceof Map) {
            out.append('{');
            boolean first = true;
            for (Map.Entry<?, ?> entry : ((Map<?, ?>) value).entrySet()) {
                if (!first) out.append(',');
                first = false;
                quote(String.valueOf(entry.getKey()), out);
                out.append(':');
                write(entry.getValue(), out);
            }
            out.append('}');
        } else if (value instanceof Iterable) {
            out.append('[');
            boolean first = true;
            for (Object item : (Iterable<?>) value) {
                if (!first) out.append(',');
                first = false;
                write(item, out);
            }
            out.append(']');
        } else {
            quote(String.valueOf(value), out);
        }
    }

    private static void quote(String text, StringBuilder out) {
        out.append('"');
        for (int i = 0; i < text.length(); i++) {
            char c = text.charAt(i);
            switch (c) {
                case '"': out.append("\\\""); break;
                case '\\': out.append("\\\\"); break;
                case '\n': out.append("\\n"); break;
                case '\r': out.append("\\r"); break;
                case '\t': out.append("\\t"); break;
                default:
                    if (c < 0x20 || c == 0x2028 || c == 0x2029) out.append(String.format("\\u%04x", (int) c));
                    else out.append(c);
            }
        }
        out.append('"');
    }
}
