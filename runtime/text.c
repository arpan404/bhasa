#include "text.h"
#include <errno.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>

static int descriptor_valid(const bhasa_buffer *value) {
    return value != NULL && value->len >= 0 && value->cap >= value->len &&
        (uint64_t)value->cap <= SIZE_MAX && (value->cap == 0 || value->data != NULL);
}

static int next_scalar(const bhasa_buffer *value, size_t *offset, int64_t *scalar) {
    size_t at = *offset;
    if (at >= (size_t)value->len) return 0;
    unsigned char first = value->data[at];
    size_t width;
    uint32_t result;
    if (first < 0x80) { width = 1; result = first; }
    else if (first >= 0xc2 && first <= 0xdf) { width = 2; result = first & 0x1f; }
    else if (first >= 0xe0 && first <= 0xef) { width = 3; result = first & 0x0f; }
    else if (first >= 0xf0 && first <= 0xf4) { width = 4; result = first & 0x07; }
    else return 0;
    if (width > (size_t)value->len - at) return 0;
    for (size_t index = 1; index < width; ++index) {
        unsigned char byte = value->data[at + index];
        if ((byte & 0xc0) != 0x80) return 0;
        result = (result << 6) | (byte & 0x3f);
    }
    if ((width == 2 && result < 0x80) || (width == 3 && result < 0x800) ||
        (width == 4 && result < 0x10000) || result > 0x10ffff ||
        (result >= 0xd800 && result <= 0xdfff)) return 0;
    *offset = at + width;
    *scalar = result;
    return 1;
}

int64_t bhasa_rt_utf8_count(const bhasa_buffer *value) {
    if (!descriptor_valid(value)) return -1;
    size_t offset = 0;
    int64_t count = 0;
    while (offset < (size_t)value->len) {
        int64_t scalar;
        if (!next_scalar(value, &offset, &scalar)) return -1;
        ++count;
    }
    return count;
}

int64_t bhasa_rt_utf8_valid(const bhasa_buffer *value) {
    return bhasa_rt_utf8_count(value) >= 0;
}

int64_t bhasa_rt_utf8_get(const bhasa_buffer *value, int64_t index) {
    if (index < 0 || bhasa_rt_utf8_count(value) <= index) return -1;
    size_t offset = 0;
    int64_t scalar = -1;
    for (int64_t position = 0; position <= index; ++position)
        if (!next_scalar(value, &offset, &scalar)) return -1;
    return scalar;
}

static void text_error(const char *message) {
    fprintf(stderr, "Bhasa runtime error: %s\n", message);
    exit(1);
}

bhasa_buffer *bhasa_rt_utf8_slice(const bhasa_buffer *value, int64_t start, int64_t count) {
    int64_t length = bhasa_rt_utf8_count(value);
    if (length < 0) text_error("invalid UTF-8");
    if (start < 0 || count < 0 || start > length || count > length - start)
        text_error("codepoint slice range out of bounds");
    size_t offset = 0;
    int64_t scalar;
    for (int64_t index = 0; index < start; ++index) (void)next_scalar(value, &offset, &scalar);
    size_t begin = offset;
    for (int64_t index = 0; index < count; ++index) (void)next_scalar(value, &offset, &scalar);
    return bhasa_rt_buffer_slice(value, (int64_t)begin, (int64_t)(offset - begin));
}

int64_t bhasa_rt_utf8_append(bhasa_buffer *value, int64_t scalar) {
    if (!descriptor_valid(value) || scalar < 0 || scalar > 0x10ffff ||
        (scalar >= 0xd800 && scalar <= 0xdfff)) return EINVAL;
    unsigned char bytes[4];
    int64_t length;
    if (scalar < 0x80) { bytes[0] = (unsigned char)scalar; length = 1; }
    else if (scalar < 0x800) {
        bytes[0] = (unsigned char)(0xc0 | (scalar >> 6));
        bytes[1] = (unsigned char)(0x80 | (scalar & 0x3f)); length = 2;
    } else if (scalar < 0x10000) {
        bytes[0] = (unsigned char)(0xe0 | (scalar >> 12));
        bytes[1] = (unsigned char)(0x80 | ((scalar >> 6) & 0x3f));
        bytes[2] = (unsigned char)(0x80 | (scalar & 0x3f)); length = 3;
    } else {
        bytes[0] = (unsigned char)(0xf0 | (scalar >> 18));
        bytes[1] = (unsigned char)(0x80 | ((scalar >> 12) & 0x3f));
        bytes[2] = (unsigned char)(0x80 | ((scalar >> 6) & 0x3f));
        bytes[3] = (unsigned char)(0x80 | (scalar & 0x3f)); length = 4;
    }
    bhasa_buffer source = {length, length, bytes};
    bhasa_rt_buffer_append(value, &source);
    return 0;
}
