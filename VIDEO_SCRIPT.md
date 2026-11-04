# Secure Browser with Approval — 3-minute demo video script

Total: 3:00. Format: two windows — Muse conversation left, real browser right.
Phone approval page appears as a held phone or a phone-width capture. Glasses
appear only in the final shot. Every action is a real tool call from the
shipped build; nothing is staged in post.

## Pre-flight

- Hosted stack healthy: `curl https://muse.secure.build/health`
- `MODEL_API_KEY` set so the decision-model line reads `muse-spark-1.3`.
  If missing on record day, use the fallback line in Shot 5 verbatim.
- Approval page open on the phone with the relay token (`?k=`), task reset.
- One warm run on the research fixture; then the recorded run on Google Flights
  only if the Flights scenario has been verified that day. Else record on the
  fixture and say so on screen.

## Script

### Shot 1 — The gap (0:00–0:20)
VISUAL: Muse chat. Browser window idle beside the chat.
SAY: "Muse can plan a trip, compare products, fill a form. Muse cannot touch
your browser — and you would not want an unsupervised agent clicking through
your accounts anyway. Secure Browser with Approval closes both sides of that gap."

### Shot 2 — Delegate (0:20–0:45)
VISUAL: Type the goal in Muse. Connector starts the task; the browser opens
Google Flights on its own.
SAY: "One sentence. The connector starts a browser agent in my own Chrome.
No site-specific scripts — the agent reads a fresh indexed table of every
operable element on the page."

### Shot 3 — Typed speed (0:45–1:25)
VISUAL: Three proposal cycles. Approval page on phone: screenshot, action line,
confidence, model name. Approve, approve, approve. Browser fields fill in.
Elapsed timer visible.
SAY: "Each step is one typed decision: an operation and one observed target,
in a single Muse Spark call. Targets the page never offered are rejected
before anything executes. A page fingerprint kills stale decisions, and every
decision is consumed once — a retry cannot double-click."

### Shot 4 — The brake (1:25–1:55)
VISUAL: Agent proposes a wrong date or airport. Reject on the phone. Browser
does not move. New proposal arrives, correct this time. Approve.
SAY: "Wrong date. Rejected — and rejection deletes the proposal on the
companion, so a rejected step can never execute late. Proposing is free.
Executing is mine."

### Shot 5 — The capability split (1:55–2:30)
VISUAL: Connector `tools/list` on screen. Highlight the six tools.
SAY: "Look at what Muse can call: start, status, propose, request approval,
reject, stop. There is no approve tool. Muse can never authorize Muse's own
browser actions — approval exists only on my page, in front of my eyes, with
the screenshot as evidence. Screenshots never even pass through the connector."
FALLBACK LINE (only if no MODEL_API_KEY on record day): "Today's recording runs
the labeled TypeSafe fallback chooser; the Muse Spark chooser ships in the repo
behind one environment variable, and the page always prints the real model name."

### Shot 6 — Finish, then glasses (2:30–3:00)
VISUAL: Final approval; results page complete in the browser. Cut: same
approval page rendered on Ray-Ban Display glasses (or simulator, labeled).
Approve one last step by voice.
SAY: "Task done. And because the approval page is just a web page with WebMCP
tools, the same brake works in my lens when my hands are busy. Secure Browser
with Approval: Muse gets a browser, the human keeps the brake. Open source —
link below."

## Capture rules

- Every browser movement must follow an audible/visible approval in-frame.
- Keep model name, confidence, and elapsed values unedited and legible.
- Subtitle tool names when spoken: `walkaway_start_task`, `walkaway_propose_step`.
- If a step stalls, cut to tests running green — never cut to imply speed.
