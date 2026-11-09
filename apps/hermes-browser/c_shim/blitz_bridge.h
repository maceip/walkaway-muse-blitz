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

/* WebAuthn navigator.credentials bridge (writes JSON credential into scratch buffer).
 * The challenge arrives through the binary scratch buffer, because a challenge
 * is arbitrary binary and cannot travel as a C string. */
int blitz_webauthn_create(const char *rp_id, const char *user_name);
int blitz_webauthn_get(const char *rp_id);

/* Binary scratch buffer: the page hands over raw bytes (a WebAuthn challenge
 * is arbitrary binary and cannot travel as a C string). */
int blitz_write_scratch_bytes(const uint8_t *bytes, size_t len);
int blitz_scratch_bytes_len(void);
int blitz_scratch_bytes_get(size_t index);

/* The credentials the relying party named in allowCredentials, as JSON. */
void blitz_set_allow_credentials(const char *json);

/* Request the host to submit a <form> element (or the form enclosing `node`). */
void blitz_submit_form(uint64_t node);

/* Mark the document as damaged so the host re-resolves layout and repaints. */
void blitz_mark_dirty(void);

/* RFC 6265 Cookie Jar (document.cookie) */
int blitz_get_cookie(void);
void blitz_set_cookie(const char *cookie);

/* Navigation & Location (window.location / history.pushState) */
int blitz_get_location_url(void);
void blitz_set_location_url(const char *url, int push_only);

/* Web Storage (is_session=0 -> localStorage, is_session=1 -> sessionStorage) */
int blitz_storage_get(int is_session, const char *key);
void blitz_storage_set(int is_session, const char *key, const char *value);
void blitz_storage_remove(int is_session, const char *key);
void blitz_storage_clear(int is_session);

/* Cookie-aware HTTP fetch (window.fetch) */
int blitz_http_fetch(
    const char *method,
    const char *url,
    const char *headers_json,
    const char *body_utf8);

/* Per-tab JavaScript Console (console.log / warn / error) */
void blitz_console_log(const char *level, const char *message);

#ifdef __cplusplus
}
#endif
