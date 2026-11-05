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
  const _mark_dirty = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_mark_dirty(): void {});

  /* Allocate an ASCIIZ copy of `s`. Caller frees. */
  function toAsciiz(s: string): c_ptr {
    let n: number = s.length;
    let buf: c_ptr = _malloc(n + 1);
    let i: number = 0;
    for (; i < n; i++) {
      let code: number = s.charCodeAt(i);
      if (code > 127) throw Error("non-ASCII string on the FFI boundary");
      _write_char(buf, i, code);
    }
    _write_char(buf, i, 0);
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
      let n: c_ptr = toAsciiz(name);
      let v: c_ptr = toAsciiz(value);
      _set_style(this.id, n, v);
      _free(n);
      _free(v);
    }

    setInnerText(value: string): void {
      let t: c_ptr = toAsciiz(value);
      _set_text(this.id, t);
      _free(t);
    }
  }

  /* Resolve an element id to a node id. 0 means "not present": NodeId 0 is the
   * null id and never resolves, so absence needs no separate channel. */
  function getElementById(id: string): c_size_t {
    let c: c_ptr = toAsciiz(id);
    let node: c_size_t = _find(c);
    _free(c);
    return node;
  }

  /* ---- Application state ----
   * Node handles are kept as raw ids rather than nullable objects: typed mode
   * does not narrow a mutable module-level binding, and an id is also what the
   * native event dispatch hands back. */
  let buttonId: c_size_t = 0;
  let labelId: c_size_t = 0;
  let clicks: number = 0;

  /* Called once by the host, after the unit has been instantiated. */
  (globalThis as any).__initBlitzContext = function (): void {
    buttonId = getElementById("action-btn");
    labelId = getElementById("status-label");
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
