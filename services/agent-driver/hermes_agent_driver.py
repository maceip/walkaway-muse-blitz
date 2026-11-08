#!/usr/bin/env python3
"""Canonical Agent-Channel Driver for Hermes Blitz Browser.

Provides:
1. `plan_validation_repair_with_local_llm(snapshot, speaker_profile)`:
   Invokes the real local `mlx-community/Qwen3-4B-Instruct-2507-4bit` model via MLX on
   Apple Silicon to inspect a Hermes `observe.result` snapshot (including any server-rendered
   validation error banner) and output the corrective `fill_form` operation bound to live
   `node_ref` handles.
2. `plan_recall_or_llm(snapshot, speaker_profile)`:
   Checks whether Hermes `observe.result` already matched a stored `FormRecipe`
   (`matched_recipe_id`). If matched, emits `{"kind": "replay_recipe"}` with `llm_calls == 0`
   in sub-millisecond time; otherwise falls back to `plan_validation_repair_with_local_llm`.
"""

import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

REPO_ROOT = Path(__file__).resolve().parents[2]


def resolve_mlx_python() -> Path:
    env_val = os.environ.get("HERMES_MLX_PYTHON")
    if env_val and Path(env_val).exists():
        return Path(env_val)
    uv_tool = Path.home() / ".local" / "share" / "uv" / "tools" / "mlx-lm" / "bin" / "python3"
    if uv_tool.exists():
        return uv_tool
    which_py = shutil.which("python3")
    return Path(which_py) if which_py else Path(sys.executable)


def resolve_hermes_binary() -> Path:
    env_val = os.environ.get("HERMES_BROWSER_BIN")
    if env_val and Path(env_val).exists():
        return Path(env_val)
    in_repo = REPO_ROOT / "apps" / "hermes-browser" / "target" / "release" / "hermes-blitz-browser"
    if in_repo.exists():
        return in_repo
    external = Path.home() / "hermes-blitz-browser" / "target" / "release" / "hermes-blitz-browser"
    return external


MLX_PYTHON = resolve_mlx_python()
MODEL_ID = "mlx-community/Qwen3-4B-Instruct-2507-4bit"

_INLINE_MLX_SCRIPT = r"""
import json, sys
from mlx_lm import load, generate

payload = json.loads(sys.stdin.read())
model_id = payload["model_id"]
prompt_messages = payload["messages"]

model, tokenizer = load(model_id)
prompt = tokenizer.apply_chat_template(
    prompt_messages,
    tokenize=False,
    add_generation_prompt=True,
)
out = generate(
    model,
    tokenizer,
    prompt=prompt,
    max_tokens=320,
    verbose=False,
)
print(json.dumps({"model_id": model_id, "output": out}))
"""


def _extract_json_object(raw_text: str) -> dict[str, Any]:
    match = re.search(r"\{.*\}", raw_text, flags=re.DOTALL)
    if not match:
        raise ValueError(f"Model output did not contain a JSON object: {raw_text!r}")
    return json.loads(match.group(0))


def plan_validation_repair_with_local_llm(
    snapshot: dict[str, Any],
    speaker_profile: dict[str, str],
) -> dict[str, Any]:
    """Run real local Qwen3-4B-Instruct-2507-4bit inference to repair form validation errors."""
    if not MLX_PYTHON.exists():
        raise RuntimeError(f"MLX Python interpreter not found at {MLX_PYTHON}")

    elements = snapshot.get("elements") or []
    controls_summary = []
    by_key: dict[str, dict[str, Any]] = {}
    for el in elements:
        key = el.get("dom_id") or el.get("name")
        if not key or el.get("tag") not in {"input", "select", "textarea"}:
            continue
        by_key[key] = el
        controls_summary.append(
            {
                "field": key,
                "tag": el.get("tag"),
                "input_type": el.get("input_type"),
                "label": el.get("label"),
                "current_value": el.get("value"),
            }
        )

    messages = [
        {
            "role": "system",
            "content": (
                "You are a form-repair co-processor for the Hermes supervised browser. "
                "Output ONLY a valid JSON object mapping each form field name to its corrected string value "
                "using the provided speaker_profile so all validation errors on the page are satisfied. "
                "Do not output markdown fences or prose."
            ),
        },
        {
            "role": "user",
            "content": json.dumps(
                {
                    "page_text": snapshot.get("text", ""),
                    "controls": controls_summary,
                    "speaker_profile": speaker_profile,
                },
                ensure_ascii=False,
            ),
        },
    ]

    t0 = time.perf_counter()
    proc = subprocess.run(
        [str(MLX_PYTHON), "-c", _INLINE_MLX_SCRIPT],
        input=json.dumps({"model_id": MODEL_ID, "messages": messages}),
        capture_output=True,
        text=True,
        check=True,
        timeout=60,
    )
    elapsed_ms = (time.perf_counter() - t0) * 1000.0

    last_line = [ln for ln in proc.stdout.strip().splitlines() if ln.strip()][-1]
    res = json.loads(last_line)
    raw_output = res["output"]
    field_values = _extract_json_object(raw_output)

    parsed_url = urlparse(snapshot.get("url") or "")
    origin_str = (
        f"{parsed_url.scheme}://{parsed_url.netloc}"
        if parsed_url.scheme and parsed_url.netloc
        else "http://127.0.0.1:8795"
    )
    username_str = str(
        field_values.get("email")
        or field_values.get("username")
        or speaker_profile.get("email")
        or "ryan@zunftmax.secure.build"
    )

    fields_ops = []
    for field_name, val in field_values.items():
        if field_name not in by_key:
            continue
        el = by_key[field_name]
        tag = (el.get("tag") or "").lower()
        input_type = (el.get("input_type") or "").lower()
        if input_type == "password":
            fields_ops.append(
                {
                    "target_ref": el["node_ref"],
                    "action": "fill_secret",
                    "origin": origin_str,
                    "username": username_str,
                }
            )
            continue
        if tag == "select":
            action = "select"
        elif tag == "input" and input_type in {"checkbox", "radio"}:
            action = "check"
        else:
            action = "type"
        fields_ops.append(
            {
                "target_ref": el["node_ref"],
                "action": action,
                "value": str(val),
            }
        )

    if not fields_ops:
        raise RuntimeError(f"Model produced no matching form fields: {raw_output!r}")

    return {
        "llm_calls": 1,
        "model_id": MODEL_ID,
        "latency_ms": elapsed_ms,
        "raw_model_output": raw_output,
        "operation": {
            "kind": "fill_form",
            "fields": fields_ops,
        },
    }


def plan_recall_or_llm(
    snapshot: dict[str, Any],
    speaker_profile: dict[str, str],
) -> dict[str, Any]:
    """Use zero-LLM FormRecipe recall when matched_recipe_id is present; otherwise call Qwen3-4B."""
    t0 = time.perf_counter()
    recipe_id = snapshot.get("matched_recipe_id")
    if recipe_id:
        elapsed_ms = (time.perf_counter() - t0) * 1000.0
        return {
            "llm_calls": 0,
            "model_id": None,
            "latency_ms": elapsed_ms,
            "operation": {
                "kind": "replay_recipe",
                "recipe_id": recipe_id,
            },
        }
    return plan_validation_repair_with_local_llm(snapshot, speaker_profile)


DEFAULT_USER_PROFILE: dict[str, str] = {
    "full_name": "Ryan MacArthur",
    "first_name": "Ryan",
    "last_name": "MacArthur",
    "email": "ryan@zunftmax.secure.build",
    "phone": "415-555-0199",
    "affiliation": "Zunftmax Systems",
    "talk_title": "Containment-First Sandboxing: What Firecracker Leaves as Homework",
    "tshirt_size": "L",
    "abstract_summary": (
        "We dissect hardware-virtualized microVM boundaries, seccomp-bpf filter synthesis, "
        "and deterministic capability channels for untrusted agent execution at scale."
    ),
}


def load_user_profile(overrides: dict[str, str] | None = None) -> dict[str, str]:
    """Load user profile from ~/.walkaway/profile.json (if present) merged with overrides."""
    profile = dict(DEFAULT_USER_PROFILE)
    profile_path = Path(os.environ.get("WALKAWAY_PROFILE_PATH", Path.home() / ".walkaway" / "profile.json"))
    if profile_path.exists():
        try:
            loaded = json.loads(profile_path.read_text(encoding="utf-8"))
            if isinstance(loaded, dict):
                for k, v in loaded.items():
                    if v is not None:
                        profile[str(k)] = str(v)
        except Exception:
            pass
    if overrides:
        for k, v in overrides.items():
            if v is not None:
                profile[str(k)] = str(v)
    return profile


def format_domain_card(url: str, action_label: str, fields_summary: list[str]) -> tuple[str, str]:
    """Format a <=80 char title and <=160 char body for the glasses DAT / Android approval card."""
    parsed = urlparse(url or "")
    domain = parsed.netloc or parsed.path or "browser"
    title = f"COMMIT {domain}: {action_label}".strip()
    if len(title) > 80:
        title = title[:77] + "..."
    body = "; ".join(fields_summary) if fields_summary else f"Approve commit on {url}"
    if len(body) > 160:
        body = body[:157] + "..."
    return title, body


def plan_general_web_form_with_local_llm(
    snapshot: dict[str, Any],
    goal: str,
    user_profile: dict[str, str] | None = None,
    preserve_existing: bool = False,
) -> dict[str, Any]:
    """Use local Qwen3-4B on Apple Silicon to map any real webpage's form controls to the user's goal."""
    if not MLX_PYTHON.exists():
        raise RuntimeError(f"MLX Python interpreter not found at {MLX_PYTHON}")

    profile = load_user_profile(user_profile)

    elements = snapshot.get("elements") or []
    controls_summary = []
    by_key: dict[str, dict[str, Any]] = {}
    submit_el: dict[str, Any] | None = None
    best_submit_score = -1

    for el in elements:
        if not el.get("visible", True) or not el.get("enabled", True):
            continue
        tag = (el.get("tag") or "").lower()
        input_type = (el.get("input_type") or "").lower()
        label = (el.get("label") or "").strip()

        if tag == "button" or (tag == "input" and input_type in {"submit", "button"}):
            dom_id_lower = (el.get("dom_id") or "").lower()
            name_lower = (el.get("name") or "").lower()
            label_lower = label.lower()
            score = 0
            if "submit" in dom_id_lower or "submit" in name_lower:
                score += 100
            if any(
                kw in label_lower
                for kw in (
                    "submit",
                    "agree & join",
                    "agree and join",
                    "sign up",
                    "create account",
                    "register",
                    "place order",
                    "submit order",
                    "complete",
                    "confirm",
                )
            ):
                score += 80
            elif any(
                kw in label_lower
                for kw in ("join", "agree", "continue", "next", "save", "send", "order", "login", "sign in")
            ):
                score += 50
            if input_type == "submit":
                score += 10
            if submit_el is None or score > best_submit_score:
                submit_el = el
                best_submit_score = score
            continue

        if tag not in {"input", "select", "textarea"}:
            continue
        if input_type in {"hidden", "reset", "file", "image"}:
            continue

        key = el.get("dom_id") or el.get("name") or el.get("node_ref")
        if not key:
            continue
        # For radio/checkbox inputs that share a `name`, disambiguate by option value, label, or node_ref
        opt_val = el.get("value") if input_type in {"radio", "checkbox"} else None
        if input_type in {"radio", "checkbox"} and (key in by_key or opt_val or label):
            suffix = opt_val or label or el.get("node_ref")
            candidate = f"{key}:{suffix}"
            if candidate not in by_key:
                key = candidate
            else:
                key = f"{key}:{el.get('node_ref')}"
        by_key[key] = el
        ctrl_entry: dict[str, Any] = {
            "field_key": key,
            "name": el.get("name") or el.get("dom_id") or "",
            "tag": tag,
            "input_type": input_type or ("select" if tag == "select" else "textarea" if tag == "textarea" else "text"),
            "label": label,
        }
        if el.get("placeholder"):
            ctrl_entry["placeholder"] = el.get("placeholder")
        if input_type in {"radio", "checkbox"}:
            ctrl_entry["option_value"] = opt_val or label
            ctrl_entry["checked"] = bool(el.get("checked"))
        else:
            ctrl_entry["current_value"] = el.get("value")
        if el.get("options"):
            ctrl_entry["options"] = el.get("options")
        controls_summary.append(ctrl_entry)

    if not by_key and submit_el is not None:
        title, body = format_domain_card(
            snapshot.get("url", ""),
            submit_el.get("label") or "Submit",
            [f"Goal: {goal}"],
        )
        return {
            "llm_calls": 0,
            "model_id": None,
            "latency_ms": 0.0,
            "stage_operation": None,
            "commit_operation": {
                "kind": "submit",
                "target_ref": submit_el["node_ref"],
            },
            "submit_target": submit_el,
            "card_title": title,
            "card_body": body,
            "fields_preview": [],
        }

    if preserve_existing:
        fields_preview = []
        for field_key, el in by_key.items():
            tag = (el.get("tag") or "").lower()
            input_type = (el.get("input_type") or "").lower()
            name_or_key = el.get("name") or field_key
            if input_type == "password" or el.get("value_state") == "sensitive":
                fields_preview.append(f"{name_or_key}=[VAULT_SECRET]")
            elif tag == "input" and input_type in {"checkbox", "radio"}:
                if el.get("checked"):
                    fields_preview.append(f"{name_or_key}=true")
            else:
                cur_val = str(el.get("value") or "").strip()
                if cur_val:
                    fields_preview.append(f"{name_or_key}={cur_val[:28]}")
        commit_op = (
            {
                "kind": "submit",
                "target_ref": submit_el["node_ref"],
            }
            if submit_el is not None
            else None
        )
        action_label = (submit_el.get("label") if submit_el else None) or "Submit Form"
        title, body = format_domain_card(snapshot.get("url", ""), action_label, fields_preview)
        return {
            "llm_calls": 0,
            "model_id": "HumanEditedDOM",
            "latency_ms": 0.0,
            "stage_operation": None,
            "commit_operation": commit_op,
            "submit_target": submit_el,
            "card_title": title,
            "card_body": body,
            "fields_preview": fields_preview,
        }

    messages = [
        {
            "role": "system",
            "content": (
                "You are an autonomous browser form planner running locally on Apple Silicon.\n"
                "Given a webpage's observed form `controls`, the user's `goal`, and the user's `user_profile`, "
                "map the controls required to fulfill the goal.\n"
                "Rules:\n"
                "1. Output ONLY a single valid JSON object mapping `field_key` -> string value. Never output markdown fences or prose.\n"
                "2. Match semantic meaning across `label`, `name`, `placeholder`, and `input_type` (for example, `custname`/`author`/`speaker_name` -> user's full name; `first-name`/`first_name` -> user's first name; `last-name`/`last_name` -> user's last name; `email-address`/`email-or-phone`/`custemail`/`email` -> user's email; `custtel`/`phone`/`tel` -> user's phone; `comments`/`instructions`/`notes` -> task instructions from `goal`).\n"
                "3. Always prioritize explicit instructions in `goal` (such as sizes, toppings, delivery instructions, or custom values) over generic defaults.\n"
                "4. For `<select>` controls (`input_type == \"select\"`), choose the matching option's `value` from `options`.\n"
                "5. For `radio` and `checkbox` controls, output `\"true\"` ONLY for the `field_key` whose `option_value` or `label` should be selected according to `goal`; omit unselected `radio`/`checkbox` keys."
            ),
        },
        {
            "role": "user",
            "content": json.dumps(
                {
                    "goal": goal,
                    "page_url": snapshot.get("url", ""),
                    "page_title": snapshot.get("title", ""),
                    "page_text_excerpt": (snapshot.get("text") or "")[:600],
                    "controls": controls_summary,
                    "user_profile": profile,
                },
                ensure_ascii=False,
            ),
        },
    ]

    t0 = time.perf_counter()
    proc = subprocess.run(
        [str(MLX_PYTHON), "-c", _INLINE_MLX_SCRIPT],
        input=json.dumps({"model_id": MODEL_ID, "messages": messages}),
        capture_output=True,
        text=True,
        check=True,
        timeout=60,
    )
    elapsed_ms = (time.perf_counter() - t0) * 1000.0

    last_line = [ln for ln in proc.stdout.strip().splitlines() if ln.strip()][-1]
    res = json.loads(last_line)
    raw_output = res["output"]
    field_values = _extract_json_object(raw_output)

    parsed_url = urlparse(snapshot.get("url") or "")
    origin_str = (
        f"{parsed_url.scheme}://{parsed_url.netloc}"
        if parsed_url.scheme and parsed_url.netloc
        else "http://127.0.0.1:8795"
    )
    username_str = str(
        field_values.get("email")
        or field_values.get("email-address")
        or field_values.get("email_address")
        or field_values.get("email-or-phone")
        or field_values.get("username")
        or profile.get("email")
        or "ryan@zunftmax.secure.build"
    )

    fields_ops = []
    fields_preview = []
    for field_key, el in by_key.items():
        tag = (el.get("tag") or "").lower()
        input_type = (el.get("input_type") or "").lower()
        if input_type == "password":
            fields_ops.append(
                {
                    "target_ref": el["node_ref"],
                    "action": "fill_secret",
                    "origin": origin_str,
                    "username": username_str,
                }
            )
            fields_preview.append(f"{field_key}=[VAULT_SECRET]")
            continue

        val = field_values.get(field_key)
        if val is None or str(val).strip() == "":
            # Check profile fallback for standard speaker/user keys if omitted by model
            raw_name = (el.get("name") or el.get("dom_id") or "").lower()
            if raw_name in {"speaker_name", "full_name", "name", "custname", "author"}:
                val = profile.get("full_name")
            elif raw_name in {"first-name", "first_name", "firstname", "given-name", "given_name"}:
                val = profile.get("first_name") or (profile.get("full_name") or "Ryan MacArthur").split()[0]
            elif raw_name in {"last-name", "last_name", "lastname", "family-name", "family_name", "surname"}:
                parts = (profile.get("full_name") or "Ryan MacArthur").split()
                val = profile.get("last_name") or (parts[-1] if len(parts) > 1 else "MacArthur")
            elif raw_name in {"email", "email-address", "email_address", "email-or-phone", "custemail", "user_email"}:
                val = profile.get("email")
            elif raw_name in {"phone", "tel", "custtel"}:
                val = profile.get("phone")
            elif raw_name in {"talk_title", "title"}:
                val = profile.get("talk_title")
            elif raw_name in {"abstract_summary", "abstract", "bio"}:
                val = profile.get("abstract_summary")
            elif raw_name == "tshirt_size":
                val = profile.get("tshirt_size")
            elif tag == "select" and el.get("options"):
                non_empty_opts = [
                    o.get("value")
                    for o in (el.get("options") or [])
                    if isinstance(o, dict) and str(o.get("value") or "").strip()
                ]
                if non_empty_opts:
                    val = non_empty_opts[0]
        if val is None:
            continue

        if tag == "select":
            action = "select"
        elif tag == "input" and input_type in {"checkbox", "radio"}:
            if str(val).lower() not in {"true", "1", "yes", "on", "checked"}:
                continue
            action = "check"
            val = "true"
        else:
            action = "type"
        str_val = str(val)
        fields_ops.append(
            {
                "target_ref": el["node_ref"],
                "action": action,
                "value": str_val,
            }
        )
        fields_preview.append(f"{el.get('name') or field_key}={str_val[:28]}")

    stage_op = (
        {
            "kind": "fill_form",
            "stage_only": True,
            "fields": fields_ops,
        }
        if fields_ops
        else None
    )
    commit_op = (
        {
            "kind": "submit",
            "target_ref": submit_el["node_ref"],
        }
        if submit_el is not None
        else None
    )
    action_label = (submit_el.get("label") if submit_el else None) or "Submit Form"
    title, body = format_domain_card(snapshot.get("url", ""), action_label, fields_preview)

    return {
        "llm_calls": 1,
        "model_id": MODEL_ID,
        "latency_ms": elapsed_ms,
        "raw_model_output": raw_output,
        "stage_operation": stage_op,
        "commit_operation": commit_op,
        "submit_target": submit_el,
        "card_title": title,
        "card_body": body,
        "fields_preview": fields_preview,
    }

