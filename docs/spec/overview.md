# suprnova-ai: overview

suprnova-ai is a Rust library crate. It ports the AI SDK for Python 0.8.0
(`https://github.com/vercel-labs/ai-python`) to Rust. It covers model
references, streaming and generation, model operations beyond chat,
OpenAI-compatible, Anthropic-compatible and Google provider adapters and
the hosted providers that use them, Google Vertex AI, a tool-calling agent
loop, and an adapter for Vercel AI SDK v6 UI messages. Its repository is
`https://github.com/eas4ai/suprnova-ai-rs`.

## The problem

Rust applications, suprchat first, need the behavior of the Python SDK: the
same event and message shapes and the same tool loop. The port keeps that
behavior, so Rust code and Python or JavaScript clients can exchange the
same streams. It departs from Python only where a requirement says so.

## What it is not

- It is not a Vercel product, and it is not the upstream Python package.
  Upstream module names such as `ai.types` belong to the Python package.
- It does not port the Vercel AI Gateway protocol, OIDC, telemetry spans,
  the MCP client, durable workflow integrations or the factories that read
  the environment (`README.md`, `docs/PORT.md`).
- It never reads process environment variables. The application passes
  every credential and setting.
- It stores no persistent state.

## Spec map

| File | Prefix | Covers |
|---|---|---|
| package.md | PKG | package, library and repository names, the upstream credit, the origin remote and the check script |
| models.md | MOD | model ids, model snapshots, retries, timeouts and dropping a call, the time limits of the reqwest transport, and the scripted test provider |
| agents.md | AGT | step checks that steer or stop an agent run, why a run stopped, unknown tool calls, tool concurrency, run usage, stopping a run and the history of an interrupted run |
| operations.md | OPS | the `ops` front ends: embeddings, images, video jobs, speech, transcription and its streaming form, speech translation, files, skills, batches, realtime sessions, server connections and WebRTC sessions, the WebSocket transport, and evaluation on a language model |
| google.md | GGL | the Gemini API: language models, buffered calls, embeddings, image generation, speech, transcription, speech translation, Veo video, the files API, batches, the Live API, the Interactions API and evaluation |
| openai.md | OAI | OpenAI's own APIs: the Responses, Chat Completions and Completions language models with their options, history, tools and streams, embeddings, images, speech, transcription, speech translation, files, skills, batches, realtime and Live sessions, decisions and prompt caching |
| anthropic.md | ANT | Anthropic's Messages API: the language model with its options, model capabilities, structured output, tools, history, stream, usage and prompt caching, the hosts that share it, files, skills, batches and evaluation |
| vertex.md | VTX | Google Vertex AI: endpoints and authentication, Gemini on Vertex, embeddings, images, video, Chirp and Gemini speech, Cloud Speech-to-Text and Gemini transcription, the Interactions API, and the Anthropic, open-model and xAI partner providers |
| hosts.md | HST | the hosted chat providers that use the Chat Completions, Responses or Messages protocol: Cerebras, DeepInfra, DeepSeek, Fireworks, Mistral, Moonshot AI, Together AI, Z.ai, xAI, Hugging Face and MiniMax, with the other model kinds their packages ship and Mistral's Conversations API; tool calls without an index in any Chat Completions stream; errors and terminal events in any Responses stream |

## Not yet specified

These areas have code and tests but no requirements yet, apart from the
parts that the files in the spec map cover. `docs/recon.md` describes what
each one does today. A later commitment specifies an area before it changes
it.

| Area | Code |
|---|---|
| Model references, streaming and generation | `src/models/` |
| Provider traits, registry and configuration | `src/providers/base.rs` |
| OpenAI-compatible adapter | `src/providers/openai/` |
| Anthropic-compatible adapter | `src/providers/anthropic/` |
| Events, messages, builders, media and tools | `src/types/` |
| HTTP transport and media downloads | `src/transport/`, `src/models/files.rs` |
| Agent loop | `src/agents/` |
| UI adapter for Vercel AI SDK v6 | `src/ui/ai_sdk/` |
| Errors | `src/errors.rs` |
