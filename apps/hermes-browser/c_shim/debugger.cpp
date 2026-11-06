/*
 * Opt-in, interpreted Hermes scratchpad debugger. This does NOT debug or attach
 * to the AOT dom_runtime, and never installs Blitz, vault, control, filesystem,
 * or network bindings. The only host functions are bounded console sinks.
 *
 * Runtime/JSI values live exclusively on the worker. The UI can only copy JSON,
 * enqueue commands, or invoke Hermes' explicitly thread-safe pause/timeout APIs
 * while holding the runtime-lifetime mutex. No runtime handle crosses the ABI.
 */
#ifndef HERMES_ENABLE_DEBUGGER
#error "Build debugger.cpp and libhermesvm with HERMES_ENABLE_DEBUGGER=ON"
#endif

#include <hermes/DebuggerAPI.h>
#include <hermes/hermes.h>
#include <jsi/jsi.h>

#include <algorithm>
#include <chrono>
#include <cmath>
#include <condition_variable>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <deque>
#include <iomanip>
#include <locale>
#include <memory>
#include <mutex>
#include <optional>
#include <sstream>
#include <stdexcept>
#include <string>
#include <string_view>
#include <thread>
#include <utility>
#include <vector>

namespace {
namespace jsi = facebook::jsi;
namespace h = facebook::hermes;
namespace dbg = facebook::hermes::debugger;
using Clock = std::chrono::steady_clock;

constexpr size_t kMaxSource = 128 * 1024;
constexpr size_t kMaxExpression = 4096;
constexpr size_t kMaxBreakpoints = 256;
constexpr size_t kMaxSnapshot = 1024 * 1024;
constexpr size_t kMaxValue = 256;
constexpr size_t kMaxOutput = 64;
constexpr size_t kMaxOutputLine = 1024;
constexpr uint32_t kMaxFrames = 16;
constexpr uint32_t kMaxScopes = 8;
constexpr uint32_t kMaxVariables = 64;
constexpr uint32_t kRunTimeoutMs = 10000;
constexpr uint32_t kEvalTimeoutMs = 2000;
constexpr auto kCloseTimeout = std::chrono::milliseconds(1500);
constexpr const char *kSourceURL = "walkaway-scratchpad.js";

std::string clipped(std::string text, size_t limit, bool *truncated = nullptr) {
  if (text.size() <= limit) return text;
  if (truncated) *truncated = true;
  size_t end = limit - 3;
  while (end && (static_cast<unsigned char>(text[end]) & 0xc0) == 0x80) --end;
  text.resize(end);
  text += "...";
  return text;
}

std::string quote(std::string_view text) {
  static constexpr char hex[] = "0123456789abcdef";
  std::string result = "\"";
  for (unsigned char c : text) {
    if (c == '"' || c == '\\') {
      result += '\\';
      result += static_cast<char>(c);
    } else if (c < 0x20) {
      result += "\\u00";
      result += hex[c >> 4];
      result += hex[c & 15];
    } else {
      result += static_cast<char>(c);
    }
  }
  result += '"';
  return result;
}

void appendItem(std::string &array, const std::string &item) {
  if (array.size() > 1) array += ',';
  array += item;
}

uint32_t coordinate(uint32_t value) {
  return value == dbg::kInvalidLocation ? 0 : value;
}

struct Preview {
  std::string value;
  const char *type;
};

// Never call JS toString(), JSON.stringify(), getters or proxy traps to inspect
// a value. Large strings are summarized before copying them out of the VM.
Preview preview(h::HermesRuntime &runtime, const jsi::Value &value, bool &truncated) {
  if (value.isUndefined()) return {"undefined", "undefined"};
  if (value.isNull()) return {"null", "null"};
  if (value.isBool()) return {value.getBool() ? "true" : "false", "boolean"};
  if (value.isNumber()) {
    double n = value.getNumber();
    if (std::isnan(n)) return {"NaN", "number"};
    if (std::isinf(n)) return {n < 0 ? "-Infinity" : "Infinity", "number"};
    std::ostringstream stream;
    stream.imbue(std::locale::classic());
    stream << std::setprecision(17) << n;
    return {stream.str(), "number"};
  }
  if (value.isString()) {
    auto string = value.getString(runtime);
    size_t length = string.length(runtime);
    if (length > kMaxValue) {
      truncated = true;
      return {"[String: " + std::to_string(length) + " code units]", "string"};
    }
    return {clipped(string.utf8(runtime), kMaxValue, &truncated), "string"};
  }
  if (value.isBigInt()) return {"[BigInt]", "bigint"};
  if (value.isSymbol()) return {"[Symbol]", "symbol"};
  if (value.isObject() && value.getObject(runtime).isFunction(runtime))
    return {"[Function]", "function"};
  return {"[Object]", "object"};
}

std::string variableJSON(
    h::HermesRuntime &runtime, const dbg::VariableInfo &variable, bool &truncated) {
  auto p = preview(runtime, variable.value, truncated);
  return "{\"name\":" + quote(clipped(variable.name, 128, &truncated)) +
      ",\"value\":" + quote(p.value) + ",\"type\":" + quote(p.type) + "}";
}

const char *pauseReason(dbg::PauseReason reason) {
  switch (reason) {
    case dbg::PauseReason::ScriptLoaded: return "script_loaded";
    case dbg::PauseReason::DebuggerStatement: return "debugger_statement";
    case dbg::PauseReason::Breakpoint: return "breakpoint";
    case dbg::PauseReason::StepFinish: return "step";
    case dbg::PauseReason::Exception: return "exception";
    case dbg::PauseReason::AsyncTriggerImplicit:
    case dbg::PauseReason::AsyncTriggerExplicit: return "async_pause";
    case dbg::PauseReason::EvalComplete: return "eval_complete";
  }
  return "unknown";
}

struct Snapshot {
  std::string state = "starting";
  std::string reason;
  uint64_t revision = 1;
  uint64_t execution = 0;
  uint32_t line = 0;
  uint32_t column = 0;
  std::string stack = "[]";
  std::string scopes = "[]";
  std::string locals = "[]";
  std::string breakpoints = "[]";
  std::string evaluation = "null";
  std::string error;
  std::deque<std::string> output;
  bool truncated = false;

  std::string json() const {
    std::string logs = "[";
    for (const auto &line : output) appendItem(logs, quote(line));
    logs += ']';
    std::string result = "{\"state\":" + quote(state) + ",\"reason\":" + quote(reason) +
        ",\"revision\":" + std::to_string(revision) +
        ",\"execution\":" + std::to_string(execution) +
        ",\"source_url\":" + quote(kSourceURL) +
        ",\"line\":" + std::to_string(line) + ",\"column\":" + std::to_string(column) +
        ",\"stack\":" + stack + ",\"scopes\":" + scopes + ",\"locals\":" + locals +
        ",\"selected_frame\":0,\"evaluation\":" + evaluation +
        ",\"error\":" + (error.empty() ? "null" : quote(error)) +
        ",\"output\":" + logs + ",\"breakpoints\":" + breakpoints +
        ",\"truncated\":" + (truncated ? "true" : "false") + "}";
    if (result.size() > kMaxSnapshot)
      throw std::runtime_error("Debugger snapshot exceeded its 1 MiB limit");
    return result;
  }
};

struct Request {
  std::string kind;
  std::string expression;
};

class Session final : public dbg::EventObserver {
 public:
  Session(std::string source, std::vector<uint32_t> lines, uint64_t execution)
      : source_(std::move(source)), lines_(std::move(lines)) {
    snapshot_.execution = execution;
  }

  std::string snapshot() {
    std::lock_guard lock(mutex_);
    return snapshot_.json();
  }

  void stop() {
    std::lock_guard lock(mutex_);
    if (finished_) return;
    stopping_ = true;
    snapshot_.state = "stopping";
    snapshot_.reason = "stop_requested";
    ++snapshot_.revision;
    // This is the ONLY cross-thread runtime operation other than async pause.
    // The same mutex guards clearing the pointer before worker-side deletion.
    if (runtime_) runtime_->asyncTriggerTimeout();
    cv_.notify_all();
  }

  bool waitUntilFinished() {
    std::unique_lock lock(mutex_);
    return cv_.wait_for(lock, kCloseTimeout, [&] { return finished_; });
  }

  void command(std::string kind, std::string expression) {
    if (kind == "stop") {
      stop();
      return;
    }
    std::lock_guard lock(mutex_);
    if (stopping_ || finished_) throw std::runtime_error("Execution is not active");
    if (kind == "pause") {
      if (snapshot_.state == "paused") return;
      if (snapshot_.state != "running" && snapshot_.state != "starting")
        throw std::runtime_error("Pause requires a running script");
      pauseRequested_ = true;
      if (debugger_) debugger_->triggerAsyncPause(dbg::AsyncPauseKind::Explicit);
      return;
    }
    if (kind != "continue" && kind != "step_into" && kind != "step_over" &&
        kind != "step_out" && kind != "eval")
      throw std::runtime_error("Unknown debugger command");
    if (snapshot_.state != "paused" || pending_)
      throw std::runtime_error("This command requires a paused script");
    if (kind == "eval" && expression.empty())
      throw std::runtime_error("Evaluation requires an expression");
    if (kind != "eval" && !expression.empty())
      throw std::runtime_error("Only eval accepts an expression");
    if (kind == "eval") {
      snapshot_.state = "evaluating";
      snapshot_.evaluation = "null";
    } else {
      snapshot_.state = "running";
      snapshot_.stack = snapshot_.scopes = snapshot_.locals = "[]";
      snapshot_.line = snapshot_.column = 0;
    }
    snapshot_.reason = kind;
    ++snapshot_.revision;
    pending_ = Request{std::move(kind), std::move(expression)};
    cv_.notify_all();
  }

  void run() noexcept {
    std::unique_ptr<h::HermesRuntime> runtime;
    std::string failure;
    try {
      auto config = h::hardenedHermesRuntimeConfig().rebuild()
          .withEnableEval(true)
          .withEnableJIT(false)
          .withOptimizedEval(false)
          .withAsyncBreakCheckInEval(true)
          .withES6BlockScoping(true)
          .withCompilationMode(::hermes::vm::CompilationMode::ForceEagerCompilation)
          .withGCConfig(::hermes::vm::GCConfig::Builder()
                            .withInitHeapSize(8 * 1024 * 1024)
                            .withMaxHeapSize(128 * 1024 * 1024)
                            .build())
          .build();
      runtime = h::makeHermesRuntime(config);
      installConsole(*runtime);
      auto &debugger = runtime->getDebugger();
      debugger.setEventObserver(this);
      debugger.setIsDebuggerAttached(true);
      debugger.setShouldPauseOnScriptLoad(true);
      debugger.setPauseOnThrowMode(dbg::PauseOnThrowMode::None);
      {
        std::lock_guard lock(mutex_);
        runtime_ = runtime.get();
        debugger_ = &debugger;
        if (!stopping_) {
          snapshot_.state = "running";
          ++snapshot_.revision;
        }
        if (pauseRequested_) debugger.triggerAsyncPause(dbg::AsyncPauseKind::Explicit);
        if (stopping_) runtime->asyncTriggerTimeout();
      }
      runtime->watchTimeLimit(kRunTimeoutMs);
      runtime->debugJavaScript(source_, kSourceURL, {});
      runtime->drainMicrotasks();
    } catch (const std::exception &error) {
      failure = clipped(error.what(), 4096);
    } catch (...) {
      failure = "Unknown native debugger failure";
    }
    if (runtime) {
      // No JSI value or observer is retained in Snapshot or on the UI thread.
      runtime->unwatchTimeLimit();
      runtime->getDebugger().setEventObserver(nullptr);
      runtime->getDebugger().setIsDebuggerAttached(false);
      {
        std::lock_guard lock(mutex_);
        runtime_ = nullptr;
        debugger_ = nullptr;
      }
      runtime.reset();
    }
    {
      std::lock_guard lock(mutex_);
      snapshot_.state = stopping_ ? "stopped" : failure.empty() ? "completed" : "error";
      snapshot_.reason = stopping_ ? "stopped" : failure.empty() ? "completed" : "exception";
      if (!stopping_ && !failure.empty()) {
        snapshot_.error = failure;
        appendOutputLocked("[error] " + failure);
      }
      snapshot_.stack = snapshot_.scopes = snapshot_.locals = "[]";
      snapshot_.line = snapshot_.column = 0;
      ++snapshot_.revision;
      finished_ = true;
      cv_.notify_all();
    }
  }

  dbg::Command didPause(dbg::Debugger &debugger) override {
    // Unwatch on the owning thread, never from the UI. Waiting for a human is
    // not execution time; each resume/eval receives a fresh bounded budget.
    runtime_->unwatchTimeLimit();
    {
      std::lock_guard lock(mutex_);
      if (stopping_) {
        // Eval catches even timeout errors to report EvalComplete. Re-issue
        // the timeout before Continue so cancellation also kills its caller.
        runtime_->asyncTriggerTimeout();
        return dbg::Command::continueExecution();
      }
    }
    const auto &program = debugger.getProgramState();
    if (program.getPauseReason() == dbg::PauseReason::ScriptLoaded) {
      installBreakpoints(debugger);
      debugger.setShouldPauseOnScriptLoad(false);
      runtime_->watchTimeLimit(kRunTimeoutMs);
      return dbg::Command::continueExecution();
    }

    bool truncated = false;
    std::string stack = "[", scopes = "[", locals = "[";
    const auto &trace = program.getStackTrace();
    uint32_t frameCount = std::min(trace.callFrameCount(), kMaxFrames);
    truncated |= trace.callFrameCount() > frameCount;
    uint32_t variableBudget = kMaxVariables;
    for (uint32_t frameIndex = 0; frameIndex < frameCount; ++frameIndex) {
      auto frame = trace.callFrameForIndex(frameIndex);
      appendItem(stack, "{\"index\":" + std::to_string(frameIndex) +
          ",\"function\":" + quote(clipped(frame.functionName, 256, &truncated)) +
          ",\"url\":" + quote(clipped(frame.location.fileName, 256, &truncated)) +
          ",\"line\":" + std::to_string(coordinate(frame.location.line)) +
          ",\"column\":" + std::to_string(coordinate(frame.location.column)) + "}");
      if (!variableBudget) {
        truncated = true;
        continue;
      }
      auto lexical = program.getLexicalInfo(frameIndex);
      uint32_t scopeCount = std::min(lexical.getScopesCount(), kMaxScopes);
      truncated |= lexical.getScopesCount() > scopeCount;
      for (uint32_t depth = 0; depth < scopeCount && variableBudget; ++depth) {
        std::string variables = "[";
        uint32_t count = std::min(lexical.getVariablesCountInScope(depth), variableBudget);
        truncated |= lexical.getVariablesCountInScope(depth) > count;
        for (uint32_t index = 0; index < count; ++index) {
          auto variable = program.getVariableInfo(frameIndex, depth, index);
          auto json = variableJSON(*runtime_, variable, truncated);
          appendItem(variables, json);
          if (frameIndex == 0) appendItem(locals, json);
        }
        variableBudget -= count;
        variables += ']';
        appendItem(scopes, "{\"frame\":" + std::to_string(frameIndex) +
            ",\"depth\":" + std::to_string(depth) + ",\"variables\":" + variables + "}");
      }
    }
    stack += ']';
    scopes += ']';
    locals += ']';
    std::string evaluation;
    if (program.getPauseReason() == dbg::PauseReason::EvalComplete) {
      auto result = program.getEvalResult();
      auto value = preview(*runtime_, result.value, truncated);
      evaluation = "{\"expression\":" + quote(evalExpression_) +
          ",\"frame\":0,\"value\":" + quote(value.value) +
          ",\"type\":" + quote(value.type) +
          ",\"is_exception\":" + (result.isException ? "true" : "false") +
          ",\"error\":" + (result.isException
              ? quote(clipped(result.exceptionDetails.text, 1024, &truncated)) : "null") + "}";
    }
    std::unique_lock lock(mutex_);
    snapshot_.stack = std::move(stack);
    snapshot_.scopes = std::move(scopes);
    snapshot_.locals = std::move(locals);
    snapshot_.truncated |= truncated;
    if (!evaluation.empty()) snapshot_.evaluation = std::move(evaluation);
    snapshot_.state = stopping_ ? "stopping" : "paused";
    snapshot_.reason = pauseReason(program.getPauseReason());
    if (frameCount) {
      auto location = trace.callFrameForIndex(0).location;
      snapshot_.line = coordinate(location.line);
      snapshot_.column = coordinate(location.column);
    }
    ++snapshot_.revision;
    cv_.wait(lock, [&] { return stopping_ || pending_.has_value(); });
    if (stopping_) {
      runtime_->asyncTriggerTimeout();
      return dbg::Command::continueExecution();
    }
    Request request = std::move(*pending_);
    pending_.reset();
    lock.unlock();
    runtime_->watchTimeLimit(request.kind == "eval" ? kEvalTimeoutMs : kRunTimeoutMs);
    if (request.kind == "eval") {
      evalExpression_ = request.expression;
      return dbg::Command::eval(request.expression, 0);
    }
    if (request.kind == "step_into") return dbg::Command::step(dbg::StepMode::Into);
    if (request.kind == "step_over") return dbg::Command::step(dbg::StepMode::Over);
    if (request.kind == "step_out") return dbg::Command::step(dbg::StepMode::Out);
    return dbg::Command::continueExecution();
  }

 private:
  void appendOutputLocked(std::string text) {
    if (snapshot_.output.size() == kMaxOutput) {
      snapshot_.output.pop_front();
      snapshot_.truncated = true;
    }
    snapshot_.output.push_back(clipped(std::move(text), kMaxOutputLine, &snapshot_.truncated));
    ++snapshot_.revision;
  }

  void installConsole(h::HermesRuntime &runtime) {
    jsi::Object console(runtime);
    for (const char *method : {"log", "info", "warn", "error", "debug"}) {
      std::string level(method);
      auto function = jsi::Function::createFromHostFunction(
          runtime, jsi::PropNameID::forAscii(runtime, method), 0,
          [this, level](jsi::Runtime &base, const jsi::Value &, const jsi::Value *args, size_t count) {
            auto &runtime = static_cast<h::HermesRuntime &>(base);
            std::string line = level == "log" ? "" : "[" + level + "] ";
            bool truncated = count > 32;
            for (size_t i = 0; i < std::min(count, size_t(32)); ++i) {
              if (i) line += ' ';
              line += preview(runtime, args[i], truncated).value;
              if (line.size() > kMaxOutputLine) {
                truncated = true;
                break;
              }
            }
            std::lock_guard lock(mutex_);
            snapshot_.truncated |= truncated;
            appendOutputLocked(std::move(line));
            return jsi::Value::undefined();
          });
      console.setProperty(runtime, method, std::move(function));
    }
    runtime.global().setProperty(runtime, "console", std::move(console));
  }

  void installBreakpoints(dbg::Debugger &debugger) {
    auto scripts = debugger.getLoadedScripts();
    auto script = std::find_if(scripts.begin(), scripts.end(), [](const auto &location) {
      return location.fileName == kSourceURL;
    });
    if (script == scripts.end())
      throw std::runtime_error("Hermes did not publish the scratchpad script location");
    std::string breakpoints = "[";
    for (uint32_t line : lines_) {
      dbg::SourceLocation location;
      location.line = line;
      location.fileId = script->fileId;
      location.fileName = kSourceURL;
      auto id = debugger.setBreakpoint(location);
      if (id == dbg::kInvalidBreakpoint)
        throw std::runtime_error("Hermes could not create a requested breakpoint");
      auto info = debugger.getBreakpointInfo(id);
      appendItem(breakpoints, "{\"id\":" + std::to_string(id) +
          ",\"requested_line\":" + std::to_string(line) +
          ",\"line\":" + std::to_string(info.resolved ? coordinate(info.resolvedLocation.line) : 0) +
          ",\"column\":" + std::to_string(info.resolved ? coordinate(info.resolvedLocation.column) : 0) +
          ",\"resolved\":" + (info.resolved ? "true" : "false") +
          ",\"enabled\":" + (info.enabled ? "true" : "false") + "}");
    }
    breakpoints += ']';
    std::lock_guard lock(mutex_);
    snapshot_.breakpoints = std::move(breakpoints);
    ++snapshot_.revision;
  }

  const std::string source_;
  const std::vector<uint32_t> lines_;
  std::mutex mutex_;
  std::condition_variable cv_;
  Snapshot snapshot_;
  std::optional<Request> pending_;
  bool stopping_ = false;
  bool finished_ = false;
  bool pauseRequested_ = false;
  // Published only under mutex_; dereferenced cross-thread only for documented
  // async triggers. Everything else, including destruction, is worker-only.
  h::HermesRuntime *runtime_ = nullptr;
  dbg::Debugger *debugger_ = nullptr;
  std::string evalExpression_;
};

class Handle {
 public:
  ~Handle() { close(); }

  void start(std::string source, std::vector<uint32_t> lines) {
    if (quarantined_ || !close())
      throw std::runtime_error("Previous script did not stop promptly; close this debugger before retrying");
    session_ = std::make_shared<Session>(std::move(source), std::move(lines), ++execution_);
    // Shared ownership is solely a last-resort native-operation shutdown guard;
    // ordinary interpreted loops terminate via asyncTriggerTimeout and join.
    worker_ = std::thread([session = session_] { session->run(); });
  }

  std::string snapshot() {
    if (session_) return session_->snapshot();
    Snapshot idle;
    idle.state = "idle";
    return idle.json();
  }

  void command(std::string kind, std::string expression) {
    if (!session_) throw std::runtime_error("Start a local script first");
    session_->command(std::move(kind), std::move(expression));
  }

 private:
  bool close() noexcept {
    if (!worker_.joinable()) return !quarantined_;
    session_->stop();
    if (session_->waitUntilFinished()) {
      worker_.join();
      return true;
    }
    // Hermes cannot preempt every native builtin (e.g. a pathological regexp).
    // Never hang the window or free a live runtime on the wrong thread. A stuck
    // worker retains ONLY its isolated state until it exits, not UI pointers.
    // The handle refuses restart after this exceptional quarantine. No unsafe
    // pthread cancellation or forced runtime destruction is attempted.
    worker_.detach();
    quarantined_ = true;
    std::fprintf(stderr, "[debugger] native operation did not stop within 1500ms; isolated worker quarantined\n");
    return false;
  }

  std::shared_ptr<Session> session_;
  std::thread worker_;
  uint64_t execution_ = 0;
  bool quarantined_ = false;
};

void writeError(char *buffer, size_t capacity, const char *message) noexcept {
  if (!buffer || !capacity) return;
  size_t count = std::min(capacity - 1, std::strlen(message));
  std::memcpy(buffer, message, count);
  buffer[count] = 0;
}

char *copyString(const std::string &text) {
  auto *copy = static_cast<char *>(std::malloc(text.size() + 1));
  if (!copy) return nullptr;
  std::memcpy(copy, text.c_str(), text.size() + 1);
  return copy;
}
} // namespace

extern "C" void *walkaway_debugger_new(char *error, size_t capacity) noexcept {
  try {
    return new Handle();
  } catch (const std::exception &e) {
    writeError(error, capacity, e.what());
  } catch (...) {
    writeError(error, capacity, "Could not allocate debugger");
  }
  return nullptr;
}

extern "C" int walkaway_debugger_start(
    void *handle, const char *source, size_t sourceLength,
    const uint32_t *breakpoints, size_t breakpointCount,
    char *error, size_t capacity) noexcept {
  try {
    if (!handle || !source || !sourceLength || sourceLength > kMaxSource)
      throw std::runtime_error("Source must contain 1 to 131072 UTF-8 bytes");
    if (breakpointCount > kMaxBreakpoints || (breakpointCount && !breakpoints))
      throw std::runtime_error("At most 256 line breakpoints are supported");
    std::string code(source, sourceLength);
    if (code.find('\0') != std::string::npos)
      throw std::runtime_error("Source must not contain NUL bytes");
    uint32_t lineCount = 1 + std::count(code.begin(), code.end(), '\n');
    std::vector<uint32_t> lines;
    for (size_t index = 0; index < breakpointCount; ++index) {
      if (!breakpoints[index] || breakpoints[index] > lineCount)
        throw std::runtime_error("Breakpoint lines are one-based and must exist in the source");
      lines.push_back(breakpoints[index]);
    }
    std::sort(lines.begin(), lines.end());
    lines.erase(std::unique(lines.begin(), lines.end()), lines.end());
    static_cast<Handle *>(handle)->start(std::move(code), std::move(lines));
    return 1;
  } catch (const std::exception &e) {
    writeError(error, capacity, e.what());
  } catch (...) {
    writeError(error, capacity, "Could not start debugger worker");
  }
  return 0;
}

extern "C" int walkaway_debugger_command(
    void *handle, const char *kind, size_t kindLength,
    const char *expression, size_t expressionLength,
    char *error, size_t capacity) noexcept {
  try {
    if (!handle || !kind || !kindLength || kindLength > 32)
      throw std::runtime_error("Invalid debugger command");
    if (expressionLength > kMaxExpression || (expressionLength && !expression))
      throw std::runtime_error("Evaluation is limited to 4096 UTF-8 bytes");
    std::string expr(expression ? expression : "", expressionLength);
    if (expr.find('\0') != std::string::npos)
      throw std::runtime_error("Evaluation must not contain NUL bytes");
    static_cast<Handle *>(handle)->command(std::string(kind, kindLength), std::move(expr));
    return 1;
  } catch (const std::exception &e) {
    writeError(error, capacity, e.what());
  } catch (...) {
    writeError(error, capacity, "Could not dispatch debugger command");
  }
  return 0;
}

extern "C" char *walkaway_debugger_snapshot(void *handle) noexcept {
  try {
    if (handle) return copyString(static_cast<Handle *>(handle)->snapshot());
  } catch (...) {
  }
  return copyString("{\"state\":\"error\",\"error\":\"Could not copy debugger snapshot\"}");
}

extern "C" void walkaway_debugger_string_free(char *string) noexcept {
  std::free(string);
}

extern "C" void walkaway_debugger_destroy(void *handle) noexcept {
  delete static_cast<Handle *>(handle);
}
