//! Opt-in, local JavaScript scratchpad debugger, separate from the AOT page VM.
//!
//! A native worker exclusively owns the interpreted Hermes runtime. The Rust
//! owner is intentionally !Send/!Sync: no JSI handle or live ProgramState can
//! escape the worker. Commands and JSON snapshots copy their data across FFI.
//! No DOM, vault, control socket, filesystem, or network bindings are installed.

use serde_json::{Value, json};
use std::ffi::{CStr, c_char, c_void};
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::rc::Rc;

pub const MAX_SOURCE_BYTES: usize = 128 * 1024;
pub const MAX_EXPRESSION_BYTES: usize = 4096;
const MAX_BREAKPOINTS: usize = 256;
const ERROR_BYTES: usize = 1024;

unsafe extern "C" {
    fn walkaway_debugger_new(error: *mut c_char, capacity: usize) -> *mut c_void;
    fn walkaway_debugger_start(
        handle: *mut c_void,
        source: *const c_char,
        source_length: usize,
        breakpoints: *const u32,
        breakpoint_count: usize,
        error: *mut c_char,
        capacity: usize,
    ) -> i32;
    fn walkaway_debugger_command(
        handle: *mut c_void,
        kind: *const c_char,
        kind_length: usize,
        expression: *const c_char,
        expression_length: usize,
        error: *mut c_char,
        capacity: usize,
    ) -> i32;
    fn walkaway_debugger_snapshot(handle: *mut c_void) -> *mut c_char;
    fn walkaway_debugger_string_free(string: *mut c_char);
    fn walkaway_debugger_destroy(handle: *mut c_void);
}

pub struct Debugger {
    handle: NonNull<c_void>,
    // Make the ownership contract explicit even if pointer auto-traits change.
    _thread_bound: PhantomData<Rc<()>>,
}

fn error_message(buffer: &[u8; ERROR_BYTES]) -> String {
    let end = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
    String::from_utf8_lossy(&buffer[..end]).into_owned()
}

impl Debugger {
    pub fn new() -> Result<Self, String> {
        let mut error = [0_u8; ERROR_BYTES];
        // SAFETY: the writable error buffer lives for this call; the returned
        // opaque handle is exclusively owned here until Drop.
        let handle = unsafe { walkaway_debugger_new(error.as_mut_ptr().cast(), error.len()) };
        NonNull::new(handle)
            .map(|handle| Self {
                handle,
                _thread_bound: PhantomData,
            })
            .ok_or_else(|| error_message(&error))
    }

    /// Start a fresh, isolated execution. Breakpoint lines are one-based.
    /// A previous execution is interrupted and joined before replacing it.
    /// This never reads a URL, page script, or file on the caller's behalf.
    pub fn start(&mut self, source: &str, breakpoints: &[u32]) -> Result<(), String> {
        if source.is_empty() || source.len() > MAX_SOURCE_BYTES || source.contains('\0') {
            return Err(format!(
                "Source must contain 1 to {MAX_SOURCE_BYTES} UTF-8 bytes without NUL"
            ));
        }
        if breakpoints.len() > MAX_BREAKPOINTS {
            return Err(format!(
                "At most {MAX_BREAKPOINTS} line breakpoints are supported"
            ));
        }
        let mut error = [0_u8; ERROR_BYTES];
        // SAFETY: all borrowed slices are valid for their supplied lengths.
        // Native code copies them synchronously, retaining no Rust pointers.
        let ok = unsafe {
            walkaway_debugger_start(
                self.handle.as_ptr(),
                source.as_ptr().cast(),
                source.len(),
                breakpoints.as_ptr(),
                breakpoints.len(),
                error.as_mut_ptr().cast(),
                error.len(),
            )
        };
        if ok == 1 {
            Ok(())
        } else {
            Err(error_message(&error))
        }
    }

    /// Commands: pause, continue, step_into, step_over, step_out, eval, stop.
    /// Eval runs in the top paused frame; only eval accepts an expression.
    /// Pause and stop are asynchronous. Poll snapshot for observed completion.
    pub fn command(&mut self, kind: &str, expression: Option<&str>) -> Result<(), String> {
        let expression = expression.unwrap_or("");
        if kind.is_empty() || kind.len() > 32 || kind.contains('\0') {
            return Err("Invalid debugger command".into());
        }
        if expression.len() > MAX_EXPRESSION_BYTES || expression.contains('\0') {
            return Err(format!(
                "Evaluation is limited to {MAX_EXPRESSION_BYTES} UTF-8 bytes without NUL"
            ));
        }
        let mut error = [0_u8; ERROR_BYTES];
        // SAFETY: native code copies strings before returning and serializes
        // the worker command under its own mutex. No runtime access occurs here.
        let ok = unsafe {
            walkaway_debugger_command(
                self.handle.as_ptr(),
                kind.as_ptr().cast(),
                kind.len(),
                expression.as_ptr().cast(),
                expression.len(),
                error.as_mut_ptr().cast(),
                error.len(),
            )
        };
        if ok == 1 {
            Ok(())
        } else {
            Err(error_message(&error))
        }
    }

    /// Copy a bounded JSON snapshot. Values are display strings, not JS handles.
    /// Locals/scopes and stack are live only when state == "paused".
    pub fn snapshot(&self) -> Value {
        struct NativeString(*mut c_char);
        impl Drop for NativeString {
            fn drop(&mut self) {
                // SAFETY: this pointer came from walkaway_debugger_snapshot;
                // the matching allocator frees it exactly once, including unwind.
                unsafe { walkaway_debugger_string_free(self.0) };
            }
        }
        // SAFETY: self owns a live handle; native code returns an independent,
        // NUL-terminated allocation and never a pointer into mutable worker data.
        let text = NativeString(unsafe { walkaway_debugger_snapshot(self.handle.as_ptr()) });
        if text.0.is_null() {
            return json!({"state":"error", "error":"Could not allocate debugger snapshot"});
        }
        // SAFETY: the returned string remains owned by text through parsing.
        let bytes = unsafe { CStr::from_ptr(text.0).to_bytes() };
        serde_json::from_slice(bytes).unwrap_or_else(
            |error| json!({"state":"error", "error":format!("Invalid debugger snapshot: {error}")}),
        )
    }

    pub fn state(&self) -> String {
        self.snapshot()["state"]
            .as_str()
            .unwrap_or("error")
            .to_owned()
    }
}

impl Drop for Debugger {
    fn drop(&mut self) {
        // SAFETY: unique handle, destroyed once. Native shutdown interrupts the
        // VM, wakes a paused observer with Continue, and joins the worker. If a
        // native builtin cannot be interrupted within 1.5s, native code retains
        // that isolated worker state rather than blocking UI or freeing it live.
        unsafe { walkaway_debugger_destroy(self.handle.as_ptr()) };
    }
}

/// Real Hermes backend tests, usable by --debuggertest without creating a window.
/// No mocks, remote scripts, network, or AOT page runtime are involved.
pub fn selftest() -> i32 {
    use std::sync::mpsc;
    use std::time::Duration;

    // Even a regression in native shutdown must fail the CLI test, not hang CI.
    let (done, receiver) = mpsc::channel();
    let watchdog = std::thread::spawn(move || {
        if receiver.recv_timeout(Duration::from_secs(45)).is_err() {
            eprintln!("[debuggertest] FAIL: 45-second backend deadline exceeded");
            std::process::exit(1);
        }
    });
    let result = run_selftest();
    let _ = done.send(());
    let _ = watchdog.join();
    match result {
        Ok(checks) => {
            println!("[debuggertest] PASS: {checks} real Hermes backend assertions");
            0
        }
        Err(error) => {
            eprintln!("[debuggertest] FAIL: {error}");
            1
        }
    }
}

fn run_selftest() -> Result<usize, String> {
    use std::time::{Duration, Instant};

    fn require(condition: bool, message: &str, checks: &mut usize) -> Result<(), String> {
        if !condition {
            return Err(message.to_owned());
        }
        *checks += 1;
        Ok(())
    }
    fn wait(
        debugger: &Debugger,
        label: &str,
        predicate: impl Fn(&Value) -> bool,
    ) -> Result<Value, String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let snapshot = debugger.snapshot();
            if predicate(&snapshot) {
                return Ok(snapshot);
            }
            if Instant::now() >= deadline {
                return Err(format!("Timeout waiting for {label}: {snapshot}"));
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn paused(debugger: &Debugger) -> Result<Value, String> {
        wait(debugger, "pause", |snapshot| snapshot["state"] == "paused")
    }
    fn complete(debugger: &Debugger) -> Result<Value, String> {
        wait(debugger, "completion", |snapshot| {
            snapshot["state"] == "completed"
        })
    }
    fn output(snapshot: &Value) -> String {
        snapshot["output"]
            .as_array()
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    }
    fn eval(debugger: &mut Debugger, expression: &str) -> Result<Value, String> {
        debugger.command("eval", Some(expression))?;
        wait(debugger, "paused evaluation result", |snapshot| {
            snapshot["state"] == "paused"
                && snapshot["reason"] == "eval_complete"
                && snapshot["evaluation"]["expression"] == expression
        })
    }

    let mut checks = 0;
    let mut debugger = Debugger::new()?;
    require(
        debugger.state() == "idle",
        "new debugger must be idle",
        &mut checks,
    )?;
    // One-based line 8 pauses before inner(local). The call chain and values
    // make step-into/over/out assertions independent of timing or VM speed.
    const SOURCE: &str = "function inner(value) {\n  var doubled = value * 2;\n  return doubled;\n}\nfunction outer(seed) {\n  var local = seed + 1;\n  console.log('ready', local);\n  var answer = inner(local);\n  console.log('answer', answer);\n  return answer;\n}\nouter(20);\n";
    debugger.start(SOURCE, &[8])?;
    let first = paused(&debugger)?;
    require(
        first["reason"] == "breakpoint" && first["line"] == 8,
        "real breakpoint must stop at line 8",
        &mut checks,
    )?;
    require(
        first["breakpoints"][0]["resolved"] == true && first["breakpoints"][0]["line"] == 8,
        "Hermes must resolve the requested line",
        &mut checks,
    )?;
    require(
        first["stack"]
            .as_array()
            .is_some_and(|frames| frames.len() >= 2)
            && first["stack"][0]["function"] == "outer",
        "paused stack must include outer and caller",
        &mut checks,
    )?;
    require(
        first["locals"].as_array().is_some_and(|locals| {
            locals
                .iter()
                .any(|v| v["name"] == "local" && v["value"] == "21")
        }),
        "paused locals must expose local = 21",
        &mut checks,
    )?;
    require(
        first["scopes"]
            .as_array()
            .is_some_and(|scopes| !scopes.is_empty()),
        "lexical scopes must be reported",
        &mut checks,
    )?;
    require(
        output(&first).contains("ready 21"),
        "host console must capture actual output",
        &mut checks,
    )?;
    let evaluated = eval(&mut debugger, "local + seed")?;
    require(
        evaluated["evaluation"]["value"] == "41"
            && evaluated["evaluation"]["is_exception"] == false,
        "eval must access paused-frame locals",
        &mut checks,
    )?;
    let bad_eval = eval(&mut debugger, "missingScratchpadName + 1")?;
    require(
        bad_eval["evaluation"]["is_exception"] == true,
        "eval exceptions must be reported without resuming",
        &mut checks,
    )?;
    let isolation = eval(
        &mut debugger,
        "[typeof document, typeof window, typeof vault, typeof fetch, typeof XMLHttpRequest, typeof WebSocket, typeof require, typeof process, typeof __initBlitzContext, typeof blitz_find_element_by_id].join(',')",
    )?;
    require(
        isolation["evaluation"]["value"] == vec!["undefined"; 10].join(","),
        "scratchpad must not expose DOM, vault, network, process or Blitz APIs",
        &mut checks,
    )?;

    debugger.command("step_into", None)?;
    let into = paused(&debugger)?;
    require(
        into["reason"] == "step" && into["line"] == 2 && into["stack"][0]["function"] == "inner",
        "step into must enter inner at line 2",
        &mut checks,
    )?;
    debugger.command("step_over", None)?;
    let over = paused(&debugger)?;
    require(
        over["reason"] == "step" && over["line"] == 3,
        "step over must advance to return at line 3",
        &mut checks,
    )?;
    let doubled = eval(&mut debugger, "doubled")?;
    require(
        doubled["evaluation"]["value"] == "42",
        "stepped local must contain computed value 42",
        &mut checks,
    )?;
    debugger.command("step_out", None)?;
    let out = paused(&debugger)?;
    require(
        out["reason"] == "step" && out["stack"][0]["function"] == "outer",
        "step out must return to outer",
        &mut checks,
    )?;
    debugger.command("continue", None)?;
    let finished = complete(&debugger)?;
    require(
        output(&finished).contains("answer 42"),
        "continue must finish and capture answer",
        &mut checks,
    )?;

    debugger.start("throw new Error('scratchpad-boom');", &[])?;
    let thrown = wait(&debugger, "thrown error", |snapshot| {
        snapshot["state"] == "error"
    })?;
    require(
        thrown["error"]
            .as_str()
            .is_some_and(|text| text.contains("scratchpad-boom")),
        "uncaught error text must survive FFI",
        &mut checks,
    )?;
    debugger.start("function broken( {", &[])?;
    let syntax = wait(&debugger, "syntax error", |snapshot| {
        snapshot["state"] == "error"
    })?;
    require(
        syntax["error"]
            .as_str()
            .is_some_and(|text| !text.is_empty()),
        "syntax error must be visible",
        &mut checks,
    )?;

    debugger.start(
        "globalThis.scratchpadSecret = 123; console.log('first');",
        &[],
    )?;
    complete(&debugger)?;
    let mut independent = Debugger::new()?;
    independent.start("console.log(typeof scratchpadSecret);", &[])?;
    require(
        output(&complete(&independent)?) == "undefined",
        "separate debuggers must not share globals",
        &mut checks,
    )?;
    debugger.start("console.log(typeof scratchpadSecret);", &[])?;
    require(
        output(&complete(&debugger)?) == "undefined",
        "restarting must create fresh globals and output",
        &mut checks,
    )?;

    debugger.start("var ticks = 0;\nwhile (true) {\n  ticks++;\n}\n", &[])?;
    wait(&debugger, "running loop", |snapshot| {
        snapshot["state"] == "running"
    })?;
    debugger.command("pause", None)?;
    let async_pause = paused(&debugger)?;
    require(
        async_pause["reason"] == "async_pause"
            && async_pause["line"].as_u64().is_some_and(|line| line > 0),
        "async pause must interrupt a real running loop",
        &mut checks,
    )?;
    debugger.command("stop", None)?;
    wait(&debugger, "stopped loop", |snapshot| {
        snapshot["state"] == "stopped"
    })?;
    require(true, "stopped paused loop", &mut checks)?;

    for source in ["debugger; while (true) {}", "while (true) {}"] {
        let mut closing = Debugger::new()?;
        closing.start(source, &[])?;
        if source.starts_with("debugger") {
            paused(&closing)?;
        } else {
            wait(&closing, "running before close", |s| {
                s["state"] == "running"
            })?;
        }
        let start = Instant::now();
        drop(closing);
        require(
            start.elapsed() < Duration::from_millis(1400),
            "close must interrupt and join, not reach native quarantine deadline",
            &mut checks,
        )?;
    }

    let mut evaluating = Debugger::new()?;
    evaluating.start("debugger; while (true) {}", &[])?;
    paused(&evaluating)?;
    evaluating.command("eval", Some("(function () { while (true) {} })()"))?;
    let start = Instant::now();
    drop(evaluating);
    require(
        start.elapsed() < Duration::from_millis(1400),
        "close must cancel eval and its infinite-loop caller",
        &mut checks,
    )?;

    debugger.start(
        "for (var i = 0; i < 200; ++i) console.log('line', i); console.log('x'.repeat(5000));",
        &[],
    )?;
    let bounded = complete(&debugger)?;
    require(
        bounded["output"]
            .as_array()
            .is_some_and(|lines| lines.len() <= 64)
            && bounded["truncated"] == true,
        "output snapshots must be bounded",
        &mut checks,
    )?;
    require(
        debugger
            .start(&"x".repeat(MAX_SOURCE_BYTES + 1), &[])
            .is_err(),
        "oversized source must be rejected",
        &mut checks,
    )?;
    require(
        debugger.start("1;", &[0]).is_err(),
        "zero-based breakpoint must be rejected",
        &mut checks,
    )?;
    require(
        debugger.command("continue", None).is_err(),
        "cannot continue completed execution",
        &mut checks,
    )?;
    Ok(checks)
}

#[cfg(test)]
mod tests {
    #[test]
    fn real_hermes_debugger_backend() {
        assert_eq!(super::selftest(), 0);
    }
}
