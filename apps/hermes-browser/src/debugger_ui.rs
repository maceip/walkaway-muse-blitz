use crate::debugger::Debugger;
use blitz_dom::{DocumentMutator, LocalName, Namespace, NodeId, QualName};
use blitz_html::HtmlDocument;
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
        // Source leaves now have retained IDs; clicking their text must still
        // focus the editor rather than being swallowed by chrome's ID bubbling.
        let source_line = id
            .strip_prefix("dbg-code-")
            .or_else(|| id.strip_prefix("dbg-row-"))
            .and_then(|line| line.parse::<u32>().ok());
        let id = if source_line.is_some() {
            "dbg-source"
        } else {
            id
        };
        if matches!(
            id,
            "dbg-status" | "dbg-stack" | "dbg-details" | "dbg-details-json" | "dbg-selection"
        ) {
            return false;
        }
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

    /// Patch a pane previously built by `render` without parsing or replacing
    /// any HTML. Returns whether visible DOM state changed (false also means
    /// the pane is absent), not whether the caller should rebuild the shell.
    ///
    /// Source/expression edits and pane geometry still use the shell path.
    /// Backend polls only change status, stack/details text and line markers;
    /// source text, editor focus/selection, node IDs and scroll containers stay
    /// intact. The caller resolves the damaged document and requests redraw.
    pub fn patch_snapshot(&self, doc: &mut HtmlDocument) -> bool {
        if doc.get_element_by_id("debugger-pane").is_none() {
            return false;
        }
        let mut mutator = doc.mutate();
        let mut changed = patch_text(&mut mutator, "dbg-status", self.state());
        changed |= patch_text(&mut mutator, "dbg-stack", &self.stack_text());
        changed |= patch_text(&mut mutator, "dbg-details-json", &self.details_text());
        let current = self.current_line();
        for (index, _) in self.source.split('\n').take(1000).enumerate() {
            let line = index as u32 + 1;
            let class = if current == u64::from(line) {
                "dbg-line current"
            } else {
                "dbg-line"
            };
            changed |= patch_class(&mut mutator, &format!("dbg-row-{line}"), class);
            changed |= patch_text(
                &mut mutator,
                &format!("dbg-line-{line}"),
                &self.gutter_text(line),
            );
        }
        changed
    }

    fn state(&self) -> &str {
        self.snapshot["state"].as_str().unwrap_or("idle")
    }

    fn current_line(&self) -> u64 {
        if self.state() == "paused" {
            self.snapshot["line"].as_u64().unwrap_or(0)
        } else {
            0
        }
    }

    fn gutter_text(&self, line: u32) -> String {
        let mark = if self.breakpoints.contains(&line) {
            "●"
        } else {
            "·"
        };
        format!("{mark} {line}")
    }

    fn stack_text(&self) -> String {
        self.snapshot["stack"]
            .as_array()
            .into_iter()
            .flatten()
            .take(32)
            .map(|frame| {
                format!(
                    "#{} {} :{}",
                    frame["index"].as_u64().unwrap_or(0),
                    frame["function"].as_str().unwrap_or("anonymous"),
                    frame["line"]
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn details_text(&self) -> String {
        serde_json::to_string_pretty(&serde_json::json!({
            "reason": self.snapshot["reason"],
            "breakpoints": self.snapshot["breakpoints"],
            "stack": self.snapshot["stack"],
            "locals": self.snapshot["locals"],
            "scopes": self.snapshot["scopes"],
            "evaluation": self.snapshot["evaluation"],
            "output": self.snapshot["output"],
            "error": self.error.as_deref().map(Value::from).unwrap_or_else(|| self.snapshot["error"].clone()),
        })).unwrap_or_default()
    }

    pub fn render(&self, height: f64) -> String {
        let current = self.current_line();
        let mut lines = String::new();
        for (index, text) in self.source.split('\n').take(1000).enumerate() {
            let line = index as u32 + 1;
            let active = if current == u64::from(line) {
                " current"
            } else {
                ""
            };
            lines.push_str(&format!("<div class=\"dbg-line{active}\" id=\"dbg-row-{line}\"><div class=\"dbg-gutter\" id=\"dbg-line-{line}\">{}</div><div class=\"dbg-code\" id=\"dbg-code-{line}\">{}</div></div>", self.gutter_text(line), escape(text)));
        }
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
        .current {{ background-color:#ffe6a0; }} .focused {{ border:2px solid #376b9d; }} .dbg-stack {{ margin:0; line-height:16px; white-space:pre; }} .dbg-expression {{ flex:1; min-width:0; background-color:white; border:1px solid #7a8896; padding:3px; white-space:pre; }}
        @media (max-width:700px) {{ .dbg-tools {{ height:50px; }} .dbg-columns {{ height:{compact_height}px; }} .dbg-note {{ font-size:9px; }} }}
        </style><div class="dbg" id="debugger-pane">
        <div class="dbg-head"><div class="dbg-title">Hermes JavaScript Debugger · <span id="dbg-status">{state}</span></div><div class="dbg-btn" id="dbg-close">Close</div></div>
        <div class="dbg-note">Local scratchpad. No page DOM/vault/network. Edit at end; Cmd+A replaces all. AOT page code is not source-debuggable.<span id="dbg-selection">{selection_note}</span></div>
        <div class="dbg-tools"><div class="dbg-btn" id="dbg-run">Run</div><div class="dbg-btn" id="dbg-pause">Pause</div><div class="dbg-btn" id="dbg-continue">Resume</div><div class="dbg-btn" id="dbg-step_over">Step over</div><div class="dbg-btn" id="dbg-step_into">Step in</div><div class="dbg-btn" id="dbg-step_out">Step out</div><div class="dbg-btn" id="dbg-stop">Stop</div><div class="dbg-btn" id="dbg-clear">Clear breakpoints</div><div class="dbg-btn" id="dbg-example">Example</div></div>
        <div class="dbg-columns"><div class="dbg-source{source_focus}" id="dbg-source">{lines}</div><div class="dbg-details" id="dbg-details"><pre class="dbg-stack" id="dbg-stack">{frames}</pre><pre id="dbg-details-json">{detail}</pre></div></div>
        <div class="dbg-evalrow"><div>Evaluate:</div><div class="dbg-expression{expression_focus}" id="dbg-expression">{expression}</div><div class="dbg-btn" id="dbg-eval">Eval</div></div>
        </div>"#,
            height = height,
            content_height = (height - 106.0).max(40.0),
            compact_height = (height - 131.0).max(40.0),
            state = escape(self.state()),
            selection_note = selection_note,
            lines = lines,
            frames = escape(&self.stack_text()),
            detail = escape(&self.details_text()),
            expression = escape(&self.expression),
        )
    }
}

// These are deliberately leaf-only operations, not innerHTML/textContent
// replacements. Blitz's set_node_text accepts text nodes, not elements, and an
// initially empty leaf (for example the idle stack) has no parsed text child.
fn patch_text(mutator: &mut DocumentMutator<'_>, id: &str, value: &str) -> bool {
    let Some(element_id) = mutator.doc.get_element_by_id(id) else {
        return false;
    };
    let text_id: Option<NodeId> = mutator.doc.get_node(element_id).and_then(|node| {
        node.children.iter().copied().find(|child_id| {
            mutator
                .doc
                .get_node(*child_id)
                .is_some_and(|child| child.text_data().is_some())
        })
    });
    if let Some(text_id) = text_id {
        if mutator
            .doc
            .get_node(text_id)
            .and_then(|node| node.text_data())
            .is_some_and(|text| text.content == value)
        {
            return false;
        }
        mutator.set_node_text(text_id, value);
    } else {
        if value.is_empty() {
            return false;
        }
        let text_id = mutator.create_text_node(value);
        mutator.append_children(element_id, &[text_id]);
    }
    true
}

fn patch_class(mutator: &mut DocumentMutator<'_>, id: &str, value: &str) -> bool {
    let Some(node_id) = mutator.doc.get_element_by_id(id) else {
        return false;
    };
    let name = LocalName::from("class");
    if mutator
        .doc
        .get_node(node_id)
        .and_then(|node| node.attr(name.clone()))
        == Some(value)
    {
        return false;
    }
    mutator.set_attribute(
        node_id,
        QualName::new(None, Namespace::from(""), name),
        value,
    );
    true
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
    use blitz_dom::{DocumentConfig, Point};
    use blitz_traits::shell::{ColorScheme, Viewport};

    fn document(ui: &DebuggerUi) -> HtmlDocument {
        let mut doc = HtmlDocument::from_html(
            &format!(
                "<!DOCTYPE html><html><body>{}</body></html>",
                ui.render(300.0)
            ),
            DocumentConfig {
                viewport: Some(Viewport::new(960, 720, 1.0, ColorScheme::Light)),
                ..Default::default()
            },
        );
        doc.resolve(0.0);
        doc
    }

    fn element(doc: &HtmlDocument, id: &str) -> NodeId {
        doc.get_element_by_id(id)
            .unwrap_or_else(|| panic!("missing #{id}"))
    }

    fn text(doc: &HtmlDocument, id: &str) -> String {
        doc.get_node(element(doc, id)).unwrap().text_content()
    }

    fn class(doc: &HtmlDocument, id: &str) -> String {
        doc.get_node(element(doc, id))
            .unwrap()
            .attr(LocalName::from("class"))
            .unwrap_or("")
            .into()
    }

    fn subtree_ids(doc: &HtmlDocument, root: NodeId) -> Vec<NodeId> {
        let mut ids = vec![root];
        for child in &doc.get_node(root).unwrap().children {
            ids.extend(subtree_ids(doc, *child));
        }
        ids
    }

    #[test]
    fn snapshot_updates_preserve_retained_nodes_scroll_focus_and_selection() {
        let mut ui = DebuggerUi::new();
        ui.open = true;
        ui.source = (1..=90)
            .map(|line| format!("const line{line} = '{}';", "source".repeat(32)))
            .collect::<Vec<_>>()
            .join("\n");
        ui.expression = "line7 + line8".into();
        ui.focus = Some(Focus::Expression);
        ui.select_all();
        ui.snapshot = serde_json::json!({
            "state": "paused", "line": 2, "reason": "breakpoint",
            "stack": [{"index": 0, "function": "scratchpad", "line": 2}],
            "output": vec!["output".repeat(100); 4]
        });
        // Exactly one parse. All subsequent changes run through the real Blitz
        // mutator and style/layout resolver, not a substitute document/runtime.
        let mut doc = document(&ui);
        let document_id = doc.id();
        let pane_id = element(&doc, "debugger-pane");
        let original_nodes = subtree_ids(&doc, pane_id);
        let source_id = element(&doc, "dbg-source");
        let details_id = element(&doc, "dbg-details");
        let expression_id = element(&doc, "dbg-expression");
        let source_scroll = Point { x: 41.0, y: 153.0 };
        let details_scroll = Point { x: 23.0, y: 31.0 };
        for (id, offset) in [(source_id, source_scroll), (details_id, details_scroll)] {
            let node = doc.get_node(id).unwrap();
            assert!(
                f64::from(node.scroll_width() - node.client_width()) > offset.x,
                "{:?}: scroll width {}, client width {}, requested x {}",
                node.attr(LocalName::from("id")),
                node.scroll_width(),
                node.client_width(),
                offset.x
            );
            assert!(
                f64::from(node.scroll_height() - node.client_height()) > offset.y,
                "{:?}: scroll height {}, client height {}, requested y {}",
                node.attr(LocalName::from("id")),
                node.scroll_height(),
                node.client_height(),
                offset.y
            );
            assert!(doc.scroll_node_by_has_changed(id, -offset.x, -offset.y, |_| {}));
            assert_eq!(*doc.get_node(id).unwrap().scroll_offset(), offset);
        }
        assert!(!ui.patch_snapshot(&mut doc), "initial DOM already matches");
        let source_text = text(&doc, "dbg-source");
        let expression_text = text(&doc, "dbg-expression");
        let selection_note = text(&doc, "dbg-selection");
        let source_class = class(&doc, "dbg-source");
        let expression_class = class(&doc, "dbg-expression");

        for revision in 1..=24 {
            let selected = match revision % 3 {
                0 => expression_id,
                1 => element(&doc, "dbg-code-7"),
                _ => element(&doc, "dbg-details-json"),
            };
            let focused = match revision % 3 {
                0 => expression_id,
                1 => source_id,
                _ => details_id,
            };
            doc.set_focus_to(focused);
            doc.set_text_selection(selected, 1, selected, 5);
            let selection = doc.get_text_selection_ranges();
            assert!(!selection.is_empty());
            let paused = revision % 2 == 0;
            let line = revision % 89 + 1;
            ui.snapshot = serde_json::json!({
                "state": if paused { "paused" } else { "running" },
                "line": line, "reason": "step",
                "stack": if paused {
                    serde_json::json!([{"index": 0, "function": "scratchpad", "line": line}])
                } else {
                    serde_json::json!([])
                },
                "locals": [{"name": "revision", "value": revision.to_string()}],
                "evaluation": {"value": revision.to_string()},
                "output": vec![format!("{} revision {revision}", "output".repeat(100)); 4]
            });
            assert!(ui.patch_snapshot(&mut doc));
            doc.resolve(0.0);
            assert_eq!(doc.id(), document_id);
            assert_eq!(subtree_ids(&doc, pane_id), original_nodes);
            assert_eq!(
                *doc.get_node(source_id).unwrap().scroll_offset(),
                source_scroll
            );
            assert_eq!(
                *doc.get_node(details_id).unwrap().scroll_offset(),
                details_scroll
            );
            assert_eq!(doc.get_focussed_node_id(), Some(focused));
            assert_eq!(doc.get_text_selection_ranges(), selection);
            assert_eq!(text(&doc, "dbg-source"), source_text);
            assert_eq!(text(&doc, "dbg-expression"), expression_text);
            assert_eq!(text(&doc, "dbg-selection"), selection_note);
            assert_eq!(class(&doc, "dbg-source"), source_class);
            assert_eq!(class(&doc, "dbg-expression"), expression_class);
            assert!(ui.focus == Some(Focus::Expression) && ui.select_all);
            assert_eq!(text(&doc, "dbg-status"), ui.state());
            assert_eq!(text(&doc, "dbg-stack"), ui.stack_text());
            assert_eq!(text(&doc, "dbg-details-json"), ui.details_text());
            for row in 1..=90 {
                assert_eq!(
                    class(&doc, &format!("dbg-row-{row}")),
                    if paused && row == line {
                        "dbg-line current"
                    } else {
                        "dbg-line"
                    }
                );
            }
            assert!(
                !ui.patch_snapshot(&mut doc),
                "same snapshot must not damage DOM"
            );
        }
    }

    #[test]
    fn empty_snapshot_leaves_gain_text_once_and_updates_never_parse_markup() {
        let mut ui = DebuggerUi::new();
        ui.snapshot = serde_json::json!({"state": ""});
        let mut doc = document(&ui);
        let status_id = element(&doc, "dbg-status");
        let stack_id = element(&doc, "dbg-stack");
        assert!(doc.get_node(status_id).unwrap().children.is_empty());
        assert!(doc.get_node(stack_id).unwrap().children.is_empty());
        assert!(!ui.patch_snapshot(&mut doc));
        ui.snapshot = serde_json::json!({
            "state": "paused", "line": 1,
            "stack": [{"function": "<script id='injected'>attack()</script>", "line": "<img>"}],
            "error": "backend <error>",
            "output": ["</pre><div id='injected'>not markup</div> & café"]
        });
        assert!(ui.patch_snapshot(&mut doc));
        doc.resolve(0.0);
        let status_children = doc.get_node(status_id).unwrap().children.clone();
        let stack_children = doc.get_node(stack_id).unwrap().children.clone();
        assert_eq!(status_children.len(), 1);
        assert_eq!(stack_children.len(), 1);
        assert!(doc.get_element_by_id("injected").is_none());
        assert_eq!(text(&doc, "dbg-stack"), ui.stack_text());
        assert_eq!(text(&doc, "dbg-details-json"), ui.details_text());
        ui.error = Some("local <failure> & retry".into());
        assert!(ui.patch_snapshot(&mut doc));
        let details: Value = serde_json::from_str(&text(&doc, "dbg-details-json")).unwrap();
        assert_eq!(details["error"], "local <failure> & retry");
        ui.error = None;
        assert!(ui.patch_snapshot(&mut doc));
        let details: Value = serde_json::from_str(&text(&doc, "dbg-details-json")).unwrap();
        assert_eq!(details["error"], "backend <error>");
        ui.snapshot = serde_json::json!({"state": "", "stack": []});
        assert!(ui.patch_snapshot(&mut doc));
        doc.resolve(0.0);
        assert_eq!(doc.get_node(status_id).unwrap().children, status_children);
        assert_eq!(doc.get_node(stack_id).unwrap().children, stack_children);
        assert_eq!(text(&doc, "dbg-status"), "");
        assert_eq!(text(&doc, "dbg-stack"), "");
        assert!(!ui.patch_snapshot(&mut doc));
    }

    #[test]
    fn retained_gutters_and_source_ids_keep_their_original_actions() {
        let mut ui = DebuggerUi::new();
        let mut doc = document(&ui);
        let pane_id = element(&doc, "debugger-pane");
        let original_nodes = subtree_ids(&doc, pane_id);
        assert!(ui.action("dbg-code-1"));
        assert!(ui.focus == Some(Focus::Source));
        assert_eq!(ui.breakpoints, BTreeSet::from([2]));
        assert!(ui.action("dbg-row-3"));
        assert!(ui.focus == Some(Focus::Source));
        assert!(ui.action("dbg-line-2"));
        assert!(ui.action("dbg-line-3"));
        assert!(ui.patch_snapshot(&mut doc));
        doc.resolve(0.0);
        assert_eq!(text(&doc, "dbg-line-2"), "· 2");
        assert_eq!(text(&doc, "dbg-line-3"), "● 3");
        assert_eq!(subtree_ids(&doc, pane_id), original_nodes);
        assert!(!ui.patch_snapshot(&mut doc));
        ui.error = Some("keep this error while reading details".into());
        ui.select_all();
        assert!(!ui.action("dbg-details-json"));
        assert!(ui.error.is_some() && ui.select_all);
    }

    #[test]
    fn patching_absent_pane_is_a_noop() {
        let ui = DebuggerUi::new();
        let mut doc = HtmlDocument::from_html("<!DOCTYPE html>", DocumentConfig::default());
        let document_id = doc.id();
        assert!(!ui.patch_snapshot(&mut doc));
        assert_eq!(doc.id(), document_id);
        assert!(doc.get_element_by_id("debugger-pane").is_none());
    }

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
