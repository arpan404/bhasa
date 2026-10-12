#ifndef BHASA_RUNTIME_TEXT_H
#define BHASA_RUNTIME_TEXT_H
#include "core.h"
#ifdef __cplusplus
extern "C" {
#endif
/* Strict Unicode scalar UTF-8: rejects overlong sequences, surrogates,
 * values above U+10FFFF, stray continuation bytes and truncated sequences. */
int64_t bhasa_rt_utf8_valid(const bhasa_buffer *value);
/* Invalid UTF-8 or invalid descriptors return -1. */
int64_t bhasa_rt_utf8_count(const bhasa_buffer *value);
/* Index is a zero-based scalar index, not a grapheme index; -1 on failure. */
int64_t bhasa_rt_utf8_get(const bhasa_buffer *value, int64_t index);
/* Owned byte copy. Invalid UTF-8 or an invalid scalar range is a runtime error. */
bhasa_buffer *bhasa_rt_utf8_slice(const bhasa_buffer *value, int64_t start, int64_t count);
/* Append one valid scalar; EINVAL leaves destination unchanged. Existing bytes
 * are not rescanned or repaired. Allocation failure follows core runtime rules. */
int64_t bhasa_rt_utf8_append(bhasa_buffer *value, int64_t scalar);
#ifdef __cplusplus
}
#endif
#endif
