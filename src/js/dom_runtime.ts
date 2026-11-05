/*
 * dom_runtime.ts — the AOT-compiled half of the browser.
 *
 * Compiled by `shermes -typed -exported-unit=dom_runtime -emit-c`. Nothing here
 * is interpreted at runtime: the event handlers below are native code, and the
 * DOM calls are direct C calls (`$SHBuiltin.extern_c`) into the Rust document,
 * with no JSI boxing and no intermediate object graph.
 *
 * The document's version of this file assumes the JS side holds the
 * `BlitzHostContext*` and passes it into every FFI call. That pointer cannot
 * survive the trip through a NaN-boxed SHLegacyValue, so the context is bound
 * natively in hermes_shim.cpp and these declarations carry no context argument.
 */

"use strict";

(function () {
  /* ---- libc, for marshalling JS strings into NUL-terminated C strings ---- */
  const _malloc = $SHBuiltin.extern_c({include: "stdlib.h"}, function malloc(size: c_size_t): c_ptr {
    throw 0;
  });
  const _free = $SHBuiltin.extern_c({include: "stdlib.h"}, function free(p: c_ptr): void {});
  const _write_char = $SHBuiltin.extern_c({declared: true}, function _sh_ptr_write_char(
    ptr: c_ptr,
    offset: c_int,
    v: c_char
  ): void {});

  /* ---- The document, as exported by the native host ---- */
  const _find = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_find_element_by_id(
    id: c_ptr
  ): c_size_t {
    throw 0;
  });
  const _set_style = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_set_style_property(
    node: c_size_t,
    name: c_ptr,
    value: c_ptr
  ): void {});
  const _set_text = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_set_inner_text(
    node: c_size_t,
    text: c_ptr
  ): void {});
  const _set_attr = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_set_attribute(
    node: c_size_t,
    name: c_ptr,
    value: c_ptr
  ): void {});
  const _mark_dirty = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_mark_dirty(): void {});

  /* Allocate a NUL-terminated UTF-8 copy of `s`. Caller frees.
   * Supports full Unicode (1-, 2-, 3-, and 4-byte UTF-8 sequences including
   * UTF-16 surrogate pairs). */
  function toUtf8z(s: string): c_ptr {
    let n: number = s.length;
    /* Worst-case UTF-8 expansion is 3 bytes per UTF-16 code unit + 1 NUL byte. */
    let buf: c_ptr = _malloc(n * 3 + 1);
    let outIdx: number = 0;
    let i: number = 0;
    while (i < n) {
      let c: number = s.charCodeAt(i);
      i = i + 1;
      if (c < 0x80) {
        _write_char(buf, outIdx, c);
        outIdx = outIdx + 1;
      } else if (c < 0x800) {
        _write_char(buf, outIdx, 0xc0 | (c >> 6));
        _write_char(buf, outIdx + 1, 0x80 | (c & 0x3f));
        outIdx = outIdx + 2;
      } else if (c >= 0xd800 && c <= 0xdbff && i < n) {
        let low: number = s.charCodeAt(i);
        if (low >= 0xdc00 && low <= 0xdfff) {
          i = i + 1;
          let cp: number = ((c - 0xd800) << 10) + (low - 0xdc00) + 0x10000;
          _write_char(buf, outIdx, 0xf0 | (cp >> 18));
          _write_char(buf, outIdx + 1, 0x80 | ((cp >> 12) & 0x3f));
          _write_char(buf, outIdx + 2, 0x80 | ((cp >> 6) & 0x3f));
          _write_char(buf, outIdx + 3, 0x80 | (cp & 0x3f));
          outIdx = outIdx + 4;
        } else {
          _write_char(buf, outIdx, 0xe0 | (c >> 12));
          _write_char(buf, outIdx + 1, 0x80 | ((c >> 6) & 0x3f));
          _write_char(buf, outIdx + 2, 0x80 | (c & 0x3f));
          outIdx = outIdx + 3;
        }
      } else {
        _write_char(buf, outIdx, 0xe0 | (c >> 12));
        _write_char(buf, outIdx + 1, 0x80 | ((c >> 6) & 0x3f));
        _write_char(buf, outIdx + 2, 0x80 | (c & 0x3f));
        outIdx = outIdx + 3;
      }
    }
    _write_char(buf, outIdx, 0);
    return buf;
  }

  /* A handle to a live DOM node. The id is a versioned slot index, so a stale
   * handle fails to resolve in Rust instead of aliasing a different node. */
  class BlitzElement {
    id: c_size_t;

    constructor(id: c_size_t) {
      this.id = id;
    }

    setStyleProperty(name: string, value: string): void {
      let n: c_ptr = toUtf8z(name);
      let v: c_ptr = toUtf8z(value);
      _set_style(this.id, n, v);
      _free(n);
      _free(v);
      _mark_dirty();
    }

    setInnerText(value: string): void {
      let t: c_ptr = toUtf8z(value);
      _set_text(this.id, t);
      _free(t);
      _mark_dirty();
    }

    setAttribute(name: string, value: string): void {
      let n: c_ptr = toUtf8z(name);
      let v: c_ptr = toUtf8z(value);
      _set_attr(this.id, n, v);
      _free(n);
      _free(v);
      _mark_dirty();
    }
  }

  /* Resolve an element id to a node id. 0 means "not present": NodeId 0 is the
   * null id and never resolves, so absence needs no separate channel. */
  function getElementById(id: string): c_size_t {
    let c: c_ptr = toUtf8z(id);
    let node: c_size_t = _find(c);
    _free(c);
    return node;
  }

  /* ---- Application state & general DOM API ----
   * Node handles are kept as raw ids rather than nullable objects: typed mode
   * does not narrow a mutable module-level binding, and an id is also what the
   * native event dispatch hands back. */
  let buttonId: c_size_t = 0;
  let labelId: c_size_t = 0;
  let clicks: number = 0;
  let customClickHandlers: any = {};

  /* Expose a general DOM surface on globalThis.document so any page or
   * evaluated script can query and mutate Blitz DOM nodes via $SHBuiltin.extern_c. */
  (globalThis as any).document = {
    getElementById: function (id: string): any {
      let nid: c_size_t = getElementById(id);
      if ((nid as any) === 0) return null;
      return new BlitzElement(nid);
    },
  };

  (globalThis as any).__registerClickHandler = function (id: string, fn: any): void {
    let nid: c_size_t = getElementById(id);
    if ((nid as any) !== 0) {
      customClickHandlers[nid as any] = fn;
    }
  };

  /* Called once by the host, after the unit has been instantiated. */
  (globalThis as any).__initBlitzContext = function (): void {
    buttonId = getElementById("action-btn");
    labelId = getElementById("status-label");
    customClickHandlers = {};
  };

  /* Called by the host for every dispatched event. `nodeId` is the bubble
   * target chosen natively (the hit node or its nearest ancestor carrying a
   * listener). */
  (globalThis as any).__dispatchBlitzEvent = function (
    eventType: string,
    nodeId: number,
    x: number,
    y: number
  ): void {
    if (eventType !== "click") return;

    let customFn: any = customClickHandlers[nodeId];
    if (customFn) {
      customFn(new BlitzElement(nodeId as any), x, y);
      _mark_dirty();
      return;
    }

    if ((buttonId as any) === 0) return;
    if (nodeId !== (buttonId as any)) return;

    clicks = clicks + 1;

    let b: BlitzElement = new BlitzElement(buttonId);
    b.setStyleProperty("background-color", "#16A34A");
    b.setInnerText("Increment Counter (" + clicks.toString() + ")");

    if ((labelId as any) !== 0) {
      let l: BlitzElement = new BlitzElement(labelId);
      l.setInnerText(
        "Click handled natively in Hermes at (" + x.toString() + ", " + y.toString() + ")"
      );
    }

    _mark_dirty();
  };
})();
