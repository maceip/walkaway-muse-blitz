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
  const _submit_form = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_submit_form(
    node: c_size_t
  ): void {});
  const _get_cookie = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_get_cookie(): c_int {
    throw 0;
  });
  const _set_cookie = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_set_cookie(
    cookie: c_ptr
  ): void {});
  const _get_location_url = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_get_location_url(): c_int {
    throw 0;
  });
  const _set_location_url = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_set_location_url(
    url: c_ptr,
    push_only: c_int
  ): void {});
  const _storage_get = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_storage_get(
    is_session: c_int,
    key: c_ptr
  ): c_int {
    throw 0;
  });
  const _storage_set = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_storage_set(
    is_session: c_int,
    key: c_ptr,
    value: c_ptr
  ): void {});
  const _storage_remove = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_storage_remove(
    is_session: c_int,
    key: c_ptr
  ): void {});
  const _storage_clear = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_storage_clear(
    is_session: c_int
  ): void {});
  const _http_fetch = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_http_fetch(
    method: c_ptr,
    url: c_ptr,
    headers_json: c_ptr,
    body_utf8: c_ptr
  ): c_int {
    throw 0;
  });
  const _console_log = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_console_log(
    level: c_ptr,
    message: c_ptr
  ): void {});
  const _remove_attr = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_remove_attribute(
    node: c_size_t,
    name: c_ptr
  ): void {});
  const _has_attr = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_has_attribute(
    node: c_size_t,
    name: c_ptr
  ): c_int {
    throw 0;
  });
  const _load_tag_name = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_load_tag_name(
    node: c_size_t
  ): c_int {
    throw 0;
  });
  const _load_form_value = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_load_form_value(
    node: c_size_t
  ): c_int {
    throw 0;
  });
  const _set_form_value = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_set_form_value(
    node: c_size_t,
    value: c_ptr
  ): void {});
  const _get_checked = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_get_checked(
    node: c_size_t
  ): c_int {
    throw 0;
  });
  const _set_checked = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_set_checked(
    node: c_size_t,
    checked: c_int
  ): void {});
  const _focus_node = $SHBuiltin.extern_c({include: "blitz_bridge.h"}, function blitz_focus_node(
    node: c_size_t
  ): void {});
  const _query_selector_within = $SHBuiltin.extern_c(
    {include: "blitz_bridge.h"},
    function blitz_query_selector_within(root: c_size_t, selector: c_ptr): c_size_t {
      throw 0;
    }
  );
  const _query_selector_all_within = $SHBuiltin.extern_c(
    {include: "blitz_bridge.h"},
    function blitz_query_selector_all_within(root: c_size_t, selector: c_ptr): c_size_t {
      throw 0;
    }
  );
  const _load_form_entries_json = $SHBuiltin.extern_c(
    {include: "blitz_bridge.h"},
    function blitz_load_form_entries_json(form_node: c_size_t): c_int {
      throw 0;
    }
  );
  const _crypto_random_bytes = $SHBuiltin.extern_c(
    {include: "blitz_bridge.h"},
    function blitz_crypto_random_bytes(len: c_size_t): c_int {
      throw 0;
    }
  );
  const _set_event_default_prevented = $SHBuiltin.extern_c(
    {include: "blitz_bridge.h"},
    function blitz_set_event_default_prevented(prevented: c_int): void {}
  );

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
    el.__nodeId = id;
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
    el.removeAttribute = function (name: string): void {
      let n: c_ptr = toUtf8z(String(name));
      _remove_attr(id, n);
      _free(n);
      _mark_dirty();
    };
    el.hasAttribute = function (name: string): boolean {
      let n: c_ptr = toUtf8z(String(name));
      let present: number = _has_attr(id, n);
      _free(n);
      return present !== 0;
    };
    el.appendChild = function (child: any): any {
      let childNodeId: any = child ? child.__nodeId || child.id : 0;
      if (childNodeId) {
        _append_child(id, childNodeId);
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
    el.submit = function (): void {
      _submit_form(id);
      _mark_dirty();
    };
    el.focus = function (): void {
      _focus_node(id);
    };
    el.blur = function (): void {};
    el.click = function (): void {
      let disp: any = (globalThis as any).__dispatchBlitzEvent;
      if (disp) disp("click", id as any, 0, 0);
    };
    el.querySelector = function (selector: string): any {
      let c: c_ptr = toUtf8z(String(selector));
      let nid: c_size_t = _query_selector_within(id, c);
      _free(c);
      if ((nid as any) === 0) return null;
      return wrapElement(nid);
    };
    el.querySelectorAll = function (selector: string): any {
      let c: c_ptr = toUtf8z(String(selector));
      let count: number = _query_selector_all_within(id, c) as any;
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

    let classListObj: any = new ObjectCtor();
    function readClassTokens(): any {
      let raw: any = el.getAttribute("class");
      let tokens: any = new ArrayCtor();
      if (!raw) return tokens;
      let parts: any = (String(raw) as any).split(/\s+/);
      for (let i: number = 0; i < parts.length; i = i + 1) {
        let tok: string = String(parts[i]);
        if (tok.length > 0) tokens[tokens.length] = tok;
      }
      return tokens;
    }
    classListObj.contains = function (cls: any): boolean {
      let want: string = String(cls);
      let tokens: any = readClassTokens();
      for (let i: number = 0; i < tokens.length; i = i + 1) {
        if (tokens[i] === want) return true;
      }
      return false;
    };
    classListObj.add = function (...names: any[]): void {
      let tokens: any = readClassTokens();
      for (let nIdx: number = 0; nIdx < names.length; nIdx = nIdx + 1) {
        let want: string = String(names[nIdx]);
        if (!want) continue;
        let found: boolean = false;
        for (let i: number = 0; i < tokens.length; i = i + 1) {
          if (tokens[i] === want) {
            found = true;
            break;
          }
        }
        if (!found) tokens[tokens.length] = want;
      }
      el.setAttribute("class", tokens.join(" "));
    };
    classListObj.remove = function (...names: any[]): void {
      let tokens: any = readClassTokens();
      let next: any = new ArrayCtor();
      for (let i: number = 0; i < tokens.length; i = i + 1) {
        let cur: string = String(tokens[i]);
        let drop: boolean = false;
        for (let nIdx: number = 0; nIdx < names.length; nIdx = nIdx + 1) {
          if (cur === String(names[nIdx])) {
            drop = true;
            break;
          }
        }
        if (!drop) next[next.length] = cur;
      }
      el.setAttribute("class", next.join(" "));
    };
    classListObj.toggle = function (cls: any, force: any): boolean {
      let want: string = String(cls);
      let has: boolean = classListObj.contains(want);
      let shouldHave: boolean = force !== undefined ? Boolean(force) : !has;
      if (shouldHave) {
        classListObj.add(want);
      } else {
        classListObj.remove(want);
      }
      return shouldHave;
    };
    el.classList = classListObj;

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
    el.removeEventListener = function (type: string, listener: any): void {
      if (!listener) return;
      let t: string = String(type);
      let byNode: any = eventListeners[t];
      if (!byNode) return;
      let key: any = id as any;
      let list: any = byNode[key];
      if (!list) return;
      let next: any = new ArrayCtor();
      for (let i: number = 0; i < list.length; i = i + 1) {
        if (list[i] !== listener) {
          next[next.length] = list[i];
        }
      }
      byNode[key] = next;
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
      let idDesc: any = new ObjectCtor();
      idDesc.get = function (): string {
        let v: any = el.getAttribute("id");
        return v === null || v === undefined ? "" : String(v);
      };
      idDesc.set = function (v: any): void {
        el.setAttribute("id", String(v));
      };
      idDesc.configurable = true;
      defineProp(el, "id", idDesc);

      let tagDesc: any = new ObjectCtor();
      tagDesc.get = function (): string {
        let len: number = _load_tag_name(id);
        if (len < 0) return "";
        return readScratchUtf8(len);
      };
      tagDesc.configurable = true;
      defineProp(el, "tagName", tagDesc);
      defineProp(el, "nodeName", tagDesc);

      let classDesc: any = new ObjectCtor();
      classDesc.get = function (): string {
        let v: any = el.getAttribute("class");
        return v === null || v === undefined ? "" : String(v);
      };
      classDesc.set = function (v: any): void {
        el.setAttribute("class", String(v));
      };
      classDesc.configurable = true;
      defineProp(el, "className", classDesc);

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

      let valueDesc: any = new ObjectCtor();
      valueDesc.get = function (): string {
        let len: number = _load_form_value(id);
        if (len < 0) return "";
        return readScratchUtf8(len);
      };
      valueDesc.set = function (v: any): void {
        let s: string = v === null || v === undefined ? "" : String(v);
        let c: c_ptr = toUtf8z(s);
        _set_form_value(id, c);
        _free(c);
        _mark_dirty();
      };
      valueDesc.configurable = true;
      defineProp(el, "value", valueDesc);

      let checkedDesc: any = new ObjectCtor();
      checkedDesc.get = function (): boolean {
        return _get_checked(id) !== 0;
      };
      checkedDesc.set = function (v: any): void {
        _set_checked(id, v ? 1 : 0);
        _mark_dirty();
      };
      checkedDesc.configurable = true;
      defineProp(el, "checked", checkedDesc);

      let disabledDesc: any = new ObjectCtor();
      disabledDesc.get = function (): boolean {
        return el.hasAttribute("disabled");
      };
      disabledDesc.set = function (v: any): void {
        if (v) {
          el.setAttribute("disabled", "disabled");
        } else {
          el.removeAttribute("disabled");
        }
      };
      disabledDesc.configurable = true;
      defineProp(el, "disabled", disabledDesc);

      let attrProps: any = ["name", "type", "placeholder", "href", "action", "method"];
      for (let pIdx: number = 0; pIdx < attrProps.length; pIdx = pIdx + 1) {
        let propName: string = String(attrProps[pIdx]);
        let d: any = new ObjectCtor();
        d.get = (function (pn: string): any {
          return function (): string {
            let v: any = el.getAttribute(pn);
            return v === null || v === undefined ? "" : String(v);
          };
        })(propName);
        d.set = (function (pn: string): any {
          return function (v: any): void {
            el.setAttribute(pn, String(v));
          };
        })(propName);
        d.configurable = true;
        defineProp(el, propName, d);
      }
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

    let cookieDesc: any = new ObjectCtor();
    cookieDesc.get = function (): string {
      let len: number = _get_cookie();
      if (len <= 0) return "";
      return readScratchUtf8(len);
    };
    cookieDesc.set = function (v: any): void {
      let c: c_ptr = toUtf8z(String(v === null || v === undefined ? "" : v));
      _set_cookie(c);
      _free(c);
    };
    cookieDesc.configurable = true;
    defineProp(docObj, "cookie", cookieDesc);

    let titleDesc: any = new ObjectCtor();
    titleDesc.get = function (): string {
      let tid: c_size_t = querySelector("title");
      if ((tid as any) === 0) return "";
      let tEl: any = wrapElement(tid);
      return tEl ? tEl.getInnerText() : "";
    };
    titleDesc.set = function (v: any): void {
      let tid: c_size_t = querySelector("title");
      if ((tid as any) !== 0) {
        let tEl: any = wrapElement(tid);
        if (tEl) tEl.setInnerText(String(v));
      }
    };
    titleDesc.configurable = true;
    defineProp(docObj, "title", titleDesc);

    let formsDesc: any = new ObjectCtor();
    formsDesc.get = function (): any {
      return docObj.querySelectorAll("form");
    };
    formsDesc.configurable = true;
    defineProp(docObj, "forms", formsDesc);
  }
  docObj.readyState = "complete";
  docObj.addEventListener = function (type: string, listener: any): void {
    let rootId: c_size_t = querySelector("html");
    if ((rootId as any) !== 0) {
      let rootEl: any = wrapElement(rootId);
      if (rootEl) rootEl.addEventListener(type, listener);
    }
  };
  docObj.removeEventListener = function (type: string, listener: any): void {
    let rootId: c_size_t = querySelector("html");
    if ((rootId as any) !== 0) {
      let rootEl: any = wrapElement(rootId);
      if (rootEl) rootEl.removeEventListener(type, listener);
    }
  };
  (globalThis as any).document = docObj;
  (globalThis as any).addEventListener = function (type: string, listener: any): void {
    docObj.addEventListener(type, listener);
  };
  (globalThis as any).removeEventListener = function (type: string, listener: any): void {
    docObj.removeEventListener(type, listener);
  };

  /* ---- window.location & window.history ---- */
  function currentHref(): string {
    let len: number = _get_location_url();
    if (len <= 0) return "muse://start";
    return readScratchUtf8(len);
  }

  function navigateTo(url: any, pushOnly: number): void {
    let c: c_ptr = toUtf8z(String(url === null || url === undefined ? "" : url));
    _set_location_url(c, pushOnly);
    _free(c);
  }

  let locObj: any = new ObjectCtor();
  locObj.assign = function (url: any): void {
    navigateTo(url, 0);
  };
  locObj.replace = function (url: any): void {
    navigateTo(url, 0);
  };
  locObj.reload = function (): void {
    navigateTo(currentHref(), 0);
  };
  locObj.toString = function (): string {
    return currentHref();
  };
  if (defineProp) {
    let hrefDesc: any = new ObjectCtor();
    hrefDesc.get = function (): string {
      return currentHref();
    };
    hrefDesc.set = function (v: any): void {
      navigateTo(v, 0);
    };
    hrefDesc.configurable = true;
    defineProp(locObj, "href", hrefDesc);

    let originDesc: any = new ObjectCtor();
    originDesc.get = function (): string {
      let h: string = currentHref();
      let schemeIdx: number = h.indexOf("://");
      if (schemeIdx < 0) return h;
      let pathIdx: number = h.indexOf("/", schemeIdx + 3);
      return pathIdx < 0 ? h : h.substring(0, pathIdx);
    };
    originDesc.configurable = true;
    defineProp(locObj, "origin", originDesc);

    let protocolDesc: any = new ObjectCtor();
    protocolDesc.get = function (): string {
      let h: string = currentHref();
      let idx: number = h.indexOf(":");
      return idx < 0 ? "https:" : h.substring(0, idx + 1);
    };
    protocolDesc.configurable = true;
    defineProp(locObj, "protocol", protocolDesc);

    let hostDesc: any = new ObjectCtor();
    hostDesc.get = function (): string {
      let h: string = currentHref();
      let schemeIdx: number = h.indexOf("://");
      if (schemeIdx < 0) return "";
      let rest: string = h.substring(schemeIdx + 3);
      let slashIdx: number = rest.indexOf("/");
      let qIdx: number = rest.indexOf("?");
      let end: number = slashIdx >= 0 ? slashIdx : qIdx >= 0 ? qIdx : rest.length;
      return rest.substring(0, end);
    };
    hostDesc.configurable = true;
    defineProp(locObj, "host", hostDesc);

    let hostnameDesc: any = new ObjectCtor();
    hostnameDesc.get = function (): string {
      let hostStr: string = locObj.host;
      let colonIdx: number = hostStr.indexOf(":");
      return colonIdx < 0 ? hostStr : hostStr.substring(0, colonIdx);
    };
    hostnameDesc.configurable = true;
    defineProp(locObj, "hostname", hostnameDesc);

    let pathnameDesc: any = new ObjectCtor();
    pathnameDesc.get = function (): string {
      let h: string = currentHref();
      let schemeIdx: number = h.indexOf("://");
      if (schemeIdx < 0) return "/";
      let slashIdx: number = h.indexOf("/", schemeIdx + 3);
      if (slashIdx < 0) return "/";
      let rest: string = h.substring(slashIdx);
      let qIdx: number = rest.indexOf("?");
      let hashIdx: number = rest.indexOf("#");
      let end: number =
        qIdx >= 0 && hashIdx >= 0
          ? Math.min(qIdx, hashIdx)
          : qIdx >= 0
          ? qIdx
          : hashIdx >= 0
          ? hashIdx
          : rest.length;
      return rest.substring(0, end);
    };
    pathnameDesc.set = function (v: any): void {
      navigateTo(v, 0);
    };
    pathnameDesc.configurable = true;
    defineProp(locObj, "pathname", pathnameDesc);

    let searchDesc: any = new ObjectCtor();
    searchDesc.get = function (): string {
      let h: string = currentHref();
      let qIdx: number = h.indexOf("?");
      if (qIdx < 0) return "";
      let hashIdx: number = h.indexOf("#", qIdx);
      return hashIdx < 0 ? h.substring(qIdx) : h.substring(qIdx, hashIdx);
    };
    searchDesc.configurable = true;
    defineProp(locObj, "search", searchDesc);

    let hashDesc: any = new ObjectCtor();
    hashDesc.get = function (): string {
      let h: string = currentHref();
      let hashIdx: number = h.indexOf("#");
      return hashIdx < 0 ? "" : h.substring(hashIdx);
    };
    hashDesc.configurable = true;
    defineProp(locObj, "hash", hashDesc);
  }
  (globalThis as any).location = locObj;
  docObj.location = locObj;

  let histObj: any = new ObjectCtor();
  histObj.pushState = function (_state: any, _title: any, url: any): void {
    if (url !== undefined && url !== null && String(url) !== "") {
      navigateTo(url, 1);
    }
  };
  histObj.replaceState = function (_state: any, _title: any, url: any): void {
    if (url !== undefined && url !== null && String(url) !== "") {
      navigateTo(url, 1);
    }
  };
  (globalThis as any).history = histObj;

  /* ---- localStorage & sessionStorage ---- */
  function makeStorage(isSession: number): any {
    let st: any = new ObjectCtor();
    st.getItem = function (key: any): any {
      let k: c_ptr = toUtf8z(String(key));
      let len: number = _storage_get(isSession, k);
      _free(k);
      if (len < 0) return null;
      return readScratchUtf8(len);
    };
    st.setItem = function (key: any, val: any): void {
      let k: c_ptr = toUtf8z(String(key));
      let v: c_ptr = toUtf8z(String(val));
      _storage_set(isSession, k, v);
      _free(k);
      _free(v);
    };
    st.removeItem = function (key: any): void {
      let k: c_ptr = toUtf8z(String(key));
      _storage_remove(isSession, k);
      _free(k);
    };
    st.clear = function (): void {
      _storage_clear(isSession);
    };
    return st;
  }
  (globalThis as any).localStorage = makeStorage(0);
  (globalThis as any).sessionStorage = makeStorage(1);

  /* ---- console.log / info / warn / error ---- */
  function emitConsole(level: string, args: any): void {
    let parts: string = "";
    let n: number = args ? args.length : 0;
    for (let i: number = 0; i < n; i = i + 1) {
      if (i > 0) parts = parts + " ";
      let a: any = args[i];
      if (typeof a === "string") {
        parts = parts + a;
      } else if (a && typeof a === "object") {
        try {
          parts = parts + (globalThis as any).JSON.stringify(a);
        } catch (_e) {
          parts = parts + String(a);
        }
      } else {
        parts = parts + String(a);
      }
    }
    let lPtr: c_ptr = toUtf8z(level);
    let mPtr: c_ptr = toUtf8z(parts);
    _console_log(lPtr, mPtr);
    _free(lPtr);
    _free(mPtr);
  }
  let consoleObj: any = new ObjectCtor();
  consoleObj.log = function (...args: any[]): void {
    emitConsole("log", args);
  };
  consoleObj.info = function (...args: any[]): void {
    emitConsole("info", args);
  };
  consoleObj.warn = function (...args: any[]): void {
    emitConsole("warn", args);
  };
  consoleObj.error = function (...args: any[]): void {
    emitConsole("error", args);
  };
  consoleObj.debug = function (...args: any[]): void {
    emitConsole("debug", args);
  };
  (globalThis as any).console = consoleObj;

  /* ---- window.fetch + Headers + Response ---- */
  function makeHeaders(raw: any): any {
    let map: any = new ObjectCtor();
    if (raw && typeof raw === "object") {
      let keys: any = ObjectCtor.keys(raw);
      for (let i: number = 0; i < keys.length; i = i + 1) {
        let k: string = String(keys[i]);
        map[k.toLowerCase()] = String(raw[k]);
      }
    }
    let h: any = new ObjectCtor();
    h._map = map;
    h.get = function (name: any): any {
      let k: string = String(name).toLowerCase();
      let v: any = map[k];
      return v === undefined ? null : v;
    };
    h.set = function (name: any, val: any): void {
      map[String(name).toLowerCase()] = String(val);
    };
    h.has = function (name: any): boolean {
      return map[String(name).toLowerCase()] !== undefined;
    };
    return h;
  }
  (globalThis as any).Headers = makeHeaders;

  (globalThis as any).fetch = function (input: any, init: any): any {
    let PromiseCtor: any = (globalThis as any).Promise;
    let urlStr: string =
      input && typeof input === "object" && input.url ? String(input.url) : String(input || "");
    let methodStr: string =
      init && init.method ? String(init.method).toUpperCase() : "GET";
    let hdrMap: any = new ObjectCtor();
    if (init && init.headers) {
      let src: any = init.headers._map ? init.headers._map : init.headers;
      if (src && typeof src === "object") {
        let keys: any = ObjectCtor.keys(src);
        for (let i: number = 0; i < keys.length; i = i + 1) {
          let k: string = String(keys[i]);
          hdrMap[k] = String(src[k]);
        }
      }
    }
    let bodyStr: string = "";
    if (init && init.body !== undefined && init.body !== null) {
      bodyStr = typeof init.body === "string" ? init.body : String(init.body);
      if (
        typeof init.body === "object" &&
        (init.body._isFormData || init.body._isUrlSearchParams) &&
        !hdrMap["content-type"] &&
        !hdrMap["Content-Type"]
      ) {
        hdrMap["content-type"] = "application/x-www-form-urlencoded;charset=UTF-8";
      }
    }
    let cMethod: c_ptr = toUtf8z(methodStr);
    let cUrl: c_ptr = toUtf8z(urlStr);
    let cHdrs: c_ptr = toUtf8z((globalThis as any).JSON.stringify(hdrMap));
    let cBody: c_ptr = toUtf8z(bodyStr);
    let len: number = _http_fetch(cMethod, cUrl, cHdrs, cBody);
    _free(cMethod);
    _free(cUrl);
    _free(cHdrs);
    _free(cBody);
    if (len < 0) {
      return PromiseCtor.reject(new Error("fetch failed"));
    }
    let rawJson: string = readScratchUtf8(len);
    let env: any = (globalThis as any).JSON.parse(rawJson);
    if (!env || env.error) {
      return PromiseCtor.reject(new Error((env && env.error) || "fetch failed"));
    }
    let respObj: any = new ObjectCtor();
    respObj.ok = Boolean(env.ok);
    respObj.status = Number(env.status || 200);
    respObj.statusText = respObj.ok ? "OK" : "Error";
    respObj.url = String(env.url || urlStr);
    respObj.headers = makeHeaders(env.headers || new ObjectCtor());
    let bodyText: string = String(env.body || "");
    respObj.text = function (): any {
      return PromiseCtor.resolve(bodyText);
    };
    respObj.json = function (): any {
      try {
        return PromiseCtor.resolve((globalThis as any).JSON.parse(bodyText));
      } catch (err) {
        return PromiseCtor.reject(err);
      }
    };
    return PromiseCtor.resolve(respObj);
  };

  /* ---- FormData, URLSearchParams, atob/btoa, crypto.getRandomValues, timers ---- */
  function makeFormData(formEl: any): any {
    let entriesList: any = new ArrayCtor();
    if (formEl && formEl.__nodeId) {
      let len: number = _load_form_entries_json(formEl.__nodeId);
      if (len > 0) {
        let parsed: any = (globalThis as any).JSON.parse(readScratchUtf8(len));
        if (parsed && parsed.length) {
          for (let i: number = 0; i < parsed.length; i = i + 1) {
            let row: any = parsed[i];
            if (row && row.length >= 2) {
              let pair: any = new ArrayCtor();
              pair[0] = String(row[0]);
              pair[1] = String(row[1]);
              entriesList[entriesList.length] = pair;
            }
          }
        }
      }
    }
    let fd: any = new ObjectCtor();
    fd._isFormData = true;
    fd._entries = entriesList;
    fd.append = function (k: any, v: any): void {
      let pair: any = new ArrayCtor();
      pair[0] = String(k);
      pair[1] = String(v === null || v === undefined ? "" : v);
      entriesList[entriesList.length] = pair;
    };
    fd.set = function (k: any, v: any): void {
      let keyStr: string = String(k);
      let valStr: string = String(v === null || v === undefined ? "" : v);
      let next: any = new ArrayCtor();
      let replaced: boolean = false;
      for (let i: number = 0; i < entriesList.length; i = i + 1) {
        let row: any = entriesList[i];
        if (row[0] === keyStr) {
          if (!replaced) {
            let pair: any = new ArrayCtor();
            pair[0] = keyStr;
            pair[1] = valStr;
            next[next.length] = pair;
            replaced = true;
          }
        } else {
          next[next.length] = row;
        }
      }
      if (!replaced) {
        let pair: any = new ArrayCtor();
        pair[0] = keyStr;
        pair[1] = valStr;
        next[next.length] = pair;
      }
      entriesList = next;
      fd._entries = entriesList;
    };
    fd.get = function (k: any): any {
      let keyStr: string = String(k);
      for (let i: number = 0; i < entriesList.length; i = i + 1) {
        if (entriesList[i][0] === keyStr) return entriesList[i][1];
      }
      return null;
    };
    fd.getAll = function (k: any): any {
      let keyStr: string = String(k);
      let out: any = new ArrayCtor();
      for (let i: number = 0; i < entriesList.length; i = i + 1) {
        if (entriesList[i][0] === keyStr) out[out.length] = entriesList[i][1];
      }
      return out;
    };
    fd.has = function (k: any): boolean {
      return fd.get(k) !== null;
    };
    fd.delete = function (k: any): void {
      let keyStr: string = String(k);
      let next: any = new ArrayCtor();
      for (let i: number = 0; i < entriesList.length; i = i + 1) {
        if (entriesList[i][0] !== keyStr) next[next.length] = entriesList[i];
      }
      entriesList = next;
      fd._entries = entriesList;
    };
    fd.entries = function (): any {
      return entriesList;
    };
    fd.forEach = function (cb: any): void {
      if (typeof cb !== "function") return;
      for (let i: number = 0; i < entriesList.length; i = i + 1) {
        cb(entriesList[i][1], entriesList[i][0], fd);
      }
    };
    fd.toString = function (): string {
      let out: string = "";
      for (let i: number = 0; i < entriesList.length; i = i + 1) {
        if (i > 0) out = out + "&";
        out =
          out +
          String((globalThis as any).encodeURIComponent(entriesList[i][0])) +
          "=" +
          String((globalThis as any).encodeURIComponent(entriesList[i][1]));
      }
      return out;
    };
    return fd;
  }
  (globalThis as any).FormData = makeFormData;

  function makeURLSearchParams(init: any): any {
    let pairs: any = new ObjectCtor();
    if (typeof init === "string") {
      let q: string = init;
      if (q.charAt(0) === "?") q = q.substring(1);
      if (q.length > 0) {
        let parts: any = (q as any).split("&");
        for (let i: number = 0; i < parts.length; i = i + 1) {
          let seg: string = String(parts[i]);
          let eq: number = seg.indexOf("=");
          if (eq >= 0) {
            let rawK: string = String((seg.substring(0, eq) as any).split("+").join(" "));
            let rawV: string = String((seg.substring(eq + 1) as any).split("+").join(" "));
            let k: string = String((globalThis as any).decodeURIComponent(rawK));
            let v: string = String((globalThis as any).decodeURIComponent(rawV));
            pairs[k] = v;
          } else if (seg.length > 0) {
            pairs[String((globalThis as any).decodeURIComponent(seg))] = "";
          }
        }
      }
    } else if (init && typeof init === "object") {
      if (init._entries && init._entries.length !== undefined) {
        for (let i: number = 0; i < init._entries.length; i = i + 1) {
          let row: any = init._entries[i];
          if (row && row.length >= 2) {
            pairs[String(row[0])] = String(row[1]);
          }
        }
      } else {
        let keys: any = ObjectCtor.keys(init);
        for (let i: number = 0; i < keys.length; i = i + 1) {
          let k: string = String(keys[i]);
          pairs[k] = String(init[k]);
        }
      }
    }
    let usp: any = new ObjectCtor();
    usp._isUrlSearchParams = true;
    usp.get = function (k: any): any {
      let v: any = pairs[String(k)];
      return v === undefined ? null : v;
    };
    usp.set = function (k: any, v: any): void {
      pairs[String(k)] = String(v);
    };
    usp.append = function (k: any, v: any): void {
      pairs[String(k)] = String(v);
    };
    usp.delete = function (k: any): void {
      delete pairs[String(k)];
    };
    usp.has = function (k: any): boolean {
      return pairs[String(k)] !== undefined;
    };
    usp.toString = function (): string {
      let keys: any = ObjectCtor.keys(pairs);
      let out: string = "";
      for (let i: number = 0; i < keys.length; i = i + 1) {
        if (i > 0) out = out + "&";
        let k: string = String(keys[i]);
        out = out
          + String((globalThis as any).encodeURIComponent(k))
          + "="
          + String((globalThis as any).encodeURIComponent(String(pairs[k])));
      }
      return out;
    };
    return usp;
  }
  (globalThis as any).URLSearchParams = makeURLSearchParams;

  let b64Std: string = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  (globalThis as any).btoa = function (input: any): string {
    let str: string = String(input === undefined ? "" : input);
    let out: string = "";
    let i: number = 0;
    while (i < str.length) {
      let c1: number = str.charCodeAt(i) & 0xff;
      i = i + 1;
      if (i === str.length) {
        out = out + b64Std.charAt(c1 >> 2) + b64Std.charAt((c1 & 0x3) << 4) + "==";
        break;
      }
      let c2: number = str.charCodeAt(i) & 0xff;
      i = i + 1;
      if (i === str.length) {
        out =
          out +
          b64Std.charAt(c1 >> 2) +
          b64Std.charAt(((c1 & 0x3) << 4) | ((c2 & 0xf0) >> 4)) +
          b64Std.charAt((c2 & 0xf) << 2) +
          "=";
        break;
      }
      let c3: number = str.charCodeAt(i) & 0xff;
      i = i + 1;
      out =
        out +
        b64Std.charAt(c1 >> 2) +
        b64Std.charAt(((c1 & 0x3) << 4) | ((c2 & 0xf0) >> 4)) +
        b64Std.charAt(((c2 & 0xf) << 2) | ((c3 & 0xc0) >> 6)) +
        b64Std.charAt(c3 & 0x3f);
    }
    return out;
  };
  (globalThis as any).atob = function (input: any): string {
    let clean: string = String(input === undefined ? "" : input)
      .replace(/[\t\n\f\r ]+/g, "")
      .replace(/=+$/, "");
    let out: string = "";
    let acc: number = 0;
    let bits: number = 0;
    for (let i: number = 0; i < clean.length; i = i + 1) {
      let ch: string = clean.charAt(i);
      if (ch === "-") ch = "+";
      else if (ch === "_") ch = "/";
      let v: number = b64Std.indexOf(ch);
      if (v < 0) continue;
      acc = (acc << 6) | v;
      bits = bits + 6;
      if (bits >= 8) {
        bits = bits - 8;
        out = out + String.fromCharCode((acc >> bits) & 0xff);
      }
    }
    return out;
  };

  let cryptoObj: any = (globalThis as any).crypto || new ObjectCtor();
  cryptoObj.getRandomValues = function (typedArray: any): any {
    if (!typedArray || typeof typedArray.byteLength !== "number") {
      throw new Error("crypto.getRandomValues requires a TypedArray");
    }
    let byteLen: number = typedArray.byteLength;
    let got: number = _crypto_random_bytes(byteLen as any);
    if (got < 0) {
      throw new Error("crypto.getRandomValues failed");
    }
    let view: any = new Uint8Array(
      typedArray.buffer || typedArray,
      typedArray.byteOffset || 0,
      byteLen
    );
    for (let i: number = 0; i < byteLen; i = i + 1) {
      let b: number = _scratch_bytes_get(i as any);
      view[i] = b >= 0 ? b : 0;
    }
    return typedArray;
  };
  (globalThis as any).crypto = cryptoObj;

  let nextTimerId: number = 1;
  (globalThis as any).setTimeout = function (fn: any, _ms: any): number {
    let id: number = nextTimerId;
    nextTimerId = nextTimerId + 1;
    if (typeof fn === "function") {
      (globalThis as any).Promise.resolve().then(function (): void {
        fn();
      });
    }
    return id;
  };
  (globalThis as any).clearTimeout = function (_id: any): void {};
  (globalThis as any).queueMicrotask = function (fn: any): void {
    if (typeof fn === "function") {
      (globalThis as any).Promise.resolve().then(function (): void {
        fn();
      });
    }
  };

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
    let idStr: string = String(json.credential_id || json.id || "");
    let idBuf: any = b64urlToBuffer(idStr);
    let clientDataBuf: any = b64urlToBuffer(json.client_data_json_b64 || "");
    let authDataBuf: any = b64urlToBuffer(json.authenticator_data_b64 || "");
    let attObjB64: string = String(json.attestation_object_b64 || json.authenticator_data_b64 || "");
    let attObjBuf: any = b64urlToBuffer(attObjB64);
    let sigBuf: any = b64urlToBuffer(json.signature_der_b64 || "");
    resp.id = idBuf;
    resp.rawId = idBuf;
    resp.id_b64 = idStr;
    resp.type = "public-key";
    resp.authenticatorAttachment = "platform";
    resp.response = new ObjectCtor();
    if (isAssertion) {
      resp.response.authenticatorData = authDataBuf;
      resp.response.clientDataJSON = clientDataBuf;
      resp.response.signature = sigBuf;
      resp.response.userHandle = null;
    } else {
      resp.response.attestationObject = attObjBuf;
      resp.response.clientDataJSON = clientDataBuf;
      resp.response.transports = ["internal"];
      resp.response.getTransports = function (): any {
        let arr: any = new ArrayCtor();
        arr[0] = "internal";
        return arr;
      };
      resp.response.getAuthenticatorData = function (): any {
        return attObjBuf;
      };
      resp.response.getPublicKeyAlgorithm = function (): number {
        return -7;
      };
    }
    resp.getClientExtensionResults = function (): any {
      return new ObjectCtor();
    };
    /* `PublicKeyCredential.toJSON()` is the serialization a relying party may
     * read off the credential. */
    resp.toJSON = function (): any {
      let out: any = new ObjectCtor();
      out.id = idStr;
      out.rawId = idStr;
      out.type = "public-key";
      out.authenticatorAttachment = "platform";
      out.response = new ObjectCtor();
      if (isAssertion) {
        out.response.authenticatorData = json.authenticator_data_b64 || "";
        out.response.clientDataJSON = json.client_data_json_b64 || "";
        out.response.signature = json.signature_der_b64 || "";
        out.response.userHandle = null;
      } else {
        out.response.attestationObject = attObjB64;
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
          pendingRelatedOrigins[requestId] = { resolve: resolve, reject: reject, isAssertion: false };
        });
      }
      let ceremonyId: string = String(parsed.ceremony_id || "");
      if (!ceremonyId) {
        return PromiseCtor.reject(new Error("WebAuthn create: the host did not name the ceremony"));
      }
      return new PromiseCtor(function (resolve: any, reject: any): void {
        pendingCeremonies[ceremonyId] = { resolve: resolve, reject: reject, isAssertion: false };
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
          pendingRelatedOrigins[requestId] = { resolve: resolve, reject: reject, isAssertion: true };
        });
      }
      let ceremonyId: string = String(parsed.ceremony_id || "");
      if (!ceremonyId) {
        return PromiseCtor.reject(new Error("WebAuthn get: the host did not name the ceremony"));
      }
      return new PromiseCtor(function (resolve: any, reject: any): void {
        pendingCeremonies[ceremonyId] = { resolve: resolve, reject: reject, isAssertion: true };
      });
    }
    return PromiseCtor.resolve(buildCredentialResponse(parsed, true));
  };
  /* Called by the host when the human has answered a pending ceremony.
   * `ok` is true when the ceremony was approved and produced an assertion or
   * registration credential, false when it was denied or expired. */
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
      entry.reject(new Error((parsed && parsed.error) || "WebAuthn ceremony failed"));
      return;
    }
    let isAssertion: boolean =
      parsed.ceremony_kind === "registration"
        ? false
        : parsed.ceremony_kind === "assertion"
        ? true
        : entry.isAssertion !== false;
    entry.resolve(buildCredentialResponse(parsed, isAssertion));
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
    let isAssertion: boolean = entry.isAssertion !== false;
    entry.resolve(
      new (globalThis as any).Promise(function (innerResolve: any, innerReject: any): void {
        pendingCeremonies[ceremonyId] = {
          resolve: innerResolve,
          reject: innerReject,
          isAssertion: isAssertion,
        };
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

  /* Expose `window.PublicKeyCredential` static capabilities and JSON helpers
   * per W3C WebAuthn Level 3 so relying parties can feature-detect platform
   * passkey support and parse JSON options. */
  let pkCredCtor: any = function PublicKeyCredential(): void {};
  pkCredCtor.isUserVerifyingPlatformAuthenticatorAvailable = function (): any {
    return (globalThis as any).Promise.resolve(true);
  };
  pkCredCtor.isConditionalMediationAvailable = function (): any {
    return (globalThis as any).Promise.resolve(true);
  };
  pkCredCtor.getClientCapabilities = function (): any {
    let caps: any = new ObjectCtor();
    caps.conditionalCreate = true;
    caps.conditionalGet = true;
    caps.hybridTransport = false;
    caps.passkeyPlatformAuthenticator = true;
    caps.userVerifyingPlatformAuthenticator = true;
    caps.relatedOrigins = true;
    return (globalThis as any).Promise.resolve(caps);
  };
  pkCredCtor.parseCreationOptionsFromJSON = function (optionsJSON: any): any {
    let out: any = new ObjectCtor();
    out.rp = (optionsJSON && optionsJSON.rp) || new ObjectCtor();
    out.user = new ObjectCtor();
    if (optionsJSON && optionsJSON.user) {
      out.user.id = b64urlToBuffer(String(optionsJSON.user.id || ""));
      out.user.name = optionsJSON.user.name || "";
      out.user.displayName = optionsJSON.user.displayName || optionsJSON.user.name || "";
    }
    out.challenge = b64urlToBuffer(String((optionsJSON && optionsJSON.challenge) || ""));
    out.pubKeyCredParams = (optionsJSON && optionsJSON.pubKeyCredParams) || new ArrayCtor();
    return out;
  };
  pkCredCtor.parseRequestOptionsFromJSON = function (optionsJSON: any): any {
    let out: any = new ObjectCtor();
    out.challenge = b64urlToBuffer(String((optionsJSON && optionsJSON.challenge) || ""));
    out.rpId = (optionsJSON && optionsJSON.rpId) || "";
    out.allowCredentials = (optionsJSON && optionsJSON.allowCredentials) || new ArrayCtor();
    out.userVerification = (optionsJSON && optionsJSON.userVerification) || "preferred";
    return out;
  };
  pkCredCtor.signalUnknownCredential = function (_opts: any): any {
    return (globalThis as any).Promise.resolve(undefined);
  };
  pkCredCtor.signalAllAcceptedCredentials = function (_opts: any): any {
    return (globalThis as any).Promise.resolve(undefined);
  };
  pkCredCtor.signalCurrentUserDetails = function (_opts: any): any {
    return (globalThis as any).Promise.resolve(undefined);
  };
  (globalThis as any).PublicKeyCredential = pkCredCtor;

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
    _set_event_default_prevented(0);
    let targetEl: any = wrapElement(nodeId as any);
    let stopped: boolean = false;
    let stopImmediate: boolean = false;
    let defaultPrevented: boolean = false;
    let ev: any = new ObjectCtor();
    ev.type = eventType;
    ev.target = targetEl;
    ev.currentTarget = targetEl;
    ev.bubbles = true;
    ev.cancelable = true;
    ev.defaultPrevented = false;
    ev.clientX = x;
    ev.clientY = y;
    ev.x = x;
    ev.y = y;
    ev.preventDefault = function (): void {
      defaultPrevented = true;
      ev.defaultPrevented = true;
      _set_event_default_prevented(1);
    };
    ev.stopPropagation = function (): void {
      stopped = true;
    };
    ev.stopImmediatePropagation = function (): void {
      stopped = true;
      stopImmediate = true;
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
          while (i < len && !stopImmediate) {
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
