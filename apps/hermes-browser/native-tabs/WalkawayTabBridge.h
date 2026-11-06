#ifndef WALKAWAY_TAB_BRIDGE_H
#define WALKAWAY_TAB_BRIDGE_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* All view/handle operations MUST run on the AppKit main thread. The caller
 * keeps the winit NSWindow/contentView alive until destroy returns. No callback
 * may reenter the browser or bridge: it only copies JSON and queues a host event.
 * Callback acceptance means queued, never a committed tab/window operation. */
typedef int32_t (*walkaway_tab_callback)(void *context, const uint8_t *json, size_t length);

int32_t walkaway_tabs_is_main_thread(void);
size_t walkaway_tabs_last_error(uint8_t *buffer, size_t capacity);
void *walkaway_tabs_create(void *winit_content_view, uint64_t window_token,
                           walkaway_tab_callback callback, void *context);
/* UTF-8 {"items":[{id,title,url,loading,secure,pinned,transferable,editable}],"selected":u64}.
 * IDs are full UInt64 JSON integers, mapped injectively to stable native UUIDs.
 * Model order should be pinned-first; target_index is the insertion index after
 * removing the moving tab (the original component clamps to its pinned group).
 * A non-transferable item also prevents transfers into its supervised window.
 * editable is required per item; false revokes the original tab field editor. */
int32_t walkaway_tabs_update(void *handle, const uint8_t *json, size_t length);
/* Logical content-view points; y is measured from the TOP, normally 74..112. */
int32_t walkaway_tabs_frame(void *handle, double x, double y, double width, double height);
/* -1=inherited appearance, 0=light, 1=dark. */
int32_t walkaway_tabs_appearance(void *handle, int32_t dark);
/* Yield to renderer-owned dropdown without destroying the native views. */
int32_t walkaway_tabs_overlays_visible(void *handle, int32_t visible);
/* Focus/select all in the selected ORIGINAL CompactTabCell.address. Enter and
 * Escape remain original cell delegate actions; search carries the edited cell's
 * stable tab_id, never an unrelated queued/global selection. No extra field. */
int32_t walkaway_tabs_focus_address(void *handle);
/* Synchronously cancels drag/hover/timers, unregisters, detaches hosting/tracking
 * views and disables the callback before the Rust callback context is released. */
void walkaway_tabs_destroy(void *handle);
/* Returns byte length, or zero on failure. Tightly packed sRGB RGBA8,
 * TOP-TO-BOTTOM and STRAIGHT ALPHA, captured from the actual native NSView.
 * Ownership transfers to the caller, which MUST call the matching free. */
size_t walkaway_tabs_capture_rgba(void *handle, uint8_t **bytes, uint32_t *width, uint32_t *height);
void walkaway_tabs_free_rgba(uint8_t *bytes);
/* x/y from detach events are AppKit global points, BOTTOM-LEFT origin.
 * Converts to a constrained native window position without a DPI mismatch. */
int32_t walkaway_tabs_position_window(void *content_view, double x, double y);
/* Real NSWindow component fixture suite, main-thread only. Returns assertion
 * count, or -1 with last_error. Never a substitute for browser E2E verification. */
int32_t walkaway_tabs_selftest(void);

#ifdef __cplusplus
}
#endif
#endif
