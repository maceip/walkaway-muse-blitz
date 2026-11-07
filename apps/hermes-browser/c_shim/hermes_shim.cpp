/*
 * hermes_shim.cpp — the C++ boundary around Static Hermes.
 *
 * Responsibilities:
 *   1. Own the SHRuntime lifetime (_sh_init / _sh_done).
 *   2. Instantiate the AOT-compiled `dom_runtime` SHUnit produced by
 *      `shermes -typed -exported-unit=dom_runtime -emit-c`.
 *   3. Expose the document pointer to the Rust side, and the document
 *      operations to the JS side, without ever letting a pointer cross JSI.
 *   4. Dispatch events into JS and drain microtasks.
 */

#include <hermes/VM/static_h.h>
#include <hermes/hermes.h>
#include <jsi/jsi.h>

#include <cstdint>
#include <cstdio>

#include "blitz_bridge.h"

/* Emitted by shermes; see build.rs. */
extern "C" SHUnit *sh_export_dom_runtime(void);

/* Implemented in Rust (src/dom_ffi.rs). The context pointer is explicit here. */
extern "C" {
uint64_t blitz_dom_find_element(void *ctx, const char *id);
uint64_t blitz_dom_query_selector(void *ctx, const char *selector);
size_t blitz_dom_query_selector_all(void *ctx, const char *selector);
uint64_t blitz_dom_query_selector_item(void *ctx, size_t index);
uint64_t blitz_dom_create_element(void *ctx, const char *tag);
void blitz_dom_append_child(void *ctx, uint64_t parent, uint64_t child);
void blitz_dom_remove_node(void *ctx, uint64_t node);
uint64_t blitz_dom_parent_node(void *ctx, uint64_t node);
void blitz_dom_set_style(void *ctx, uint64_t node, const char *name, const char *value);
void blitz_dom_set_text(void *ctx, uint64_t node, const char *text);
int blitz_dom_load_inner_text(void *ctx, uint64_t node);
void blitz_dom_set_inner_html(void *ctx, uint64_t node, const char *html);
int blitz_dom_load_inner_html(void *ctx, uint64_t node);
void blitz_dom_set_attribute(void *ctx, uint64_t node, const char *name, const char *value);
int blitz_dom_load_attribute(void *ctx, uint64_t node, const char *name);
int blitz_dom_scratch_byte(void *ctx, size_t index);
double blitz_dom_get_bounding_rect_field(void *ctx, uint64_t node, int field);
int blitz_dom_webauthn_create(void *ctx, const char *rp_id, const char *user_name);
int blitz_dom_webauthn_get(void *ctx, const char *rp_id);
int blitz_dom_write_scratch_bytes(void *ctx, const uint8_t *bytes, size_t len);
int blitz_dom_scratch_bytes_len(void *ctx);
int blitz_dom_scratch_bytes_get(void *ctx, size_t index);
void blitz_dom_mark_dirty(void *ctx);
}

struct HermesAppRuntime {
  SHRuntime *shr;
  facebook::hermes::HermesRuntime *rt;
  void *blitz_ctx;
};

static HermesAppRuntime *g_app = nullptr;

/* The browser is single-threaded at this boundary. Every entry into a Hermes
 * runtime activates that runtime only for the duration of the call, then
 * restores the previous value. This is what allows one runtime per tab even
 * though the AOT FFI symbols cannot receive a context argument. */
struct ScopedCurrentApp {
  HermesAppRuntime *previous;
  explicit ScopedCurrentApp(HermesAppRuntime *app) : previous(g_app) { g_app = app; }
  ~ScopedCurrentApp() { g_app = previous; }
  ScopedCurrentApp(const ScopedCurrentApp &) = delete;
  ScopedCurrentApp &operator=(const ScopedCurrentApp &) = delete;
};

/* ------------------------------------------------------------------------- */
/* JS-visible bridge: the document context is bound here, not in JS.          */
/* ------------------------------------------------------------------------- */

extern "C" uint64_t blitz_find_element_by_id(const char *id) {
  if (!g_app) return 0;
  return blitz_dom_find_element(g_app->blitz_ctx, id);
}

extern "C" uint64_t blitz_query_selector(const char *selector) {
  if (!g_app) return 0;
  return blitz_dom_query_selector(g_app->blitz_ctx, selector);
}

extern "C" size_t blitz_query_selector_all(const char *selector) {
  if (!g_app) return 0;
  return blitz_dom_query_selector_all(g_app->blitz_ctx, selector);
}

extern "C" uint64_t blitz_query_selector_item(size_t index) {
  if (!g_app) return 0;
  return blitz_dom_query_selector_item(g_app->blitz_ctx, index);
}

extern "C" uint64_t blitz_create_element(const char *tag) {
  if (!g_app) return 0;
  return blitz_dom_create_element(g_app->blitz_ctx, tag);
}

extern "C" void blitz_append_child(uint64_t parent, uint64_t child) {
  if (!g_app) return;
  blitz_dom_append_child(g_app->blitz_ctx, parent, child);
}

extern "C" void blitz_remove_node(uint64_t node) {
  if (!g_app) return;
  blitz_dom_remove_node(g_app->blitz_ctx, node);
}

extern "C" uint64_t blitz_parent_node(uint64_t node) {
  if (!g_app) return 0;
  return blitz_dom_parent_node(g_app->blitz_ctx, node);
}

extern "C" void blitz_set_style_property(uint64_t node, const char *name, const char *value) {
  if (!g_app) return;
  blitz_dom_set_style(g_app->blitz_ctx, node, name, value);
}

extern "C" void blitz_set_inner_text(uint64_t node, const char *text) {
  if (!g_app) return;
  blitz_dom_set_text(g_app->blitz_ctx, node, text);
}

extern "C" int blitz_load_inner_text(uint64_t node) {
  if (!g_app) return -1;
  return blitz_dom_load_inner_text(g_app->blitz_ctx, node);
}

extern "C" void blitz_set_inner_html(uint64_t node, const char *html) {
  if (!g_app) return;
  blitz_dom_set_inner_html(g_app->blitz_ctx, node, html);
}

extern "C" int blitz_load_inner_html(uint64_t node) {
  if (!g_app) return -1;
  return blitz_dom_load_inner_html(g_app->blitz_ctx, node);
}

extern "C" void blitz_set_attribute(uint64_t node, const char *name, const char *value) {
  if (!g_app) return;
  blitz_dom_set_attribute(g_app->blitz_ctx, node, name, value);
}

extern "C" int blitz_load_attribute(uint64_t node, const char *name) {
  if (!g_app) return -1;
  return blitz_dom_load_attribute(g_app->blitz_ctx, node, name);
}

extern "C" int blitz_scratch_byte(size_t index) {
  if (!g_app) return 0;
  return blitz_dom_scratch_byte(g_app->blitz_ctx, index);
}

extern "C" double blitz_get_bounding_rect_field(uint64_t node, int field) {
  if (!g_app) return 0.0;
  return blitz_dom_get_bounding_rect_field(g_app->blitz_ctx, node, field);
}

extern "C" int blitz_webauthn_create(const char *rp_id, const char *user_name) {
  if (!g_app) return -1;
  return blitz_dom_webauthn_create(g_app->blitz_ctx, rp_id, user_name);
}

extern "C" int blitz_webauthn_get(const char *rp_id) {
  if (!g_app) return -1;
  return blitz_dom_webauthn_get(g_app->blitz_ctx, rp_id);
}

extern "C" int blitz_write_scratch_bytes(const uint8_t *bytes, size_t len) {
  if (!g_app) return -1;
  return blitz_dom_write_scratch_bytes(g_app->blitz_ctx, bytes, len);
}

extern "C" int blitz_scratch_bytes_len(void) {
  if (!g_app) return -1;
  return blitz_dom_scratch_bytes_len(g_app->blitz_ctx);
}

extern "C" int blitz_scratch_bytes_get(size_t index) {
  if (!g_app) return -1;
  return blitz_dom_scratch_bytes_get(g_app->blitz_ctx, index);
}

extern "C" void blitz_mark_dirty(void) {
  if (!g_app) return;
  blitz_dom_mark_dirty(g_app->blitz_ctx);
}

/* ------------------------------------------------------------------------- */
/* Runtime lifecycle                                                          */
/* ------------------------------------------------------------------------- */

static void report_pending_exception(SHRuntime *shr) {
  SHLegacyValue thrown = _sh_get_clear_thrown_value(shr);
  (void)thrown;
  std::fprintf(stderr, "[hermes] dom_runtime unit threw during initialisation\n");
}

extern "C" HermesAppRuntime *hermes_app_init(void *blitz_ctx_ptr) {
  auto *app = new HermesAppRuntime();
  app->blitz_ctx = blitz_ctx_ptr;
  ScopedCurrentApp current(app);

  app->shr = _sh_init(0, nullptr);
  if (!app->shr) {
    std::fprintf(stderr, "[hermes] _sh_init failed\n");
    delete app;
    return nullptr;
  }

  SHLegacyValue result;
  if (!_sh_unit_init_guarded(app->shr, sh_export_dom_runtime, &result)) {
    report_pending_exception(app->shr);
    _sh_done(app->shr);
    delete app;
    return nullptr;
  }

  app->rt = _sh_get_hermes_runtime(app->shr);
  if (!app->rt) {
    std::fprintf(stderr, "[hermes] no JSI runtime behind SHRuntime\n");
    _sh_done(app->shr);
    delete app;
    return nullptr;
  }

  /* Hand control to the DOM runtime so it can bind its listeners. */
  try {
    facebook::jsi::Runtime &j = *app->rt;
    facebook::jsi::Function init =
        j.global().getPropertyAsFunction(j, "__initBlitzContext");
    init.call(j);
    j.drainMicrotasks();
  } catch (const facebook::jsi::JSIException &e) {
    std::fprintf(stderr, "[hermes] __initBlitzContext threw JSIException: %s\n", e.what());
    _sh_done(app->shr);
    delete app;
    return nullptr;
  } catch (const std::exception &e) {
    std::fprintf(stderr, "[hermes] __initBlitzContext threw std::exception: %s\n", e.what());
    _sh_done(app->shr);
    delete app;
    return nullptr;
  }

  return app;
}

extern "C" bool hermes_app_dispatch_event(
    HermesAppRuntime *app,
    const char *event_type,
    uint64_t hit_node_id,
    float x,
    float y) {
  if (!app || !app->rt) return false;
  ScopedCurrentApp current(app);
  facebook::jsi::Runtime &j = *app->rt;

  try {
    facebook::jsi::Function dispatch =
        j.global().getPropertyAsFunction(j, "__dispatchBlitzEvent");
    dispatch.call(
        j,
        facebook::jsi::String::createFromUtf8(j, event_type ? event_type : ""),
        facebook::jsi::Value(static_cast<double>(hit_node_id)),
        facebook::jsi::Value(static_cast<double>(x)),
        facebook::jsi::Value(static_cast<double>(y)));

    j.drainMicrotasks();
    return true;
  } catch (const facebook::jsi::JSIException &e) {
    std::fprintf(stderr, "[hermes] event dispatch threw JSIException: %s\n", e.what());
    return false;
  } catch (const std::exception &e) {
    std::fprintf(stderr, "[hermes] event dispatch threw std::exception: %s\n", e.what());
    return false;
  } catch (...) {
    std::fprintf(stderr, "[hermes] event dispatch threw unknown exception\n");
    return false;
  }
}

extern "C" bool hermes_app_eval_script(
    HermesAppRuntime *app,
    const char *script_utf8,
    const char *source_url) {
  if (!app || !app->rt || !script_utf8) return false;
  ScopedCurrentApp current(app);
  facebook::jsi::Runtime &j = *app->rt;
  try {
    auto buf = std::make_shared<facebook::jsi::StringBuffer>(script_utf8);
    j.evaluateJavaScript(buf, source_url ? source_url : "<eval>");
    j.drainMicrotasks();
    return true;
  } catch (const facebook::jsi::JSIException &e) {
    std::fprintf(stderr, "[hermes] eval threw JSIException: %s\n", e.what());
    return false;
  } catch (const std::exception &e) {
    std::fprintf(stderr, "[hermes] eval threw std::exception: %s\n", e.what());
    return false;
  } catch (...) {
    std::fprintf(stderr, "[hermes] eval threw unknown exception\n");
    return false;
  }
}

extern "C" void hermes_app_destroy(HermesAppRuntime *app) {
  if (!app) return;
  /* No jsi handles may be live at this point: they point into the runtime. */
  HermesAppRuntime *previous = g_app;
  g_app = app;
  if (app->shr) _sh_done(app->shr);
  app->shr = nullptr;
  app->rt = nullptr;
  g_app = previous == app ? nullptr : previous;
  delete app;
}
