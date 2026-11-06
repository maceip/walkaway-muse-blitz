#pragma once

/*
 * C surface shared by the AOT-compiled TypeScript runtime (dom_runtime.ts) and
 * the native host (hermes_shim.cpp -> src/dom_ffi.rs).
 *
 * The active tab's BlitzHostContext* stays on the native side, bound via
 * ScopedCurrentApp in hermes_shim.cpp, so no 64-bit native pointer is ever
 * NaN-boxed into a JS double.
 */

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Queries */
uint64_t blitz_find_element_by_id(const char *id);
uint64_t blitz_query_selector(const char *selector);
size_t blitz_query_selector_all(const char *selector);
uint64_t blitz_query_selector_item(size_t index);

/* Tree construction & mutation */
uint64_t blitz_create_element(const char *tag);
void blitz_append_child(uint64_t parent, uint64_t child);
void blitz_remove_node(uint64_t node);
uint64_t blitz_parent_node(uint64_t node);

/* Style, text, HTML & attributes */
void blitz_set_style_property(uint64_t node, const char *name, const char *value);
void blitz_set_inner_text(uint64_t node, const char *text);
int blitz_load_inner_text(uint64_t node);
void blitz_set_inner_html(uint64_t node, const char *html);
int blitz_load_inner_html(uint64_t node);
void blitz_set_attribute(uint64_t node, const char *name, const char *value);
int blitz_load_attribute(uint64_t node, const char *name);
int blitz_scratch_byte(size_t index);

/* Taffy resolved layout geometry: field 0=x, 1=y, 2=width, 3=height */
double blitz_get_bounding_rect_field(uint64_t node, int field);

/* Mark the document as damaged so the host re-resolves layout and repaints. */
void blitz_mark_dirty(void);

#ifdef __cplusplus
}
#endif
