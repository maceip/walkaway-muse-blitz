use crate::debugger::Debugger;
use serde_json::Value;
use std::collections::BTreeSet;

pub const EXAMPLE: &str = "function square(value) {\n  const result = value * value;\n  return result;\n}\nconst answer = square(7);\nconsole.log('answer', answer);\n";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Source,
    Expression,
}

pub struct DebuggerUi {
    pub open: bool,
    pub focus: Option<Focus>,
    select_all: bool,
    pub source: String,
    pub expression: String,
    pub breakpoints: BTreeSet<u32>,
    pub snapshot: Value,
    pub error: Option<String>,
    backend: Option<Debugger>,
}

impl DebuggerUi {
    pub fn new() -> Self {
        Self {
            open: false,
            focus: None,
            select_all: false,
            source: EXAMPLE.into(),
            expression: "value".into(),
            breakpoints: BTreeSet::from([2]),
            snapshot: serde_json::json!({"state": "idle"}),
            error: None,
            backend: None,
        }
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.focus = None;
        self.select_all = false;
    }

    pub fn poll(&mut self) -> bool {
        let Some(backend) = &self.backend else {
            return false;
        };
        let snapshot = backend.snapshot();
        if self.snapshot == snapshot {
            return false;
        }
        self.snapshot = snapshot;
        true
    }

    pub fn action(&mut self, id: &str) -> bool {
        self.error = None;
        if id != "dbg-source" && id != "dbg-expression" {
            self.select_all = false;
        }
        match id {
            "dbg-source" => {
                self.focus = Some(Focus::Source);
                self.select_all = false;
            }
            "dbg-expression" => {
                self.focus = Some(Focus::Expression);
                self.select_all = false;
            }
            "dbg-run" => {
                if self.busy() {
                    self.error = Some("Stop the running script before starting again.".into());
                } else {
                    if self.backend.is_none() {
                        match Debugger::new() {
                            Ok(backend) => self.backend = Some(backend),
                            Err(error) => self.error = Some(error),
                        }
                    }
                    if let Some(backend) = &mut self.backend {
                        let lines: Vec<u32> = self.breakpoints.iter().copied().collect();
                        if let Err(error) = backend.start(&self.source, &lines) {
                            self.error = Some(error);
                        }
                    }
                    self.focus = None;
                }
            }
            "dbg-clear" => {
                if self.busy() {
                    self.error = Some("Stop the script to change its breakpoints.".into());
                } else {
                    self.breakpoints.clear();
                }
            }
            "dbg-example" => {
                if !self.busy() {
                    self.source = EXAMPLE.into();
                    self.breakpoints = BTreeSet::from([2]);
                }
            }
            "dbg-pause" | "dbg-continue" | "dbg-step_into" | "dbg-step_over" | "dbg-step_out"
            | "dbg-stop" | "dbg-eval" => {
                if let Some(backend) = &mut self.backend {
                    let expression = (id == "dbg-eval").then_some(self.expression.as_str());
                    if let Err(error) = backend.command(id.trim_start_matches("dbg-"), expression) {
                        self.error = Some(error);
                    }
                }
            }
            _ => {
                if let Some(line) = id.strip_prefix("dbg-line-").and_then(|v| v.parse().ok()) {
                    if self.busy() {
                        self.error = Some("Stop the script to change its breakpoints.".into());
                    } else if !self.breakpoints.remove(&line) {
                        self.breakpoints.insert(line);
                    }
                } else {
                    return false;
                }
            }
        }
        self.poll();
        true
    }

    pub fn busy(&self) -> bool {
        matches!(
            self.snapshot["state"].as_str(),
            Some("running" | "paused" | "starting" | "evaluating" | "stopping")
        )
    }

    pub fn edit(&mut self, text: &str) {
        if self.select_all {
            self.clear_input();
        }
        match self.focus {
            Some(Focus::Source) if !self.busy() => {
                if self.source.len() + text.len() <= 64 * 1024 {
                    self.source.push_str(text);
                }
            }
            Some(Focus::Expression) => {
                if self.expression.len() + text.len() <= 4096 {
                    self.expression.push_str(text);
                }
            }
            _ => {}
        }
    }

    pub fn select_all(&mut self) {
        self.select_all = true;
    }

    pub fn backspace(&mut self) {
        if self.select_all {
            self.clear_input();
            return;
        }
        match self.focus {
            Some(Focus::Source) if !self.busy() => {
                self.source.pop();
            }
            Some(Focus::Expression) => {
                self.expression.pop();
            }
            _ => {}
        }
    }

    pub fn clear_input(&mut self) {
        self.select_all = false;
        match self.focus {
            Some(Focus::Source) if !self.busy() => self.source.clear(),
            Some(Focus::Expression) => self.expression.clear(),
            _ => {}
        }
    }

    pub fn source_changed(&mut self, source: String) {
        if source.len() <= 64 * 1024 && !self.busy() {
            self.source = source;
            self.breakpoints
                .retain(|line| *line as usize <= self.source.lines().count().max(1));
        }
    }

    pub fn render(&self, height: f64) -> String {
        let state = self.snapshot["state"].as_str().unwrap_or("idle");
        let paused = state == "paused";
        let current = self.snapshot["line"].as_u64().unwrap_or(0);
        let mut lines = String::new();
        for (index, text) in self.source.split('\n').take(1000).enumerate() {
            let line = index as u32 + 1;
            let mark = if self.breakpoints.contains(&line) {
                "●"
            } else {
                "·"
            };
            let active = if current == u64::from(line) && paused {
                " current"
            } else {
                ""
            };
            lines.push_str(&format!("<div class=\"dbg-line{active}\"><div class=\"dbg-gutter\" id=\"dbg-line-{line}\">{mark} {line}</div><div class=\"dbg-code\">{}</div></div>", escape(text)));
        }
        let mut frames = String::new();
        if let Some(stack) = self.snapshot["stack"].as_array() {
            for frame in stack.iter().take(32) {
                let index = frame["index"].as_u64().unwrap_or(0);
                frames.push_str(&format!(
                    "<div class=\"dbg-frame\">#{index} {} :{}</div>",
                    escape(frame["function"].as_str().unwrap_or("anonymous")),
                    frame["line"]
                ));
            }
        }
        let detail = serde_json::to_string_pretty(&serde_json::json!({
            "reason": self.snapshot["reason"],
            "breakpoints": self.snapshot["breakpoints"],
            "stack": self.snapshot["stack"],
            "locals": self.snapshot["locals"],
            "scopes": self.snapshot["scopes"],
            "evaluation": self.snapshot["evaluation"],
            "output": self.snapshot["output"],
            "error": self.error.as_deref().map(Value::from).unwrap_or_else(|| self.snapshot["error"].clone()),
        })).unwrap_or_default();
        let selection_note = if self.select_all {
            " · All input selected"
        } else {
            ""
        };
        let source_focus = if self.focus == Some(Focus::Source) {
            " focused"
        } else {
            ""
        };
        let expression_focus = if self.focus == Some(Focus::Expression) {
            " focused"
        } else {
            ""
        };
        format!(
            r#"<style>
        .dbg {{ position:absolute; left:0; right:0; bottom:0; height:{height}px; background-color:#d2d5dc; border-top:3px solid #62768c; color:#18283a; font-family:Menlo,monospace; font-size:11px; padding:5px 8px; overflow:hidden; }}
        .dbg-head,.dbg-tools,.dbg-evalrow {{ display:flex; flex-direction:row; align-items:center; gap:6px; height:25px; }}
        .dbg-title {{ font-weight:bold; flex:1; }} .dbg-note {{ color:#33475b; font-size:10px; height:20px; white-space:nowrap; overflow:hidden; }}
        .dbg-tools {{ flex-wrap:wrap; height:25px; }}
        .dbg-btn {{ flex-shrink:0; white-space:nowrap; padding:3px 7px; border:1px solid #f9fbff; border-right-color:#697887; border-bottom-color:#697887; background-color:#dde1e7; }}
        .dbg-btn:hover {{ background-color:#edf1f6; }}
        .dbg-columns {{ display:flex; flex-direction:row; gap:8px; height:{content_height}px; }}
        .dbg-source {{ flex:1; min-width:0; overflow:scroll; background-color:#f5f6f8; border:1px solid #7a8896; padding:3px; }}
        .dbg-details {{ width:38%; overflow:scroll; background-color:#f5f6f8; border:1px solid #7a8896; padding:4px; white-space:pre; font-size:10px; }}
        .dbg-line {{ display:flex; flex-direction:row; min-height:17px; }} .dbg-gutter {{ width:48px; flex-shrink:0; color:#435a70; }} .dbg-code {{ white-space:pre; }}
        .current {{ background-color:#ffe6a0; }} .focused {{ border:2px solid #376b9d; }} .dbg-frame {{ padding:2px; white-space:nowrap; }}        .dbg-expression {{ flex:1; min-width:0; background-color:white; border:1px solid #7a8896; padding:3px; white-space:pre; }}
        @media (max-width:700px) {{ .dbg-tools {{ height:50px; }} .dbg-columns {{ height:{compact_height}px; }} .dbg-note {{ font-size:9px; }} }}
        </style><div class="dbg" id="debugger-pane">
        <div class="dbg-head"><div class="dbg-title">Hermes JavaScript Debugger · {state}</div><div class="dbg-btn" id="dbg-close">Close</div></div>
        <div class="dbg-note">Local scratchpad. No page DOM/vault/network. Edit at end; Cmd+A replaces all. AOT page code is not source-debuggable.{selection_note}</div>
        <div class="dbg-tools"><div class="dbg-btn" id="dbg-run">Run</div><div class="dbg-btn" id="dbg-pause">Pause</div><div class="dbg-btn" id="dbg-continue">Resume</div><div class="dbg-btn" id="dbg-step_over">Step over</div><div class="dbg-btn" id="dbg-step_into">Step in</div><div class="dbg-btn" id="dbg-step_out">Step out</div><div class="dbg-btn" id="dbg-stop">Stop</div><div class="dbg-btn" id="dbg-clear">Clear breakpoints</div><div class="dbg-btn" id="dbg-example">Example</div></div>
        <div class="dbg-columns"><div class="dbg-source{source_focus}" id="dbg-source">{lines}</div><div class="dbg-details" id="dbg-details">{frames}<pre>{detail}</pre></div></div>
        <div class="dbg-evalrow"><div>Evaluate:</div><div class="dbg-expression{expression_focus}" id="dbg-expression">{expression}</div><div class="dbg-btn" id="dbg-eval">Eval</div></div>
        </div>"#,
            height = height,
            content_height = (height - 106.0).max(40.0),
            compact_height = (height - 131.0).max(40.0),
            state = escape(state),
            selection_note = selection_note,
            lines = lines,
            frames = frames,
            detail = escape(&detail),
            expression = escape(&self.expression),
        )
    }
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_and_output_are_text_not_chrome_markup() {
        let mut ui = DebuggerUi::new();
        ui.source = "</div><script>attack()</script>".into();
        ui.snapshot = serde_json::json!({"state":"paused", "output":["<img src=x>"], "line":1});
        let html = ui.render(300.0);
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<img src=x>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn select_all_replaces_only_the_focused_buffer() {
        let mut ui = DebuggerUi::new();
        ui.action("dbg-expression");
        ui.select_all();
        ui.edit("answer + 1");
        assert_eq!(ui.expression, "answer + 1");
        assert_eq!(ui.source, EXAMPLE);
        ui.action("dbg-source");
        ui.select_all();
        ui.backspace();
        assert!(ui.source.is_empty());
    }

    #[test]
    fn running_source_and_breakpoints_cannot_be_rewritten() {
        let mut ui = DebuggerUi::new();
        ui.snapshot = serde_json::json!({"state":"running"});
        ui.action("dbg-source");
        ui.select_all();
        ui.edit("replacement");
        ui.action("dbg-clear");
        assert_eq!(ui.source, EXAMPLE);
        assert!(ui.breakpoints.contains(&2));
        assert!(ui.error.is_some());
    }
}
