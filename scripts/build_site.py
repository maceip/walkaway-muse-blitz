"""Render Walkaway public pages (landing, privacy, terms, support, MCP docs) to site/."""

import html
import re
from pathlib import Path

ROOT = Path(__file__).parent.parent
SITE = ROOT / "site"

PAGE = """<!doctype html>
<html lang="en"><head><meta charset="UTF-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<meta name="application-name" content="Walkaway"/>
<meta name="description" content="{desc}"/>
<title>{title} — Walkaway</title>
<style>
body{{background:#10161f;color:#f2f5f9;font:17px/1.6 system-ui,-apple-system,"Segoe UI",sans-serif;margin:0}}
main{{max-width:760px;margin:0 auto;padding:40px 22px 64px}}
h1{{font-size:30px}} h2{{font-size:20px;margin-top:32px}} a{{color:#6fd598}}
code{{background:#1a2331;padding:2px 6px;border-radius:4px;font-size:14px}}
li{{margin:6px 0}} .muted{{color:#a8b3c2}}
</style></head><body><main>{body}</main></body></html>
"""


def md(text):
    out, para, in_list = [], [], False

    def flush():
        if para:
            out.append("<p>" + " ".join(para) + "</p>")
            para.clear()

    for line in text.splitlines():
        line = line.strip()
        if line.startswith("# "):
            flush(); out.append(f"<h1>{html.escape(line[2:])}</h1>")
        elif line.startswith("## "):
            flush(); out.append(f"<h2>{html.escape(line[3:])}</h2>")
        elif line.startswith("- "):
            flush()
            if not in_list:
                out.append("<ul>"); in_list = True
            out.append(f"<li>{inline(line[2:])}</li>")
        elif not line:
            flush()
            if in_list:
                out.append("</ul>"); in_list = False
        else:
            if in_list:
                out.append("</ul>"); in_list = False
            para.append(inline(line))
    flush()
    if in_list:
        out.append("</ul>")
    return "\n".join(out)


def inline(text):
    text = html.escape(text)
    text = re.sub(r"`([^`]+)`", r"<code>\1</code>", text)
    text = re.sub(r"\*\*([^*]+)\*\*", r"<strong>\1</strong>", text)
    return text


LANDING = """
<h1>Walkaway</h1>
<p><strong>Leave your desk. Your agent keeps working. Your glasses only interrupt you for decisions.</strong></p>
<p>Walkaway lets Muse delegate real browser work to an agent in your own Chrome,
while approval for every step happens on Meta Ray-Ban Display glasses, where the
wearer sees the live screenshot and the exact proposed action. The Muse Connector
can start, propose, monitor, reject, and stop tasks. The connector has no approve
tool: Muse can never authorize Muse's own browser actions.</p>
<h2>Links</h2>
<ul>
<li><a href="/app/">Glasses Web App</a> (token required)</li>
<li><a href="/docs">Connector documentation</a></li>
<li><a href="/privacy">Privacy policy</a></li>
<li><a href="/terms">Terms of service</a></li>
<li><a href="/support">Support</a></li>
</ul>
<p class="muted">Walkaway is self-hosted open-source software (BSD-3-Clause).
The developer operates no Walkaway servers and receives no user data.</p>
"""

SUPPORT = """
<h1>Support</h1>
<p>Email: <a href="mailto:ryan.macarthur@gmail.com">ryan.macarthur@gmail.com</a></p>
<p>Walkaway is self-hosted: the companion, connector, and browser agent run on
your own machine. When reporting a problem, include the connector
<code>/health</code> response, the active <code>decision_model</code> shown in
the glasses app, and the exact goal text. Never include API keys or tokens.</p>
"""

DOCS = """
<h1>Walkaway Connector documentation</h1>
<p>Existing MCP over streamable HTTP. Endpoint: <code>POST /mcp</code> on
<code>muse.secure.build</code>. Protocol <code>2025-03-26</code>. Authentication:
Bearer token (<code>WALKAWAY_CONNECTOR_TOKEN</code> on the host).</p>
<h2>Tools</h2>
<ul>
<li><code>walkaway_start_task</code> (write, open-world): start a task from the user's verbatim goal.</li>
<li><code>walkaway_status</code> (read-only): status, page title/URL, proposed step, approval state.</li>
<li><code>walkaway_propose_step</code> (write, open-world): propose one step. Never executes.</li>
<li><code>walkaway_request_approval</code> (write): put a proposal on the user's glasses. Cannot approve.</li>
<li><code>walkaway_reject_step</code> (write): discard the proposal permanently.</li>
<li><code>walkaway_stop_task</code> (destructive): stop and close the browser.</li>
</ul>
<h2>Data rules</h2>
<ul>
<li>Connector results are text only. Screenshots never transit the connector.</li>
<li>Executing a step requires approval recorded on the glasses Web App.</li>
<li>No task history is stored on disk.</li>
</ul>
<h2>Errors</h2>
<p>Tool failures return MCP <code>isError</code> text results. HTTP 401 means a
missing or wrong bearer token. HTTP 413 means the request body exceeded 64 KiB.</p>
"""


def main():
    SITE.mkdir(exist_ok=True)
    pages = {
        "index.html": ("Walkaway", "Delegate browser work from Muse; approve every step on your glasses", LANDING),
        "privacy.html": ("Privacy policy", "Walkaway privacy policy", md((ROOT / "docs/PRIVACY.md").read_text())),
        "terms.html": ("Terms of service", "Walkaway terms of service", md((ROOT / "docs/TERMS.md").read_text())),
        "support.html": ("Support", "Walkaway support", SUPPORT),
        "docs.html": ("Connector documentation", "Walkaway MCP connector documentation", DOCS),
    }
    for name, (title, desc, body) in pages.items():
        (SITE / name).write_text(PAGE.format(title=title, desc=desc, body=body))
    print("wrote", ", ".join(pages))


if __name__ == "__main__":
    main()
