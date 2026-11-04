#pragma once

/*
 * C surface shared by the AOT-compiled TypeScript runtime (dom_runtime.ts) and
 * the native host.
 *
 * These are the symbols named in the design document (`blitz_set_style_property`,
 * `blitz_set_inner_text`, ...). The document implies the JS side holds the
 * `BlitzHostContext*` and passes it back in on every call. That does not work:
 * `c_ptr` values are transported through `SHLegacyValue`, which is NaN-boxed, so
 * a pointer arriving from JSI as a `double` would be reinterpreted from the
 * double's bit pattern and corrupted. The context pointer therefore stays on the
 * native side, bound once in hermes_shim.cpp, and these functions carry no
 * context argument. The Rust implementations are named `blitz_dom_*` and take
 * the pointer explicitly.
 */

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Resolve an element by its HTML `id`. Returns 0 when absent (NodeId 0 is the
 * null id and never resolves). */
uint64_t blitz_find_element_by_id(const char *id);

/* Inline-style mutation: `node.style.setProperty(name, value)`. */
void blitz_set_style_property(uint64_t node, const char *name, const char *value);

/* Text-content mutation: `node.innerText = value`. */
void blitz_set_inner_text(uint64_t node, const char *text);

/* Attribute mutation: `node.setAttribute(name, value)`. */
void blitz_set_attribute(uint64_t node, const char *name, const char *value);

/* Mark the document as damaged so the host re-resolves layout and repaints. */
void blitz_mark_dirty(void);

#ifdef __cplusplus
}
#endif
