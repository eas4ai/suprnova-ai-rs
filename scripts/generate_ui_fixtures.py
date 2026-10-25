#!/usr/bin/env python3
"""Capture UI compatibility fixtures while running the unmodified Python tests.

Usage: python scripts/generate_ui_fixtures.py UPSTREAM DESTINATION
Requires Python >=3.12, upstream runtime dependencies, pytest and pytest-asyncio.
The Rust tests read the resulting JSON and do not import Python.
"""

import base64
import json
from pathlib import Path
import random
import sys

source, destination = map(Path, sys.argv[1:])
sys.path.insert(0, str(source / "src"))

import pytest
from ai.ui import ai_sdk
from ai.ui.ai_sdk import outbound_stream
from ai.ui.ai_sdk.tool_utils import normalize_tool_args
from ai.types import messages as core_messages

# Seed the upstream context, leaving its ID algorithm unchanged.
core_messages._id_random.set(random.Random(0))

fixtures = {"source": "ai-python 0.8.0 tests/ui", "inbound": [], "outbound": [],
            "stream": [], "serialization": [], "tool_arguments": []}


def dump(model):
    return model.model_dump(mode="json", by_alias=True)


original_inbound = ai_sdk.to_messages


def inbound(messages, **kwargs):
    wire = [dump(m) for m in messages]
    # Tool annotation validators are ported in ui_inbound_messages.rs. A JSON
    # fixture cannot retain their Python runtime classes.
    if kwargs.get("tools") is not None:
        return original_inbound(messages, **kwargs)
    try:
        result, approvals = original_inbound(messages, **kwargs)
    except ValueError:
        fixtures["inbound"].append({"input": wire, "error": True})
        raise
    fixtures["inbound"].append({"input": wire, "messages": [dump(m) for m in result],
                                "approvals": [a._asdict() for a in approvals]})
    return result, approvals


original_outbound = ai_sdk.to_ui_messages


def outbound(messages):
    result = original_outbound(messages)
    fixtures["outbound"].append({"input": [dump(m) for m in messages],
                                 "messages": [dump(m) for m in result]})
    return result


original_stream = ai_sdk.to_stream
original_serialize = outbound_stream.serialize_event


async def stream(events):
    inputs, outputs = [], []
    replayable = True

    async def capture():
        nonlocal replayable
        async for event in events:
            if getattr(event, "aggregator_factory", None) is not None:
                replayable = False
            wire = event.model_dump(mode="json")
            # replay is runtime-only in both ports.
            if event.replay:
                wire["replay"] = True
            if isinstance(getattr(event, "data", None), bytes):
                # Event JSON cannot distinguish file bytes from UTF-8 text.
                wire["runtime_file_bytes"] = base64.b64encode(event.data).decode("ascii")
            inputs.append(wire)
            yield event

    async for event in original_stream(capture()):
        outputs.append(json.loads(original_serialize(event)))
        yield event
    if replayable:
        fixtures["stream"].append({"input": inputs, "events": outputs})


def serialize(event):
    result = original_serialize(event)
    fixtures["serialization"].append({"event": json.loads(result), "json": result})
    return result


ai_sdk.to_messages = inbound
ai_sdk.to_ui_messages = outbound
ai_sdk.to_stream = outbound_stream.to_stream = stream
outbound_stream.serialize_event = serialize

status = pytest.main([str(source / "tests" / "ui"), "-q", "-p", "no:cacheprovider"])
if status:
    raise SystemExit(status)

# Add non-ASCII/control/astral strings to the exact encoder fixtures.
from ai.ui.ai_sdk import ui_events

serialize(ui_events.UITextDeltaEvent(id="unicode", delta='café 👋\n\t"\\\u2028'))
serialize(ui_events.UIDataEvent(data_type="unicode", data={"é": [None, False, "🎉"]}))
numbers = [1e-8, 1e-5, 1e-4, 1e16, 1e20, -0.0, 0.00009999999999999999,
           5e-324, 1.7976931348623157e308, 1.2345678901234567]
serialize(ui_events.UIDataEvent(data_type="numbers", data=numbers))
for value in ({"z": 1, "a": {"b": "é", "a": False}}, [1, None, "👋"], None, "already serialized", numbers):
    fixtures["tool_arguments"].append({"input": value, "args": normalize_tool_args(value)})

# Deduplicate calls made repeatedly by the upstream persistence tests.
for name in ("inbound", "outbound", "stream", "serialization"):
    unique = {json.dumps(case, sort_keys=True): case for case in fixtures[name]}
    fixtures[name] = list(unique.values())
destination.write_text(json.dumps(fixtures, indent=2, ensure_ascii=True) + "\n")
print({name: len(cases) for name, cases in fixtures.items() if isinstance(cases, list)})
