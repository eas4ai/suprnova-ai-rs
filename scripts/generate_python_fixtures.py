#!/usr/bin/env python3
"""Generate wire fixtures directly from the unmodified Python 0.8.0 source.

Run in an isolated environment with pydantic>=2.13. No provider SDKs are imported.
The source and destination paths are explicit arguments.
"""
import importlib.util
import json
from pathlib import Path
import sys
import types

source, destination = map(Path, sys.argv[1:])
root = source / "src" / "ai"
for name, path in [("ai", root), ("ai.types", root / "types")]:
    module = types.ModuleType(name)
    module.__path__ = [str(path)]
    sys.modules[name] = module

modules = {}
for name in ["media", "usage", "messages", "tools", "events"]:
    qualified = f"ai.types.{name}"
    spec = importlib.util.spec_from_file_location(qualified, root / "types" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[qualified] = module
    spec.loader.exec_module(module)
    modules[name] = module

m, e, t, u = (modules[name] for name in ["messages", "events", "tools", "usage"])
text = m.TextPart(id="text", text="hello")
reasoning = m.ReasoningPart(id="reason", text="thinking")
file = m.FilePart(id="file", data=b"\x89PNG\r\n", media_type="image/png", filename="image.png")
call = m.ToolCallPart(id="call", tool_call_id="tc", tool_name="weather", tool_args='{"city":"SF"}')
result = m.ToolResultPart(id="result", tool_call_id="tc", tool_name="weather", result={"temp": 20})
null_result = m.ToolResultPart(id="null-result", tool_call_id="tc", tool_name="weather", result="fallback", model_input=None)
special = m.ToolResultPart(id="special", tool_call_id="tc", tool_name="weather", result=m.ContentOutput(value=[text, file]), result_kind="special")
builtin = m.BuiltinToolCallPart(id="builtin", tool_call_id="bc", tool_name="search", tool_args="{}")
builtin_return = m.BuiltinToolReturnPart(id="builtin-return", tool_call_id="bc", tool_name="search", result=["answer"])
hook = m.HookPart(id="hook", hook_id="approve", hook_type="approval", status="pending", tool_call_id="tc")
assistant = m.Message(id="assistant", role="assistant", parts=[text, reasoning, call, builtin, builtin_return, file])
tool_message = m.Message(id="tool", role="tool", parts=[result, null_result, special])
messages = [
    m.Message(id="system", role="system", parts=[text]),
    m.Message(id="user", role="user", parts=[text, file]),
    assistant,
    tool_message,
    m.Message(id="internal", role="internal", parts=[hook]),
]
kwargs = dict(message=assistant, usage=u.Usage(input_tokens=7, output_tokens=3), provider_metadata={"key":"value"})
events = [
    e.StreamStart(), e.StreamEnd(finish_reason="tool_call", response_id="r1", response_model="actual", **kwargs),
    e.TextStart(block_id="t"), e.TextDelta(block_id="t", chunk="hello"), e.TextEnd(block_id="t"),
    e.ReasoningStart(block_id="r"), e.ReasoningDelta(block_id="r", chunk="think"), e.ReasoningEnd(block_id="r"),
    e.ToolStart(tool_call_id="tc", tool_name="weather"), e.ToolDelta(tool_call_id="tc", chunk="{}"), e.ToolEnd(tool_call_id="tc", tool_call=call),
    e.BuiltinToolStart(tool_call_id="bc", tool_name="search"), e.BuiltinToolDelta(tool_call_id="bc", chunk="{}"),
    e.BuiltinToolEnd(tool_call_id="bc", tool_call=builtin), e.BuiltinToolResult(tool_call_id="bc", result=builtin_return),
    e.FileEvent(block_id="f", media_type="image/png", data="aW1hZ2U=", filename="image.png"),
    e.ToolCallResult(message=tool_message, results=[result, null_result, special]),
    e.PartialToolCallResult(tool_call_id="tc", tool_name="weather", label={"nested":True}, value=[1,2]),
    e.Retry(), e.RunBlocked(hooks=(hook,)), e.HookEvent(message=messages[-1], hook=hook),
]
tools = [
    t.Tool(kind="function", name="weather", spec=t.ToolSpec(params={"type":"object","properties":{"city":{"type":"string"}},"required":["city"]})),
    t.Tool(kind="provider", name="search", tool_config=t.ToolConfig(id="provider.search", args={"max_uses":3})),
    t.Tool(kind="provider", name="mcp", tool_config=t.ToolConfig(id="openai.mcp", args={"headers":{"x_api_key":"fixture-value"},"server_label":"server"})),
]
fixture = {"source":"ai-python 0.8.0", "events":[x.model_dump(mode="json") for x in events],
           "messages":[x.model_dump(mode="json") for x in messages], "tools":[x.model_dump(mode="json") for x in tools],
           "usage":[u.Usage().model_dump(mode="json"), u.Usage(input_tokens=7,output_tokens=3,reasoning_tokens=1,raw={"provider":7}).model_dump(mode="json")]}
destination.write_text(json.dumps(fixture, indent=2, sort_keys=True)+"\n")
print(f"wrote {len(events)} events, {len(messages)} messages, {len(tools)} tools, 2 usage values")
