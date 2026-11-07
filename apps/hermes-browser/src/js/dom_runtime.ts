/*
 * dom_runtime.ts — the AOT-compiled half of the browser.
 *
 * Compiled by `shermes -typed -exported-unit=dom_runtime -emit-c`. Nothing here
 * is interpreted at runtime: the DOM bindings and event bubbling loop below are
 * native C code calling directly into Rust (`$SHBuiltin.extern_c`) with zero JSI
 * boxing overhead.
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

  /* ---- The document, as exported by the native host (blitz_bridge.h) ---- */
  const _find = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_find_element_by_id(
    id: c_ptr
  ): c_size_t {
    throw 0;
  });
  const _query_selector = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_query_selector(
    selector: c_ptr
  ): c_size_t {
    throw 0;
  });
  const _query_selector_all = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_query_selector_all(
    selector: c_ptr
  ): c_size_t {
    throw 0;
  });
  const _query_selector_item = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_query_selector_item(
    index: c_size_t
  ): c_size_t {
    throw 0;
  });
  const _create_element = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_create_element(
    tag: c_ptr
  ): c_size_t {
    throw 0;
  });
  const _append_child = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_append_child(
    parent: c_size_t,
    child: c_size_t
  ): void {});
  const _remove_node = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_remove_node(
    node: c_size_t
  ): void {});
  const _parent_node = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_parent_node(
    node: c_size_t
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
  const _load_text = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_load_inner_text(
    node: c_size_t
  ): c_int {
    throw 0;
  });
  const _set_html = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_set_inner_html(
    node: c_size_t,
    html: c_ptr
  ): void {});
  const _load_html = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_load_inner_html(
    node: c_size_t
  ): c_int {
    throw 0;
  });
  const _set_attr = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_set_attribute(
    node: c_size_t,
    name: c_ptr,
    value: c_ptr
  ): void {});
  const _load_attr = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_load_attribute(
    node: c_size_t,
    name: c_ptr
  ): c_int {
    throw 0;
  });
  const _scratch_byte = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_scratch_byte(
    index: c_size_t
  ): c_int {
    throw 0;
  });
  const _get_rect_field = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_get_bounding_rect_field(
    node: c_size_t,
    field: c_int
  ): c_double {
    throw 0;
  });
  const _webauthn_create = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_webauthn_create(
    rp_id: c_ptr,
    user_name: c_ptr
  ): c_int {
    throw 0;
  });
  const _webauthn_get = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_webauthn_get(
    rp_id: c_ptr
  ): c_int {
    throw 0;
  });
  const _mark_dirty = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_mark_dirty(): void {});
  const _write_scratch_bytes = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_write_scratch_bytes(
    bytes: c_ptr,
    len: c_size_t
  ): c_int {
    throw 0;
  });
  const _scratch_bytes_len = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_scratch_bytes_len(): c_int {
    throw 0;
  });
  const _scratch_bytes_get = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_scratch_bytes_get(
    index: c_size_t
  ): c_int {
    throw 0;
  });
  const _set_allow_credentials = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_set_allow_credentials(
    json: c_ptr
  ): void {});

  /* Allocate a NUL-terminated UTF-8 copy of `s`. Caller frees.
   * Supports full Unicode (1-, 2-, 3-, and 4-byte UTF-8 sequences including
   * UTF-16 surrogate pairs). */
  function toUtf8z(s: string): c_ptr {
    let n: number = s.length;
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

  /* Decode `len` UTF-8 bytes from the Rust host's scratch buffer into a JS string. */
  function readScratchUtf8(len: number): string {
    let out: string = "";
    let i: number = 0;
    while (i < len) {
      let b0: number = _scratch_byte(i) & 0xff;
      i = i + 1;
      if (b0 < 0x80) {
        out = out + String.fromCharCode(b0);
      } else if ((b0 & 0xe0) === 0xc0 && i < len) {
        let b1: number = _scratch_byte(i) & 0x3f;
        i = i + 1;
        out = out + String.fromCharCode(((b0 & 0x1f) << 6) | b1);
      } else if ((b0 & 0xf0) === 0xe0 && i + 1 < len) {
        let b1: number = _scratch_byte(i) & 0x3f;
        let b2: number = _scratch_byte(i + 1) & 0x3f;
        i = i + 2;
        out = out + String.fromCharCode(((b0 & 0x0f) << 12) | (b1 << 6) | b2);
      } else if ((b0 & 0xf8) === 0xf0 && i + 2 < len) {
        let b1: number = _scratch_byte(i) & 0x3f;
        let b2: number = _scratch_byte(i + 1) & 0x3f;
        let b3: number = _scratch_byte(i + 2) & 0x3f;
        i = i + 3;
        let cp: number = (((b0 & 0x07) << 18) | (b1 << 12) | (b2 << 6) | b3) - 0x10000;
        out =
          out +
          String.fromCharCode(0xd800 + (cp >> 10)) +
          String.fromCharCode(0xdc00 + (cp & 0x3ff));
      }
    }
    return out;
  }

  /* Coerce a WebAuthn challenge into a Uint8Array.
   *
   * The WebAuthn spec makes `challenge` an `ArrayBuffer` (a `BufferSource`).
   * A relying party that follows the spec sends binary, so the bytes must
   * survive intact: a NUL byte or a byte above 0x7f must not be truncated or
   * re-encoded. A plain string is accepted as well, because some sites and
   * test harnesses pass one; it is encoded as UTF-8.
   *
   * Returns null when the value cannot be read as bytes at all. */
  function challengeToBytes(challenge: any): any {
    if (challenge === null || challenge === undefined) return null;
    if (typeof challenge === "string") {
      let enc: any = new TextEncoder();
      return new Uint8Array(enc.encode(challenge));
    }
    if (typeof challenge === "number") {
      return new Uint8Array([challenge & 0xff]);
    }
    if (challenge instanceof Uint8Array) return challenge;
    if (challenge instanceof ArrayBuffer) return new Uint8Array(challenge);
    /* A DataView, or any other view over a buffer. */
    if (challenge && typeof challenge.byteLength === "number" && challenge.buffer) {
      return new Uint8Array(challenge.buffer, challenge.byteOffset || 0, challenge.byteLength);
    }
    /* An array of byte values. */
    if (challenge && typeof challenge.length === "number" && typeof challenge !== "string") {
      let out: any = new Uint8Array(challenge.length);
      for (let i: number = 0; i < challenge.length; i = i + 1) {
        out[i] = challenge[i] & 0xff;
      }
      return out;
    }
    return null;
  }

  /* Hand `bytes` to the Rust host through the binary scratch buffer.
   *
   * The bytes are copied out of the JS heap into a C allocation and then into
   * the host's buffer, so the host sees exactly the bytes the page sent.
   *
   * Each byte is written through the char writer. `char` is signed on this
   * platform, but the host reads the buffer as `unsigned char`, so the bit
   * pattern of every byte is preserved exactly — a byte above 0x7f arrives
   * unchanged. */
  function writeChallengeBytes(bytes: any): number {
    let len: number = bytes.length;
    if (len === 0) {
      let empty: c_ptr = 0 as any as c_ptr;
      _write_scratch_bytes(empty, 0);
      return 0;
    }
    let buf: c_ptr = _malloc(len);
    for (let i: number = 0; i < len; i = i + 1) {
      _write_char(buf, i, bytes[i] & 0xff);
    }
    let written: number = _write_scratch_bytes(buf, len);
    _free(buf);
    return written;
  }

  /* Read `len` bytes back from the Rust host's binary scratch buffer. */
  function readScratchBytes(len: number): any {
    let out: any = new Uint8Array(len);
    for (let i: number = 0; i < len; i = i + 1) {
      out[i] = _scratch_bytes_get(i) & 0xff;
    }
    return out;
  }

  /* Encode bytes as base64url, the form a credential id travels in. */
  function bytesToB64url(bytes: any): string {
    let alphabet: string = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let out: string = "";
    let acc: number = 0;
    let bits: number = 0;
    for (let i: number = 0; i < bytes.length; i = i + 1) {
      acc = (acc << 8) | (bytes[i] & 0xff);
      bits = bits + 8;
      while (bits >= 6) {
        bits = bits - 6;
        out = out + alphabet.charAt((acc >> bits) & 63);
      }
    }
    if (bits > 0) {
      out = out + alphabet.charAt((acc << (6 - bits)) & 63);
    }
    return out;
  }

  /* Serialize the relying party's `allowCredentials` for the host.
   *
   * An empty list means the relying party named none, and the authenticator
   * discovers the credential itself. */
  function allowCredentialsToJson(allow: any): string {
    if (!allow) return "[]";
    let length: number = 0;
    try {
      length = allow.length;
    } catch (e) {
      return "[]";
    }
    if (typeof length !== "number" || length <= 0) return "[]";
    /* An array *literal* in this AOT Hermes is sealed: it has no `push` and
     * cannot take new index properties. The constructor form yields a real
     * mutable array. */
    let items: any = new (globalThis as any).Array();
    for (let i: number = 0; i < length; i = i + 1) {
      let entry: any = allow[i];
      if (!entry) continue;
      let rawId: any = entry.id;
      let id: string = "";
      /* Duck-typed rather than `instanceof`: a relying party may hand over an
       * ArrayBuffer, a typed array over one, or a base64url string. */
      if (typeof rawId === "string") {
        id = rawId;
      } else if (rawId && typeof rawId.byteLength === "number" && rawId.buffer) {
        /* A typed array view. */
        id = bytesToB64url(new Uint8Array(rawId.buffer, rawId.byteOffset || 0, rawId.byteLength));
      } else if (rawId && typeof rawId.byteLength === "number") {
        /* A bare ArrayBuffer. */
        id = bytesToB64url(new Uint8Array(rawId));
      }
      if (!id) continue;
      let item: any = new (globalThis as any).Object();
      item.id = id;
      item.type = entry.type || "public-key";
      item.transports = new (globalThis as any).Array();
      items.push(item);
    }
    return (globalThis as any).JSON.stringify(items);
  }

  class BlitzStyleDeclaration {
    id: c_size_t;

    constructor(id: c_size_t) {
      this.id = id;
    }

    setProperty(name: string, value: string): void {
      let n: c_ptr = toUtf8z(name);
      let v: c_ptr = toUtf8z(value);
      _set_style(this.id, n, v);
      _free(n);
      _free(v);
      _mark_dirty();
    }
  }

  /* Event listener registry keyed by eventType -> nodeId -> Array<Function>. */
  let eventListeners: any = {};
  let customClickHandlers: any = {};

  /* WebAuthn ceremonies that are waiting for a human. Keyed by the ceremony
   * id the host names, so the host can settle the right promise when the
   * human answers. */
  let pendingCeremonies: any = {};

  /* WebAuthn ceremonies that are waiting for the host to run the related
   * origins well-known fetch (WebAuthn Level 3 §5.11.1). Keyed by the request
   * id the host names, so the host can settle the right promise when the
   * fetch has answered. */
  let pendingRelatedOrigins: any = {};

  /* A handle to a live DOM node. The id is a versioned slot index, so a stale
   * handle fails to resolve in Rust instead of aliasing a different node. */
  class BlitzElement {
    id: c_size_t;
    style: BlitzStyleDeclaration;

    constructor(id: c_size_t) {
      this.id = id;
      this.style = new BlitzStyleDeclaration(id);
    }

    setStyleProperty(name: string, value: string): void {
      this.style.setProperty(name, value);
    }

    setInnerText(value: string): void {
      let t: c_ptr = toUtf8z(String(value));
      _set_text(this.id, t);
      _free(t);
      _mark_dirty();
    }

    getInnerText(): string {
      let len: number = _load_text(this.id);
      if (len < 0) return "";
      return readScratchUtf8(len);
    }

    setInnerHTML(value: string): void {
      let h: c_ptr = toUtf8z(String(value));
      _set_html(this.id, h);
      _free(h);
      _mark_dirty();
    }

    getInnerHTML(): string {
      let len: number = _load_html(this.id);
      if (len < 0) return "";
      return readScratchUtf8(len);
    }

    setAttribute(name: string, value: string): void {
      let n: c_ptr = toUtf8z(String(name));
      let v: c_ptr = toUtf8z(String(value));
      _set_attr(this.id, n, v);
      _free(n);
      _free(v);
      _mark_dirty();
    }

    getAttribute(name: string): any {
      let n: c_ptr = toUtf8z(String(name));
      let len: number = _load_attr(this.id, n);
      _free(n);
      if (len < 0) return null;
      return readScratchUtf8(len);
    }

    appendChild(child: BlitzElement): BlitzElement {
      if (child && (child.id as any) !== 0) {
        _append_child(this.id, child.id);
        _mark_dirty();
      }
      return child;
    }

    removeNode(): void {
      _remove_node(this.id);
      _mark_dirty();
    }

    remove(): void {
      this.removeNode();
    }

    getParentNode(): any {
      let pid: c_size_t = _parent_node(this.id);
      if ((pid as any) === 0) return null;
      return new BlitzElement(pid);
    }

    getStyle(): BlitzStyleDeclaration {
      return this.style;
    }

    addEventListener(type: string, listener: any): void {
      if (!listener) return;
      let t: string = String(type);
      let byNode: any = eventListeners[t];
      if (!byNode) {
        byNode = {};
        eventListeners[t] = byNode;
      }
      let key: any = this.id as any;
      let list: any = byNode[key];
      if (!list) {
        list = [];
        byNode[key] = list;
      }
      list.push(listener);
    }

    getBoundingClientRect(): any {
      let x: number = _get_rect_field(this.id, 0);
      let y: number = _get_rect_field(this.id, 1);
      let w: number = _get_rect_field(this.id, 2);
      let h: number = _get_rect_field(this.id, 3);
      return {
        x: x,
        y: y,
        width: w,
        height: h,
        top: y,
        left: x,
        right: x + w,
        bottom: y + h,
      };
    }
  }

  let ObjectCtor: any = (globalThis as any).Object;
  let ArrayCtor: any = (globalThis as any).Array;
  let defineProp: any = ObjectCtor ? ObjectCtor.defineProperty : null;

  function wrapElement(id: c_size_t): any {
    if ((id as any) === 0) return null;
    let el: any = new ObjectCtor();
    el.id = id;
    let styleObj: any = new ObjectCtor();
    styleObj.setProperty = function (name: string, value: string): void {
      let n: c_ptr = toUtf8z(String(name));
      let v: c_ptr = toUtf8z(String(value));
      _set_style(id, n, v);
      _free(n);
      _free(v);
      _mark_dirty();
    };
    el.style = styleObj;
    el.setStyleProperty = function (name: string, value: string): void {
      let n: c_ptr = toUtf8z(String(name));
      let v: c_ptr = toUtf8z(String(value));
      _set_style(id, n, v);
      _free(n);
      _free(v);
      _mark_dirty();
    };
    el.setInnerText = function (value: string): void {
      let t: c_ptr = toUtf8z(String(value));
      _set_text(id, t);
      _free(t);
      _mark_dirty();
    };
    el.getInnerText = function (): string {
      let len: number = _load_text(id);
      if (len < 0) return "";
      return readScratchUtf8(len);
    };
    el.setInnerHTML = function (value: string): void {
      let h: c_ptr = toUtf8z(String(value));
      _set_html(id, h);
      _free(h);
      _mark_dirty();
    };
    el.getInnerHTML = function (): string {
      let len: number = _load_html(id);
      if (len < 0) return "";
      return readScratchUtf8(len);
    };
    el.setAttribute = function (name: string, value: string): void {
      let n: c_ptr = toUtf8z(String(name));
      let v: c_ptr = toUtf8z(String(value));
      _set_attr(id, n, v);
      _free(n);
      _free(v);
      _mark_dirty();
    };
    el.getAttribute = function (name: string): any {
      let n: c_ptr = toUtf8z(String(name));
      let len: number = _load_attr(id, n);
      _free(n);
      if (len < 0) return null;
      return readScratchUtf8(len);
    };
    el.appendChild = function (child: any): any {
      if (child && child.id) {
        _append_child(id, child.id);
        _mark_dirty();
      }
      return child;
    };
    el.removeNode = function (): void {
      _remove_node(id);
      _mark_dirty();
    };
    el.remove = function (): void {
      _remove_node(id);
      _mark_dirty();
    };
    el.addEventListener = function (type: string, listener: any): void {
      if (!listener) return;
      let t: string = String(type);
      let byNode: any = eventListeners[t];
      if (!byNode) {
        byNode = new ObjectCtor();
        eventListeners[t] = byNode;
      }
      let key: any = id as any;
      let list: any = byNode[key];
      if (!list) {
        list = new ArrayCtor();
        byNode[key] = list;
      }
      list[list.length] = listener;
    };
    el.getBoundingClientRect = function (): any {
      let x: number = _get_rect_field(id, 0);
      let y: number = _get_rect_field(id, 1);
      let w: number = _get_rect_field(id, 2);
      let h: number = _get_rect_field(id, 3);
      let rectObj: any = new ObjectCtor();
      rectObj.x = x;
      rectObj.y = y;
      rectObj.width = w;
      rectObj.height = h;
      rectObj.top = y;
      rectObj.left = x;
      rectObj.right = x + w;
      rectObj.bottom = y + h;
      return rectObj;
    };
    if (defineProp) {
      let textDesc: any = new ObjectCtor();
      textDesc.get = function (): string {
        return el.getInnerText();
      };
      textDesc.set = function (v: any): void {
        el.setInnerText(String(v));
      };
      textDesc.configurable = true;
      defineProp(el, "innerText", textDesc);
      defineProp(el, "textContent", textDesc);

      let htmlDesc: any = new ObjectCtor();
      htmlDesc.get = function (): string {
        return el.getInnerHTML();
      };
      htmlDesc.set = function (v: any): void {
        el.setInnerHTML(String(v));
      };
      htmlDesc.configurable = true;
      defineProp(el, "innerHTML", htmlDesc);

      let parentDesc: any = new ObjectCtor();
      parentDesc.get = function (): any {
        let pid: c_size_t = _parent_node(id);
        if ((pid as any) === 0) return null;
        return wrapElement(pid);
      };
      parentDesc.configurable = true;
      defineProp(el, "parentNode", parentDesc);
      defineProp(el, "parentElement", parentDesc);
    }
    return el;
  }

  function getElementById(id: string): c_size_t {
    let c: c_ptr = toUtf8z(String(id));
    let node: c_size_t = _find(c);
    _free(c);
    return node;
  }

  function querySelector(selector: string): c_size_t {
    let c: c_ptr = toUtf8z(String(selector));
    let node: c_size_t = _query_selector(c);
    _free(c);
    return node;
  }

  let buttonId: c_size_t = 0;
  let labelId: c_size_t = 0;
  let clicks: number = 0;

  /* Expose the W3C DOM surface on globalThis.document so page scripts and
   * evaluated JS can query, construct, and mutate Blitz DOM nodes via $SHBuiltin.extern_c. */
  let docObj: any = new ObjectCtor();
  docObj.getElementById = function (id: string): any {
    let nid: c_size_t = getElementById(id);
    if ((nid as any) === 0) return null;
    return wrapElement(nid);
  };
  docObj.querySelector = function (selector: string): any {
    let nid: c_size_t = querySelector(selector);
    if ((nid as any) === 0) return null;
    return wrapElement(nid);
  };
  docObj.querySelectorAll = function (selector: string): any {
    let c: c_ptr = toUtf8z(String(selector));
    let count: number = _query_selector_all(c) as any;
    _free(c);
    let results: any = new ArrayCtor();
    let i: number = 0;
    while (i < count) {
      let nid: c_size_t = _query_selector_item(i as any);
      if ((nid as any) !== 0) {
        results[results.length] = wrapElement(nid);
      }
      i = i + 1;
    }
    return results;
  };
  docObj.createElement = function (tag: string): any {
    let c: c_ptr = toUtf8z(String(tag));
    let nid: c_size_t = _create_element(c);
    _free(c);
    if ((nid as any) === 0) return null;
    return wrapElement(nid);
  };
  if (defineProp) {
    let bodyDesc: any = new ObjectCtor();
    bodyDesc.get = function (): any {
      return wrapElement(querySelector("body"));
    };
    bodyDesc.configurable = true;
    defineProp(docObj, "body", bodyDesc);

    let docElDesc: any = new ObjectCtor();
    docElDesc.get = function (): any {
      return wrapElement(querySelector("html"));
    };
    docElDesc.configurable = true;
    defineProp(docObj, "documentElement", docElDesc);
  }
  (globalThis as any).document = docObj;

  (globalThis as any).window = globalThis;
  (globalThis as any).self = globalThis;

  /* Expose W3C WebAuthn navigator.credentials.create() and navigator.credentials.get()
   * backed by the native Rust P-256 authenticator via $SHBuiltin.extern_c.
   *
   * The ceremony itself is performed in Rust, in the host, by the passkey
   * broker, with the private key in the macOS Keychain and behind the human
   * approval gate. Nothing here signs anything: this is the API surface a
   * relying party talks to, the same way a browser exposes it. The spec
   * forbids simulating a passkey ceremony in page JS, and nothing in this
   * file does that. */
  let credsObj: any = new ObjectCtor();

  /* Decode a base64url string into an ArrayBuffer, the way a WebAuthn
   * response field is delivered to the page. */
  function b64urlToBuffer(value: string): any {
    let alphabet: string = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let clean: string = String(value).replace(/=+$/, "");
    let out: any = new Uint8Array(Math.floor((clean.length * 6) / 8));
    let acc: number = 0;
    let bits: number = 0;
    let o: number = 0;
    for (let i: number = 0; i < clean.length; i = i + 1) {
      let v: number = alphabet.indexOf(clean.charAt(i));
      if (v < 0) continue;
      acc = (acc << 6) | v;
      bits = bits + 6;
      if (bits >= 8) {
        bits = bits - 8;
        out[o] = (acc >> bits) & 0xff;
        o = o + 1;
      }
    }
    return out.buffer;
  }

  /* Build the response object a relying party expects:
   * an ArrayBuffer `id`, an ArrayBuffer `authenticatorData`, an ArrayBuffer
   * `clientDataJSON`, an ArrayBuffer `signature`, and a `getClientExtensionResults`
   * method. */
  function buildCredentialResponse(json: any, isAssertion: boolean): any {
    let resp: any = new ObjectCtor();
    let idBuf: any = b64urlToBuffer(json.credential_id || json.id || "");
    let clientDataBuf: any = b64urlToBuffer(json.client_data_json_b64 || "");
    let authDataBuf: any = b64urlToBuffer(json.authenticator_data_b64 || "");
    let sigBuf: any = b64urlToBuffer(json.signature_der_b64 || "");
    resp.id = idBuf;
    resp.rawId = idBuf;
    resp.type = "public-key";
    resp.authenticatorAttachment = "platform";
    resp.response = new ObjectCtor();
    if (isAssertion) {
      resp.response.authenticatorData = authDataBuf;
      resp.response.clientDataJSON = clientDataBuf;
      resp.response.signature = sigBuf;
      resp.response.userHandle = null;
    } else {
      resp.response.attestationObject = authDataBuf;
      resp.response.clientDataJSON = clientDataBuf;
      resp.response.transports = ["internal"];
    }
    resp.getClientExtensionResults = function (): any {
      return new ObjectCtor();
    };
    /* `PublicKeyCredential.toJSON()` is the serialization a relying party may
     * read off the credential. */
    resp.toJSON = function (): any {
      let out: any = new ObjectCtor();
      out.id = json.credential_id || json.id || "";
      out.rawId = json.credential_id || json.id || "";
      out.type = "public-key";
      out.authenticatorAttachment = "platform";
      out.response = new ObjectCtor();
      if (isAssertion) {
        out.response.authenticatorData = json.authenticator_data_b64 || "";
        out.response.clientDataJSON = json.client_data_json_b64 || "";
        out.response.signature = json.signature_der_b64 || "";
        out.response.userHandle = null;
      } else {
        out.response.attestationObject = json.authenticator_data_b64 || "";
        out.response.clientDataJSON = json.client_data_json_b64 || "";
        out.response.transports = ["internal"];
      }
      out.clientExtensionResults = new ObjectCtor();
      return out;
    };
    return resp;
  }

  credsObj.create = function (options: any): any {
    let PromiseCtor: any = (globalThis as any).Promise;
    let pk: any = options ? options.publicKey || options : new ObjectCtor();
    let rpObj: any = pk.rp || new ObjectCtor();
    let userObj: any = pk.user || new ObjectCtor();
    let rpIdStr: string = String(rpObj.id || rpObj.name || "");
    let userNameStr: string = String(userObj.name || userObj.displayName || "");
    let chalBytes: any = challengeToBytes(pk.challenge);
    if (chalBytes === null) {
      return PromiseCtor.reject(new Error("WebAuthn create: challenge is missing"));
    }
    let cRp: c_ptr = toUtf8z(rpIdStr);
    let cUser: c_ptr = toUtf8z(userNameStr);
    writeChallengeBytes(chalBytes);
    let len: number = _webauthn_create(cRp, cUser);
    _free(cRp);
    _free(cUser);
    if (len < 0) {
      return PromiseCtor.reject(new Error("WebAuthn create failed"));
    }
    let jsonStr: string = readScratchUtf8(len);
    let parsed: any = (globalThis as any).JSON.parse(jsonStr);
    if (!parsed || parsed.error) {
      return PromiseCtor.reject(new Error((parsed && parsed.error) || "WebAuthn create failed"));
    }
    /* A ceremony that needs a human stays pending. The page's promise is
     * not settled now; the host settles it once the human has answered, and
     * until then the page cannot see a credential. */
    if (parsed.pending === true) {
      /* A ceremony that has to consult the relying party's related origins
       * well-known document first is waiting on the network, not on a human.
       * The host resolves it once the fetch has answered. */
      if (parsed.phase === "related-origins") {
        let requestId: string = String(parsed.request_id || "");
        if (!requestId) {
          return PromiseCtor.reject(
            new Error("WebAuthn create: the host did not name the related origins request")
          );
        }
        return new PromiseCtor(function (resolve: any, reject: any): void {
          pendingRelatedOrigins[requestId] = { resolve: resolve, reject: reject };
        });
      }
      let ceremonyId: string = String(parsed.ceremony_id || "");
      if (!ceremonyId) {
        return PromiseCtor.reject(new Error("WebAuthn create: the host did not name the ceremony"));
      }
      return new PromiseCtor(function (resolve: any, reject: any): void {
        pendingCeremonies[ceremonyId] = { resolve: resolve, reject: reject };
      });
    }
    return PromiseCtor.resolve(buildCredentialResponse(parsed, false));
  };
  credsObj.get = function (options: any): any {
    let PromiseCtor: any = (globalThis as any).Promise;
    let pk: any = options ? options.publicKey || options : new ObjectCtor();
    let rpIdStr: string = String(pk.rpId || (pk.rp && pk.rp.id) || "");
    let chalBytes: any = challengeToBytes(pk.challenge);
    if (chalBytes === null) {
      return PromiseCtor.reject(new Error("WebAuthn get: challenge is missing"));
    }
    let cRp: c_ptr = toUtf8z(rpIdStr);
    writeChallengeBytes(chalBytes);
    // The relying party may name the credentials it accepts. When it does,
    // the authenticator must use exactly those; when it does not, the
    // authenticator discovers the credential itself.
    let allowJson: string = allowCredentialsToJson(pk.allowCredentials);
    let cAllow: c_ptr = toUtf8z(allowJson);
    _set_allow_credentials(cAllow);
    _free(cAllow);
    let len: number = _webauthn_get(cRp);
    _free(cRp);
    if (len < 0) {
      return PromiseCtor.reject(new Error("WebAuthn get failed"));
    }
    let jsonStr: string = readScratchUtf8(len);
    let parsed: any = (globalThis as any).JSON.parse(jsonStr);
    if (!parsed || parsed.error) {
      return PromiseCtor.reject(new Error((parsed && parsed.error) || "WebAuthn get failed"));
    }
    /* A ceremony that needs a human stays pending. The page's promise is
     * not settled now; the host settles it once the human has answered, and
     * until then the page cannot see an assertion. */
    if (parsed.pending === true) {
      /* A ceremony that has to consult the relying party's related origins
       * well-known document first is waiting on the network, not on a human.
       * The host resolves it once the fetch has answered. */
      if (parsed.phase === "related-origins") {
        let requestId: string = String(parsed.request_id || "");
        if (!requestId) {
          return PromiseCtor.reject(
            new Error("WebAuthn get: the host did not name the related origins request")
          );
        }
        return new PromiseCtor(function (resolve: any, reject: any): void {
          pendingRelatedOrigins[requestId] = { resolve: resolve, reject: reject };
        });
      }
      let ceremonyId: string = String(parsed.ceremony_id || "");
      if (!ceremonyId) {
        return PromiseCtor.reject(new Error("WebAuthn get: the host did not name the ceremony"));
      }
      return new PromiseCtor(function (resolve: any, reject: any): void {
        pendingCeremonies[ceremonyId] = { resolve: resolve, reject: reject };
      });
    }
    return PromiseCtor.resolve(buildCredentialResponse(parsed, true));
  };
  /* Called by the host when the human has answered a pending ceremony.
   * `ok` is true when the ceremony was approved and produced an assertion,
   * false when it was denied or expired. */
  (globalThis as any).__settleWebAuthn = function (ceremonyId: string, ok: boolean, payload: string): void {
    let entry: any = pendingCeremonies[ceremonyId];
    if (!entry) {
      return;
    }
    delete pendingCeremonies[ceremonyId];
    if (!ok) {
      let message: string = payload || "The passkey request was not approved";
      entry.reject(new Error(message));
      return;
    }
    let parsed: any = (globalThis as any).JSON.parse(payload);
    if (!parsed || parsed.error) {
      entry.reject(new Error((parsed && parsed.error) || "WebAuthn get failed"));
      return;
    }
    entry.resolve(buildCredentialResponse(parsed, true));
  };
  /* Called by the host when the related origins well-known fetch has answered
   * (WebAuthn Level 3 §5.11.1). `ok` is true when the relying party's own
   * document named the caller's origin, false when it did not. When it did,
   * the host has already started the ceremony, so the payload carries the
   * ceremony the page should now wait on. */
  (globalThis as any).__settleRelatedOrigins = function (
    requestId: string,
    ok: boolean,
    payload: string
  ): void {
    let entry: any = pendingRelatedOrigins[requestId];
    if (!entry) {
      return;
    }
    delete pendingRelatedOrigins[requestId];
    if (!ok) {
      let message: string = payload || "The relying party does not accept this origin";
      entry.reject(new Error(message));
      return;
    }
    let parsed: any = (globalThis as any).JSON.parse(payload);
    if (!parsed || parsed.error) {
      entry.reject(new Error((parsed && parsed.error) || "WebAuthn failed"));
      return;
    }
    /* The host has started the ceremony and named it. The page's promise now
     * waits for the human, exactly as it would have without the related
     * origins step. */
    let ceremonyId: string = String(parsed.ceremony_id || "");
    if (!ceremonyId) {
      entry.reject(new Error("WebAuthn: the host did not name the ceremony"));
      return;
    }
    entry.resolve(
      new (globalThis as any).Promise(function (innerResolve: any, innerReject: any): void {
        pendingCeremonies[ceremonyId] = { resolve: innerResolve, reject: innerReject };
      })
    );
  };
  /* `CredentialsContainer.preventSilentMediation()` is part of the spec. The
   * browser honours it by refusing to answer a conditional request without an
   * explicit user gesture. */
  credsObj.preventSilentMediation = function (): void {};
  let navObj: any = new ObjectCtor();
  navObj.credentials = credsObj;
  navObj.userAgent = "Walkaway-Hermes-Blitz/0.1";
  (globalThis as any).navigator = navObj;

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
    customClickHandlers = new ObjectCtor();
    eventListeners = new ObjectCtor();
  };

  /* Called by the host for every dispatched event. Walks up the DOM ancestor
   * chain via `blitz_parent_node` so listeners registered on target or ancestor
   * elements receive bubbled events. */
  (globalThis as any).__dispatchBlitzEvent = function (
    eventType: string,
    nodeId: number,
    x: number,
    y: number
  ): void {
    let targetEl: any = wrapElement(nodeId as any);
    let stopped: boolean = false;
    let ev: any = new ObjectCtor();
    ev.type = eventType;
    ev.target = targetEl;
    ev.currentTarget = targetEl;
    ev.clientX = x;
    ev.clientY = y;
    ev.x = x;
    ev.y = y;
    ev.stopPropagation = function (): void {
      stopped = true;
    };

    let byNode: any = eventListeners[eventType];
    let cur: number = nodeId;
    let handledByCustom: boolean = false;

    while (cur !== 0 && !stopped) {
      let curEl: any = wrapElement(cur as any);
      ev.currentTarget = curEl;

      if (byNode) {
        let listeners: any = byNode[cur];
        if (listeners) {
          let i: number = 0;
          let len: number = listeners.length;
          while (i < len) {
            let fn: any = listeners[i];
            if (fn) {
              fn(ev);
            }
            i = i + 1;
          }
        }
      }

      if (eventType === "click") {
        let customFn: any = customClickHandlers[cur];
        if (customFn) {
          handledByCustom = true;
          customFn(curEl, x, y);
          _mark_dirty();
        }
      }

      cur = _parent_node(cur as any) as any;
    }

    if (eventType !== "click" || handledByCustom || stopped) {
      return;
    }

    /* Check if `nodeId` or any of its ancestors is the start-page `#action-btn`. */
    if ((buttonId as any) === 0) return;
    let checkNode: number = nodeId;
    let hitActionBtn: boolean = false;
    while (checkNode !== 0) {
      if (checkNode === (buttonId as any)) {
        hitActionBtn = true;
        break;
      }
      checkNode = _parent_node(checkNode as any) as any;
    }
    if (!hitActionBtn) return;

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
