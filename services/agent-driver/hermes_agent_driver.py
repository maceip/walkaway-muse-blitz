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
import subprocess
import time
from pathlib import Path
from typing import Any

MLX_PYTHON = Path(
    os.environ.get(
        "HERMES_MLX_PYTHON",
        "/Users/mac/.local/share/uv/tools/mlx-lm/bin/python3",
    )
)
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

    fields_ops = []
    for field_name, val in field_values.items():
        if field_name not in by_key:
            continue
        el = by_key[field_name]
        tag = el.get("tag")
        input_type = el.get("input_type")
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
