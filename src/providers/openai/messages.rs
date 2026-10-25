use super::{
    dialect,
    params::invalid,
    profile::{self, Dialect, ResponsesDialect},
    protocol::Wire,
};
use crate::{
    Error,
    types::{
        Message, MessageRole, Part,
        media::{self, MediaDataRef},
        messages::*,
    },
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn media_ref(data: &FileData) -> MediaDataRef<'_> {
    match data {
        FileData::Text(s) => MediaDataRef::Text(s),
        FileData::Bytes(b) => MediaDataRef::Bytes(b),
    }
}
fn dialect_of(wire: Wire) -> Dialect {
    match wire {
        Wire::Chat(profile) => profile.dialect,
        Wire::Responses(_) => Dialect::OpenAI,
    }
}
/// A user file, the `index`-th part of its message, as the API takes it.
fn file(part: &FilePart, index: usize, wire: Wire, history: &History<'_>) -> Result<Value, Error> {
    let mt = if part.media_type == "image/*" {
        "image/jpeg"
    } else {
        &part.media_type
    };
    match wire {
        Wire::Responses(profile) => {
            return match profile.dialect {
                ResponsesDialect::Xai => xai_file(part, mt),
                ResponsesDialect::HuggingFace => huggingface_file(part, mt),
                ResponsesDialect::OpenAI => {
                    openai_file(part, mt, index, true, history.pass_through_files)
                }
            };
        }
        Wire::Chat(profile) if profile.openai_own => {
            return openai_file(part, mt, index, false, false);
        }
        Wire::Chat(_) => {}
    }
    match dialect_of(wire) {
        Dialect::OpenAI | Dialect::VertexXai => {}
        Dialect::DeepSeek => return super::deepseek::image(part, mt, true),
        Dialect::Moonshot => return moonshot_file(part, mt),
        Dialect::Mistral => return mistral_file(part, mt),
        // The TypeScript provider downloads every file URL for MaaS.
        Dialect::VertexMaas if fetch_url(part).is_some() => {
            return Err(invalid(
                "Google Vertex AI MaaS does not fetch file URLs; supply the file bytes or build the provider with a download transport",
            ));
        }
        Dialect::VertexMaas => {}
    }
    let data = media_ref(&part.data);
    if mt.starts_with("image/") {
        let url = media::data_to_data_url(data, mt);
        return Ok(json!({"type":"image_url","image_url":{"url":url}}));
    }
    let url = part
        .data
        .as_text()
        .filter(|s| media::is_downloadable_url(s));
    if mt == "application/pdf" {
        if url.is_some() {
            return Err(chat_url_error("PDF"));
        }
        let filename = part.filename.as_deref().unwrap_or("document.pdf");
        let data = media::data_to_data_url(data, mt);
        return Ok(json!({"type":"file","file":{"filename":filename,"file_data":data}}));
    }
    if mt.starts_with("audio/") {
        if url.is_some() {
            return Err(chat_url_error("audio"));
        }
        return Ok(
            json!({"type":"input_audio","input_audio":{"format":mt.split_once('/').map_or(mt,|(_,v)|v),"data":media::data_to_base64(data)}}),
        );
    }
    if mt.starts_with("text/") {
        let text = text_file(part)?;
        return Ok(json!({"type":"text","text":text}));
    }
    Err(invalid(format!("Unsupported media type for OpenAI: {mt}")))
}
fn chat_url_error(kind: &str) -> Error {
    invalid(format!(
        "chat {kind} URLs require a download transport; supply file bytes or build the provider with with_download_transport"
    ))
}
/// A user file as OpenAI's own Responses (`responses`) or Chat Completions
/// API takes it (OAI-007). Text data that starts with `file-` names a file
/// uploaded to OpenAI. On Responses a `text/*` file goes as text unless
/// `passThroughUnsupportedFiles` sends it as a file, as Shawn ruled for
/// OAI-007 on 2026-10-23; Chat Completions takes no text file.
fn openai_file(
    part: &FilePart,
    mt: &str,
    index: usize,
    responses: bool,
    pass_through: bool,
) -> Result<Value, Error> {
    let text = part.data.as_text();
    let file_id = text.filter(|s| s.starts_with("file-"));
    let url = text.filter(|s| media::is_downloadable_url(s));
    let data = media_ref(&part.data);
    let detail = part
        .provider_metadata
        .as_ref()
        .and_then(|md| md.get("openai")?.get("imageDetail"))
        .filter(|detail| !detail.is_null());
    if !responses && let Some(id) = file_id {
        return Ok(json!({"type": "file", "file": {"file_id": id}}));
    }
    if mt.starts_with("image/") {
        let mut image = match (responses, file_id) {
            (true, Some(id)) => json!({"type": "input_image", "file_id": id}),
            (true, None) => {
                json!({"type": "input_image", "image_url": media::data_to_data_url(data, mt)})
            }
            (false, _) => {
                json!({"type": "image_url", "image_url": {"url": media::data_to_data_url(data, mt)}})
            }
        };
        if let Some(detail) = detail {
            let target = if responses {
                &mut image
            } else {
                &mut image["image_url"]
            };
            target["detail"] = detail.clone();
        }
        return Ok(image);
    }
    if let Some(id) = file_id {
        return Ok(json!({"type": "input_file", "file_id": id}));
    }
    let filename = |default: String| part.filename.clone().unwrap_or(default);
    if mt == "application/pdf" {
        if let Some(url) = url {
            if responses {
                return Ok(json!({"type": "input_file", "file_url": url}));
            }
            return Err(chat_url_error("PDF"));
        }
        let filename = filename(format!("part-{index}.pdf"));
        let data = media::data_to_data_url(data, mt);
        return Ok(if responses {
            json!({"type": "input_file", "filename": filename, "file_data": data})
        } else {
            json!({"type": "file", "file": {"filename": filename, "file_data": data}})
        });
    }
    if !responses && mt.starts_with("audio/") {
        if url.is_some() {
            return Err(chat_url_error("audio"));
        }
        let format = match mt {
            "audio/wav" => "wav",
            "audio/mp3" | "audio/mpeg" => "mp3",
            _ => {
                return Err(invalid(format!(
                    "OpenAI chat completions takes WAV and MP3 audio, not {mt}"
                )));
            }
        };
        return Ok(
            json!({"type": "input_audio", "input_audio": {"data": media::data_to_base64(data), "format": format}}),
        );
    }
    if responses && pass_through {
        return Ok(match url {
            Some(url) => json!({"type": "input_file", "file_url": url}),
            None => json!({
                "type": "input_file",
                "filename": filename(format!("part-{index}")),
                "file_data": media::data_to_data_url(data, mt),
            }),
        });
    }
    if responses && mt.starts_with("text/") {
        return Ok(json!({"type": "input_text", "text": text_file(part)?}));
    }
    Err(invalid(format!(
        "OpenAI does not take {mt} files{}",
        if responses {
            " unless the option passThroughUnsupportedFiles is true"
        } else {
            ""
        }
    )))
}
fn text_file(part: &FilePart) -> Result<String, Error> {
    Ok(match &part.data {
        FileData::Bytes(b) => {
            String::from_utf8(b.clone()).map_err(|_| invalid("OpenAI text input is not UTF-8"))?
        }
        FileData::Text(s) if media::is_url(s) => s.clone(),
        FileData::Text(s) => String::from_utf8(
            STANDARD
                .decode(s)
                .map_err(|_| invalid("OpenAI text input is not base64"))?,
        )
        .map_err(|_| invalid("OpenAI text input is not UTF-8"))?,
    })
}
/// The media URL of a file, which a host that cannot fetch URLs cannot take.
fn fetch_url(part: &FilePart) -> Option<&str> {
    part.data
        .as_text()
        .filter(|s| media::is_downloadable_url(s))
}
/// Moonshot AI takes images, videos and text, inline: it does not fetch
/// URLs, so a provider downloads them first when it has a download transport
/// (`convert-to-moonshotai-chat-messages.ts`).
fn moonshot_file(part: &FilePart, mt: &str) -> Result<Value, Error> {
    const IMAGES: [&str; 7] = [
        "image/jpeg",
        "image/png",
        "image/gif",
        "image/webp",
        "image/bmp",
        "image/heic",
        "image/heif",
    ];
    const VIDEOS: [&str; 9] = [
        "video/mp4",
        "video/mpeg",
        "video/mov",
        "video/avi",
        "video/x-flv",
        "video/mpg",
        "video/webm",
        "video/wmv",
        "video/3gpp",
    ];
    if fetch_url(part).is_some() {
        return Err(invalid(
            "Moonshot AI does not fetch media URLs; supply the file bytes or build the provider with a download transport",
        ));
    }
    if mt.starts_with("text/") {
        return Ok(json!({"type":"text","text":text_file(part)?}));
    }
    let kind = if IMAGES.contains(&mt) {
        "image_url"
    } else if VIDEOS.contains(&mt) {
        "video_url"
    } else {
        return Err(invalid(format!("Moonshot AI does not take {mt} files")));
    };
    let url = media::data_to_data_url(media_ref(&part.data), mt);
    Ok(json!({"type":kind,kind:{"url":url}}))
}
/// xAI takes images, and other files only as URLs
/// (`convert-to-xai-responses-input.ts`).
fn xai_file(part: &FilePart, mt: &str) -> Result<Value, Error> {
    if mt.starts_with("image/") {
        let url = media::data_to_data_url(media_ref(&part.data), mt);
        return Ok(json!({"type":"input_image","image_url":url}));
    }
    match fetch_url(part) {
        Some(url) => Ok(json!({"type":"input_file","file_url":url})),
        None => Err(invalid(format!(
            "xAI takes {mt} files only as URLs; supply the file as an http or https URL"
        ))),
    }
}
/// Hugging Face takes images only
/// (`convert-to-huggingface-responses-messages.ts`).
fn huggingface_file(part: &FilePart, mt: &str) -> Result<Value, Error> {
    if !mt.starts_with("image/") {
        return Err(invalid(format!("Hugging Face does not take {mt} files")));
    }
    let url = media::data_to_data_url(media_ref(&part.data), mt);
    Ok(json!({"type":"input_image","image_url":url}))
}
/// Mistral takes images and PDFs (`convert-to-mistral-chat-messages.ts`).
pub(super) fn mistral_file(part: &FilePart, mt: &str) -> Result<Value, Error> {
    let url = media::data_to_data_url(media_ref(&part.data), mt);
    if mt.starts_with("image/") {
        return Ok(json!({"type":"image_url","image_url":url}));
    }
    if mt == "application/pdf" {
        if fetch_url(part).is_some_and(|url| !url.starts_with("https://")) {
            return Err(invalid(
                "Mistral fetches PDF URLs over https only; supply the file bytes",
            ));
        }
        return Ok(json!({"type":"document_url","document_url":url}));
    }
    Err(invalid(format!("Mistral does not take {mt} files")))
}
pub(super) fn tool_result(value: &ToolResultValue, wire: Wire) -> Result<Value, Error> {
    let responses = matches!(wire, Wire::Responses(_));
    // Only OpenAI's Responses API takes files other than images in a result.
    let result_files = wire.responses_dialect() == Some(ResponsesDialect::OpenAI);
    match value {
        ToolResultValue::Json(Value::Null) => Ok(json!("")),
        ToolResultValue::Json(Value::String(s)) => Ok(json!(s)),
        // Moonshot AI and Mistral take only text; DeepSeek takes parts only
        // with an image.
        ToolResultValue::Special(SpecialToolResult::Content(content))
            if match dialect_of(wire) {
                Dialect::OpenAI | Dialect::VertexMaas | Dialect::VertexXai => false,
                Dialect::DeepSeek => !content.value.iter().any(
                    |p| matches!(p, ContentPart::File(f) if f.media_type.starts_with("image/")),
                ),
                Dialect::Moonshot | Dialect::Mistral => true,
            } =>
        {
            Ok(json!(serde_json::to_string(&content.value)?))
        }
        ToolResultValue::Special(SpecialToolResult::Content(content)) => {
            let mut parts = vec![];
            for p in &content.value {
                parts.push(match p {
                    ContentPart::Text(t) => {
                        json!({"type":if responses {"input_text"}else {"text"},"text":t.text})
                    }
                    // DeepSeek takes a result's images as a user's, without
                    // `fileData` (HST-035).
                    ContentPart::File(f)
                        if dialect_of(wire) == Dialect::DeepSeek
                            && f.media_type.starts_with("image/") =>
                    {
                        let mt = if f.media_type == "image/*" {
                            "image/jpeg"
                        } else {
                            &f.media_type
                        };
                        super::deepseek::image(f, mt, false)?
                    }
                    ContentPart::File(f) => {
                        let url = media::data_to_data_url(media_ref(&f.data), &f.media_type);
                        if f.media_type.starts_with("image/") {
                            if responses {
                                json!({"type":"input_image","image_url":url})
                            } else {
                                json!({"type":"image_url","image_url":{"url":url}})
                            }
                        } else if result_files {
                            let mut value = json!({"type":"input_file","file_data":url});
                            if let Some(name) = &f.filename {
                                value["filename"] = json!(name);
                            }
                            value
                        } else {
                            json!({"type":if responses {"input_text"} else {"text"},"text":format!("[file: {}]",f.media_type)})
                        }
                    }
                });
            }
            Ok(parts.into())
        }
        _ => Ok(json!(serde_json::to_string(value)?)),
    }
}
/// The MCP approval requests of a history by request id: those of the
/// requests' built-in calls and of the hooks that name one (OAI-008).
fn mcp_requests(messages: &[Message]) -> BTreeSet<String> {
    let mut requests = BTreeSet::new();
    for part in messages.iter().flat_map(|m| &m.parts) {
        let request = match part {
            Part::BuiltinToolCall(_) => metadata(part, "openai")
                .and_then(|md| md.get("raw_item"))
                .filter(|raw| raw["type"] == "mcp_approval_request")
                .and_then(|raw| {
                    raw.get("approval_request_id")
                        .and_then(Value::as_str)
                        .filter(|id| !id.is_empty())
                        .or_else(|| raw["id"].as_str())
                }),
            Part::Hook(hook) => hook
                .metadata
                .get("openai")
                .and_then(|md| md.get("approval_request_id"))
                .and_then(Value::as_str),
            _ => None,
        };
        if let Some(request) = request {
            requests.insert(request.to_owned());
        }
    }
    requests
}

/// The history without internal messages and hooks, with each unanswered
/// tool call given an error result. `approvals` keeps the hooks of MCP
/// approval requests, which OpenAI's Responses API answers (OAI-008): those
/// whose `openai` metadata names the request, and those named by a request
/// id, such as the hook in an internal message that the UI adapter makes
/// of an approval.
fn repair(
    messages: Vec<Message>,
    approvals: bool,
    requests: &BTreeSet<String>,
) -> Result<Vec<Message>, Error> {
    let answers = |hook: &HookPart| {
        approvals && (hook.metadata.contains_key("openai") || requests.contains(&hook.hook_id))
    };
    let mut cleaned = vec![];
    for mut msg in messages {
        if msg.role == MessageRole::Internal {
            msg.parts
                .retain(|p| matches!(p, Part::Hook(hook) if answers(hook)));
            if !msg.parts.is_empty() {
                cleaned.push(msg);
            }
            continue;
        }
        msg.parts.retain(|p| match p {
            Part::Hook(hook) => answers(hook),
            _ => true,
        });
        if msg.parts.is_empty() {
            continue;
        }
        for p in &mut msg.parts {
            if let Part::ToolCall(c) = p
                && serde_json::from_str::<Value>(&c.tool_args).is_err()
            {
                c.tool_args = "{}".into();
            }
        }
        cleaned.push(msg);
    }
    let mut calls = BTreeSet::new();
    let mut results = BTreeSet::new();
    let mut pending = BTreeSet::new();
    for msg in &cleaned {
        if matches!(msg.role, MessageRole::User | MessageRole::Assistant) {
            pending.clear();
        }
        for p in &msg.parts {
            match p {
                Part::ToolCall(c) if msg.role == MessageRole::Assistant => {
                    if !calls.insert(c.tool_call_id.clone()) {
                        return Err(invalid("duplicate tool-call id in history"));
                    }
                    pending.insert(c.tool_call_id.clone());
                }
                Part::ToolResult(r) if msg.role == MessageRole::Tool => {
                    if !results.insert(r.tool_call_id.clone()) {
                        return Err(invalid("duplicate tool-result id in history"));
                    }
                    if !pending.remove(&r.tool_call_id) {
                        return Err(invalid("orphaned tool result in history"));
                    }
                }
                _ => {}
            }
        }
    }
    let mut output = vec![];
    let mut unanswered: Vec<ToolCallPart> = vec![];
    for msg in cleaned {
        if matches!(msg.role, MessageRole::User | MessageRole::Assistant) {
            flush_pending(&mut output, &mut unanswered);
        }
        if msg.role == MessageRole::Assistant {
            for c in msg.tool_calls() {
                if !results.contains(&c.tool_call_id) {
                    unanswered.push(c.clone());
                }
            }
        }
        output.push(msg);
    }
    flush_pending(&mut output, &mut unanswered);
    Ok(output)
}
fn flush_pending(output: &mut Vec<Message>, pending: &mut Vec<ToolCallPart>) {
    if !pending.is_empty() {
        output.push(Message::new(
            MessageRole::Tool,
            pending
                .drain(..)
                .map(|c| {
                    Part::ToolResult(ToolResultPart::error(
                        c.tool_call_id,
                        c.tool_name,
                        json!("Tool result not available"),
                    ))
                })
                .collect(),
        ));
    }
}
/// A part's metadata under the host's key, such as `openai` or `xai`.
fn metadata<'a>(part: &'a Part, key: &str) -> Option<&'a Value> {
    (match part {
        Part::Text(p) => p.provider_metadata.as_ref(),
        Part::Reasoning(p) => p.provider_metadata.as_ref(),
        Part::ToolCall(p) => p.provider_metadata.as_ref(),
        Part::BuiltinToolCall(p) => p.provider_metadata.as_ref(),
        Part::BuiltinToolReturn(p) => p.provider_metadata.as_ref(),
        Part::File(p) => p.provider_metadata.as_ref(),
        Part::ToolResult(p) => p.provider_metadata.as_ref(),
        _ => None,
    })
    .and_then(|m| m.get(key))
}
/// A part's item id and encrypted reasoning, from its metadata.
fn item_id(md: Option<&Value>) -> Option<&str> {
    md.and_then(|m| m.get("item_id").or_else(|| m.get("itemId")))
        .and_then(Value::as_str)
}
fn encrypted_reasoning(md: Option<&Value>) -> Option<&Value> {
    md.and_then(|m| {
        m.get("reasoning_encrypted_content")
            .or_else(|| m.get("reasoningEncryptedContent"))
    })
    .filter(|value| !value.is_null())
}
/// An xAI assistant turn: an item for each text part, tool call and
/// reasoning part that has an item id or encrypted content, with the xAI
/// item ids and no references to stored items
/// (`convert-to-xai-responses-input.ts`).
fn xai_assistant(msg: &Message, key: &str, out: &mut Vec<Value>) {
    for part in &msg.parts {
        let md = metadata(part, key);
        let id = item_id(md);
        match part {
            Part::Text(t) => {
                let mut item = json!({"role":"assistant","content":t.text});
                if let Some(id) = id {
                    item["id"] = json!(id);
                }
                out.push(item);
            }
            Part::ToolCall(c) => out.push(json!({
                "type":"function_call",
                "id":id.unwrap_or(&c.tool_call_id),
                "call_id":c.tool_call_id,
                "name":c.tool_name,
                "arguments":c.tool_args,
                "status":"completed",
            })),
            Part::Reasoning(r) => {
                let encrypted = encrypted_reasoning(md);
                if id.is_none() && encrypted.is_none() {
                    continue;
                }
                let summary = if r.text.is_empty() {
                    json!([])
                } else {
                    json!([{"type":"summary_text","text":r.text}])
                };
                let mut item = json!({"type":"reasoning","id":id.unwrap_or(""),"summary":summary,"status":"completed"});
                if let Some(encrypted) = encrypted {
                    item["encrypted_content"] = encrypted.clone();
                }
                out.push(item);
            }
            _ => {}
        }
    }
}
/// A Hugging Face assistant turn: each text and reasoning part as an
/// assistant message of output text, and tool calls as `function_call`
/// items (`convert-to-huggingface-responses-messages.ts`, which drops the
/// tool calls; HST-024 keeps them).
fn huggingface_assistant(msg: &Message, out: &mut Vec<Value>) {
    for part in &msg.parts {
        match part {
            Part::Text(TextPart { text, .. }) | Part::Reasoning(ReasoningPart { text, .. }) => {
                out.push(json!({"role":"assistant","content":[{"type":"output_text","text":text}]}));
            }
            Part::ToolCall(c) => out.push(json!({"type":"function_call","call_id":c.tool_call_id,"name":c.tool_name,"arguments":c.tool_args})),
            _ => {}
        }
    }
}
/// An assistant turn under OpenAI's Responses rules
/// (`convert-to-openai-responses-input.ts`). A tool call goes out in full
/// (OAI-001). With stored responses, reasoning and text with an item id go
/// out as one reference each, except a reasoning model's text that no
/// reasoning item precedes, whose stored message OpenAI cannot take without
/// that item. Without them, text goes out without its id and reasoning only
/// with its encrypted content (OAI-002). Text that goes out in full is one
/// assistant message per part, with the `phase` of its item.
/// An assistant turn as OpenAI's Responses API takes it in history
/// (`convert-to-openai-responses-input.ts`, OAI-001, OAI-002 and OAI-008).
fn openai_assistant(
    msg: &Message,
    history: &History<'_>,
    parallel: &mut Parallel,
    out: &mut Vec<Value>,
) {
    let references = history.references;
    let reasoning_model = history.reasoning;
    let mut sent = BTreeSet::new();
    // Reasoning items rebuilt from their parts, which take later summaries.
    let mut built = BTreeSet::new();
    let mut after_reasoning = false;
    for p in &msg.parts {
        let md = metadata(p, "openai");
        let id = item_id(md);
        // A conversation already holds every item OpenAI stored for it.
        if history.conversation && id.is_some() {
            continue;
        }
        if let Part::ToolCall(c) = p
            && let Some(kind) = history.host_tools.kind(&c.tool_name)
        {
            // A previous response already holds the calls it made.
            if history.previous_response && references && id.is_some() {
                continue;
            }
            match id {
                Some(id) if references => {
                    if sent.insert(id.to_owned()) {
                        out.push(json!({"type": "item_reference", "id": id}));
                    }
                }
                _ => out.push(host_call(kind, c, id, md)),
            }
            continue;
        }
        if let Part::ToolCall(c) = p {
            match parallel.call(c) {
                Some(Some(wrapper)) if !history.conversation => out.push(wrapper),
                Some(_) => {}
                None => out.push(function_call(c, md)),
            }
            continue;
        }
        if let Part::Hook(hook) = p {
            approval_response(hook, history, parallel, out);
            continue;
        }
        if let Part::BuiltinToolCall(_) = p
            && id.is_some_and(|id| parallel.requests.contains(id))
        {
            continue;
        }
        let raw = md
            .and_then(|m| m.get("raw_item"))
            .filter(|raw| raw.is_object());
        match p {
            Part::Text(t) => {
                if references
                    && let Some(id) = id
                    && (!reasoning_model || after_reasoning)
                {
                    if sent.insert(id.to_owned()) {
                        out.push(json!({"type":"item_reference","id":id}));
                    }
                    continue;
                }
                let mut message = json!({"role":"assistant","content":t.text});
                let phase = md.and_then(|m| {
                    [m.get("phase"), raw.and_then(|raw| raw.get("phase"))]
                        .into_iter()
                        .flatten()
                        .find(|phase| phase.is_string())
                });
                if let Some(phase) = phase {
                    message["phase"] = phase.clone();
                }
                out.push(message);
            }
            Part::Reasoning(r) => {
                after_reasoning |= id.is_some();
                // A previous response already holds its reasoning.
                if history.previous_response && id.is_some() {
                    continue;
                }
                if references && let Some(id) = id {
                    if sent.insert(id.to_owned()) {
                        out.push(json!({"type":"item_reference","id":id}));
                    }
                    continue;
                }
                let encrypted = encrypted_reasoning(md).or_else(|| {
                    raw.and_then(|raw| raw.get("encrypted_content"))
                        .filter(|value| !value.is_null())
                });
                // Without stored responses OpenAI needs the encrypted
                // content; reasoning without it is left out.
                let Some(encrypted) = encrypted else {
                    continue;
                };
                if let Some(id) = id
                    && !sent.insert(id.to_owned())
                {
                    // A raw item already holds every summary of its item; a
                    // rebuilt one takes this part's summary.
                    if built.contains(id)
                        && !r.text.is_empty()
                        && let Some(item) = out.iter_mut().rev().find(|item| item["id"] == id)
                        && let Some(summary) = item["summary"].as_array_mut()
                    {
                        summary.push(json!({"type":"summary_text","text":r.text}));
                    }
                    continue;
                }
                if let Some(raw) = raw {
                    let mut raw = raw.clone();
                    raw["encrypted_content"] = encrypted.clone();
                    out.push(raw);
                    continue;
                }
                let summary = if r.text.is_empty() {
                    json!([])
                } else {
                    json!([{"type":"summary_text","text":r.text}])
                };
                let mut item =
                    json!({"type":"reasoning","summary":summary,"encrypted_content":encrypted});
                if let Some(id) = id {
                    item["id"] = json!(id);
                    built.insert(id.to_owned());
                }
                out.push(item);
            }
            // A conversation holds the results of OpenAI's own tools.
            Part::BuiltinToolReturn(_) if history.conversation => {}
            _ => {
                if references && let Some(id) = id {
                    if sent.insert(id.to_owned()) {
                        out.push(json!({"type":"item_reference","id":id}));
                    }
                    continue;
                }
                if let Part::BuiltinToolCall(call) = p
                    && call.tool_name == "compaction"
                    && let Some(id) = id
                {
                    let encrypted = md.and_then(|m| m.get("encrypted_content"));
                    out.push(
                        json!({"type": "compaction", "id": id, "encrypted_content": encrypted}),
                    );
                    continue;
                }
                // A call, its result and an image file carry the same raw
                // item; OpenAI takes each item once.
                if let Some(raw) = raw
                    && id.is_none_or(|id| sent.insert(id.to_owned()))
                {
                    out.push(raw.clone());
                }
            }
        }
    }
}

/// A call the application runs, as its own Responses item (OAI-010). Its
/// input is the reader's: `{"action"}` for shells, `{"call_id",
/// "operation"}` for `apply_patch`, `{"actions", "pending_safety_checks",
/// "status"}` for `computer`, and a JSON string for a custom tool.
fn host_call(kind: &str, call: &ToolCallPart, id: Option<&str>, md: Option<&Value>) -> Value {
    let input: Value = serde_json::from_str(&call.tool_args).unwrap_or(Value::Null);
    let mut item = match kind {
        "local_shell" => {
            json!({"type": "local_shell_call", "call_id": call.tool_call_id, "action": input["action"]})
        }
        "shell" => {
            json!({"type": "shell_call", "call_id": call.tool_call_id, "status": "completed", "action": input["action"]})
        }
        "apply_patch" => json!({
            "type": "apply_patch_call",
            "call_id": input.get("call_id").filter(|v| v.is_string()).cloned().unwrap_or_else(|| json!(call.tool_call_id)),
            "status": "completed",
            "operation": input["operation"],
        }),
        "computer" => json!({
            "type": "computer_call",
            "call_id": call.tool_call_id,
            "status": input.get("status").filter(|v| v.is_string()).cloned().unwrap_or_else(|| json!("completed")),
            "actions": input.get("actions").cloned().unwrap_or_else(|| json!([])),
            "pending_safety_checks": input.get("pending_safety_checks").cloned().unwrap_or_else(|| json!([])),
        }),
        _ => {
            let mut item = json!({
                "type": "custom_tool_call",
                "call_id": call.tool_call_id,
                "name": call.tool_name,
                "input": input.as_str().map_or_else(|| call.tool_args.clone(), str::to_owned),
            });
            if let Some(flag) = own_or_raw(md, "async", Value::is_boolean) {
                item["async"] = flag.clone();
            }
            item
        }
    };
    if let Some(id) = id {
        item["id"] = json!(id);
    }
    item
}

/// The result of a call the application runs, as its output item
/// (OAI-010). `local_shell` and custom tools take text, or an object's
/// `output`; `shell` takes `{"output": [...], "max_output_length"}`;
/// `apply_patch` `{"status", "output"}`; and `computer` `{"output":
/// {"type": "computer_screenshot", ...}, "acknowledged_safety_checks"}`.
fn host_result(
    kind: &str,
    call_id: &str,
    value: &ToolResultValue,
    breakpoint: Option<Value>,
) -> Result<Value, Error> {
    let value = match value {
        ToolResultValue::Json(value) => value.clone(),
        other => serde_json::to_value(other)?,
    };
    let text = |value: &Value| match value.get("output").unwrap_or(value) {
        Value::String(text) => json!(text),
        other => json!(other.to_string()),
    };
    let bad = |what: &str| invalid(format!("a {kind} tool result must be {what}"));
    Ok(match kind {
        "local_shell" => {
            json!({"type": "local_shell_call_output", "call_id": call_id, "output": text(&value)})
        }
        "shell" => {
            let output = value
                .get("output")
                .filter(|o| o.is_array())
                .ok_or_else(|| bad("an object with an output array"))?;
            let mut item =
                json!({"type": "shell_call_output", "call_id": call_id, "output": output});
            if let Some(max) = value.get("max_output_length").filter(|v| !v.is_null()) {
                item["max_output_length"] = max.clone();
            }
            item
        }
        "apply_patch" => {
            let status = value
                .get("status")
                .filter(|s| s == &"completed" || s == &"failed")
                .ok_or_else(|| bad("an object with a status of completed or failed"))?;
            let mut item =
                json!({"type": "apply_patch_call_output", "call_id": call_id, "status": status});
            if let Some(output) = value.get("output").filter(|v| !v.is_null()) {
                item["output"] = output.clone();
            }
            item
        }
        "computer" => {
            let output = value
                .get("output")
                .filter(|o| o["type"] == "computer_screenshot")
                .ok_or_else(|| bad("an object with a computer_screenshot output"))?;
            let mut item =
                json!({"type": "computer_call_output", "call_id": call_id, "output": output});
            if let Some(checks) = value
                .get("acknowledged_safety_checks")
                .filter(|v| !v.is_null())
            {
                item["acknowledged_safety_checks"] = checks.clone();
            }
            item
        }
        // A custom tool's text keeps its prompt cache breakpoint (OAI-026).
        _ => {
            let output = match breakpoint {
                Some(breakpoint) => {
                    json!([{"type": "input_text", "text": text(&value), "prompt_cache_breakpoint": breakpoint}])
                }
                None => text(&value),
            };
            json!({"type": "custom_tool_call_output", "call_id": call_id, "output": output})
        }
    })
}

/// A prompt cache breakpoint, `{"mode": "explicit", "ttl"}`, from a part's
/// `openai` metadata `promptCacheBreakpoint` (OAI-026).
fn breakpoint(part: &Part) -> Result<Option<Value>, Error> {
    check_breakpoint(metadata(part, "openai").and_then(|md| md.get("promptCacheBreakpoint")))
}

/// A system message's breakpoint: its own metadata's, or its first text part's.
fn system_breakpoint(msg: &Message) -> Result<Option<Value>, Error> {
    let own = msg
        .provider_metadata
        .as_ref()
        .and_then(|md| md.get("openai"))
        .and_then(|md| md.get("promptCacheBreakpoint"));
    match own {
        Some(_) => check_breakpoint(own),
        None => match msg.parts.iter().find(|p| matches!(p, Part::Text(_))) {
            Some(part) => breakpoint(part),
            None => Ok(None),
        },
    }
}

fn check_breakpoint(value: Option<&Value>) -> Result<Option<Value>, Error> {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let valid = value["mode"] == "explicit"
        && value
            .get("ttl")
            .is_none_or(|ttl| ttl == "30m" || ttl == "12h")
        && value
            .as_object()
            .is_some_and(|o| o.keys().all(|k| k == "mode" || k == "ttl"));
    if !valid {
        return Err(invalid(
            "openai promptCacheBreakpoint must be {\"mode\": \"explicit\"} with an optional ttl of 30m or 12h",
        ));
    }
    Ok(Some(value.clone()))
}

/// The `additional_tools` item a tool result's `additionalTools` metadata
/// asks for: function definitions OpenAI loads after the result
/// (`openaiResponsesToolResultOptionsSchema`).
fn additional_tools(tools: &Value) -> Result<Value, Error> {
    let bad = || {
        invalid(
            "openai additionalTools must be function definitions with a type, name and parameters, and an optional description and strict",
        )
    };
    let list = tools.as_array().ok_or_else(bad)?;
    for tool in list {
        let tool = tool.as_object().ok_or_else(bad)?;
        let valid = tool.get("type").is_some_and(|t| t == "function")
            && tool
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| !name.is_empty())
            && tool.get("parameters").is_some_and(Value::is_object)
            && tool.get("description").is_none_or(Value::is_string)
            && tool.get("strict").is_none_or(Value::is_boolean)
            && tool.keys().all(|key| {
                ["type", "name", "description", "parameters", "strict"].contains(&key.as_str())
            });
        if !valid {
            return Err(bad());
        }
    }
    Ok(json!({"type": "additional_tools", "role": "developer", "tools": list}))
}

/// A resolved `ToolApproval` hook of an MCP approval request, which its
/// `openai` metadata or its hook id names: an `mcp_approval_response`, once
/// per request, after a reference to the stored request. The request's own
/// call goes back as nothing, as TypeScript gives it no item id.
fn approval_response(
    hook: &HookPart,
    history: &History<'_>,
    parallel: &mut Parallel,
    out: &mut Vec<Value>,
) {
    let named = hook
        .metadata
        .get("openai")
        .and_then(|md| md.get("approval_request_id"))
        .and_then(Value::as_str);
    let Some(request) = named.or_else(|| {
        parallel
            .requests
            .contains(&hook.hook_id)
            .then_some(hook.hook_id.as_str())
    }) else {
        return;
    };
    if hook.hook_type != "ToolApproval"
        || hook.status != HookStatus::Resolved
        || !parallel.approvals.insert(request.to_owned())
    {
        return;
    }
    let approve = hook
        .resolution
        .as_ref()
        .and_then(|r| r.get("granted"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if history.references && !history.conversation && !history.previous_response {
        out.push(json!({"type": "item_reference", "id": request}));
    }
    out.push(json!({
        "type": "mcp_approval_response",
        "approval_request_id": request,
        "approve": approve,
    }));
}

/// The `function_call` named `parallel` that OpenAI answered with, which
/// the reader expands into one call per tool use (OAI-011). With a
/// conversation or a previous response, OpenAI holds the wrapper, so a group
/// whose calls all have results goes back as the one wrapper call and one
/// output (OAI-008). Also the MCP approvals already answered.
#[derive(Default)]
struct Parallel {
    /// Each expanded call's wrapper id, for the complete groups.
    member: BTreeMap<String, String>,
    /// Each complete group: the wrapper call and its results in order.
    groups: BTreeMap<String, (Value, Vec<String>)>,
    /// The results of the expanded calls of complete groups.
    results: BTreeMap<String, ToolResultPart>,
    emitted_calls: BTreeSet<String>,
    emitted_results: BTreeSet<String>,
    /// The MCP approval requests of the history, and those answered.
    requests: BTreeSet<String>,
    approvals: BTreeSet<String>,
}

impl Parallel {
    fn collect(messages: &[Message], history: &History<'_>) -> Self {
        let mut this = Self::default();
        if !history.conversation && !history.previous_response {
            return this;
        }
        // By wrapper id: its metadata, and each use's call id by index.
        let mut found: BTreeMap<String, (Value, BTreeMap<u64, String>, bool)> = BTreeMap::new();
        let results: BTreeSet<&str> = messages
            .iter()
            .flat_map(|m| &m.parts)
            .filter_map(|p| match p {
                Part::ToolResult(r) => Some(r.tool_call_id.as_str()),
                _ => None,
            })
            .collect();
        for part in messages.iter().flat_map(|m| &m.parts) {
            let Part::ToolCall(call) = part else {
                continue;
            };
            let Some(group) = metadata(part, "openai").and_then(|m| m.get("parallel_tool_call"))
            else {
                continue;
            };
            let (Some(wrapper), Some(index)) = (
                group.get("tool_call_id").and_then(Value::as_str),
                group.get("index").and_then(Value::as_u64),
            ) else {
                continue;
            };
            let mut shared = group.clone();
            shared.as_object_mut().map(|g| g.remove("index"));
            let entry = found
                .entry(wrapper.to_owned())
                .or_insert_with(|| (shared.clone(), BTreeMap::new(), false));
            if entry.0 != shared || entry.1.contains_key(&index) {
                entry.2 = true;
            }
            entry.1.insert(index, call.tool_call_id.clone());
        }
        for (wrapper, (group, uses, invalid)) in found {
            let count = group.get("count").and_then(Value::as_u64);
            let complete = !invalid
                && count == Some(uses.len() as u64)
                && uses.keys().copied().eq(0..uses.len() as u64)
                && uses.values().all(|id| results.contains(id.as_str()));
            if !complete {
                continue;
            }
            let call = json!({
                "type": "function_call",
                "call_id": wrapper,
                "name": group.get("tool_name"),
                "arguments": group.get("input"),
            });
            let ids: Vec<String> = uses.into_values().collect();
            for id in &ids {
                this.member.insert(id.clone(), wrapper.clone());
            }
            this.groups.insert(wrapper, (call, ids));
        }
        for part in messages.iter().flat_map(|m| &m.parts) {
            if let Part::ToolResult(r) = part
                && this.member.contains_key(&r.tool_call_id)
            {
                this.results.insert(r.tool_call_id.clone(), r.clone());
            }
        }
        this
    }

    /// For an expanded call of a complete group, its wrapper call the first
    /// time and `None` after; `None` outside such a group.
    fn call(&mut self, call: &ToolCallPart) -> Option<Option<Value>> {
        let wrapper = self.member.get(&call.tool_call_id)?;
        let first = self.emitted_calls.insert(wrapper.clone());
        Some(first.then(|| self.groups[wrapper].0.clone()))
    }
}
/// A tool call the application runs, as OpenAI's Responses API takes it in
/// history: its `call_id`, name and arguments, with the namespace its item
/// recorded (`convert-to-openai-responses-input.ts`).
fn function_call(call: &ToolCallPart, md: Option<&Value>) -> Value {
    let mut item = json!({"type":"function_call","call_id":call.tool_call_id,"name":call.tool_name,"arguments":call.tool_args});
    if let Some(namespace) = own_or_raw(md, "namespace", Value::is_string) {
        item["namespace"] = namespace.clone();
    }
    if let Some(flag) = own_or_raw(md, "async", Value::is_boolean) {
        item["async"] = flag.clone();
    }
    if let Some(caller) = caller(md) {
        item["caller"] = caller;
    }
    item
}
/// The part's own value of `key`, as TypeScript keeps it, then its raw
/// item's, as the crate's reader keeps it: the first one `keep` accepts.
fn own_or_raw<'a>(md: Option<&'a Value>, key: &str, keep: fn(&Value) -> bool) -> Option<&'a Value> {
    let md = md?;
    [md.get(key), md.get("raw_item").and_then(|raw| raw.get(key))]
        .into_iter()
        .flatten()
        .find(|value| keep(value))
}
/// The caller of a call or its result in OpenAI's wire form. TypeScript
/// keeps a program's id as `callerId`, OpenAI's items as `caller_id`.
fn caller(md: Option<&Value>) -> Option<Value> {
    let caller = own_or_raw(md, "caller", Value::is_object)?;
    Some(match caller.get("callerId") {
        Some(id) if caller["type"] == "program" => json!({"type": "program", "caller_id": id}),
        _ => caller.clone(),
    })
}
/// The callers of the history's calls, by call id: a program's call and its
/// output both name the program (OAI-011).
fn callers(messages: &[Message]) -> BTreeMap<String, Value> {
    messages
        .iter()
        .flat_map(|m| &m.parts)
        .filter_map(|part| match part {
            Part::ToolCall(call) => {
                Some((call.tool_call_id.clone(), caller(metadata(part, "openai"))?))
            }
            _ => None,
        })
        .collect()
}
/// What the call decides about how its history goes out.
#[derive(Clone, Copy, Debug)]
pub(super) struct History<'a> {
    pub model: &'a str,
    /// Items OpenAI stored may go out as references (OAI-002).
    pub references: bool,
    /// The model is a reasoning model (OAI-005).
    pub reasoning: bool,
    /// The role system messages go out with, or `None` to leave them out.
    pub system_role: Option<&'static str>,
    /// Why a reasoning effort update cannot apply, when it cannot.
    pub update_blocked: Option<&'a str>,
    /// `passThroughUnsupportedFiles`: send other files as `input_file`.
    pub pass_through_files: bool,
    /// The call continues a conversation (option `conversation`).
    pub conversation: bool,
    /// The call continues a previous response (`previousResponseId`).
    pub previous_response: bool,
    /// The tools whose text results go JSON-quoted, because they declare
    /// an `output_schema` (OAI-009).
    pub quoted_results: &'a [String],
    /// The provider tools the application runs (OAI-010).
    pub host_tools: &'a super::tools::CallTools,
    /// The call goes to a DeepSeek base URL ending in `/beta`, which takes
    /// an assistant prefix (HST-034).
    pub deepseek_beta: bool,
}

impl<'a> History<'a> {
    /// The history of a server other than OpenAI's own Responses API.
    pub(super) fn plain(model: &'a str, references: bool) -> Self {
        Self {
            model,
            references,
            reasoning: profile::reasoning_model(model),
            system_role: Some("system"),
            update_blocked: None,
            pass_through_files: false,
            conversation: false,
            previous_response: false,
            quoted_results: &[],
            host_tools: &super::tools::NO_CALL_TOOLS,
            deepseek_beta: false,
        }
    }
}

/// A system message that only sets a reasoning effort: the
/// `configuration_update` item it goes out as, at its place (OAI-005).
fn effort_update(msg: &Message, history: &History<'_>) -> Result<Option<Value>, Error> {
    let Some(effort) = msg
        .provider_metadata
        .as_ref()
        .and_then(|md| md.get("openai"))
        .and_then(|md| md.get("reasoningEffortUpdate"))
    else {
        return Ok(None);
    };
    let effort = effort
        .as_str()
        .filter(|effort| super::options::EFFORT_UPDATES.contains(effort))
        .ok_or_else(|| {
            invalid(format!(
                "message-level reasoningEffortUpdate must be one of {}, not {effort}",
                super::options::EFFORT_UPDATES.join(", ")
            ))
        })?;
    if !msg.text().is_empty() {
        return Err(invalid(
            "message-level reasoningEffortUpdate requires empty system message content",
        ));
    }
    if let Some(reason) = history.update_blocked {
        return Err(invalid(format!("message-level {reason}")));
    }
    Ok(Some(
        json!({"type": "configuration_update", "reasoning": {"effort": effort}}),
    ))
}

pub(super) fn convert(
    messages: Vec<Message>,
    wire: Wire,
    history: History<'_>,
) -> Result<Vec<Value>, Error> {
    let model = history.model;
    let openai_responses = wire.responses_dialect() == Some(ResponsesDialect::OpenAI);
    // OpenAI's own APIs, which take prompt cache breakpoints (OAI-026).
    let openai_own = openai_responses || matches!(wire, Wire::Chat(profile) if profile.openai_own);
    let responses = matches!(wire, Wire::Responses(_));
    let reasoning_field = match wire {
        Wire::Chat(profile) => profile.reasoning_field,
        Wire::Responses(_) => "reasoning",
    };
    let dialect = dialect_of(wire);
    let requests = mcp_requests(&messages);
    let callers = callers(&messages);
    let messages = repair(messages, openai_responses, &requests)?;
    let mut parallel = Parallel::collect(&messages, &history);
    parallel.requests = requests;
    let last = messages.len().saturating_sub(1);
    let last_user = messages.iter().rposition(|m| m.role == MessageRole::User);
    let mut out = vec![];
    for (index, msg) in messages.into_iter().enumerate() {
        // DeepSeek names a message and prefixes the last assistant one from
        // the message's `deepseek` metadata (HST-034).
        let deepseek = match dialect {
            Dialect::DeepSeek => Some(super::deepseek::MessageOptions::parse(&msg)?),
            _ => None,
        };
        match msg.role {
            MessageRole::System => {
                if openai_responses && let Some(update) = effort_update(&msg, &history)? {
                    out.push(update);
                    continue;
                }
                let text = msg.text();
                if let Some(role) = history.system_role
                    && (!responses || !text.is_empty())
                {
                    let breakpoint = if openai_own {
                        system_breakpoint(&msg)?
                    } else {
                        None
                    };
                    let content = match breakpoint {
                        Some(breakpoint) => json!([{
                            "type": if responses { "input_text" } else { "text" },
                            "text": text,
                            "prompt_cache_breakpoint": breakpoint,
                        }]),
                        None => json!(text),
                    };
                    let mut entry = json!({"role":role,"content":content});
                    if let Some(deepseek) = &deepseek {
                        deepseek.name(&mut entry);
                    }
                    out.push(entry);
                }
            }
            MessageRole::User => {
                let breakpoints = if openai_own {
                    msg.parts
                        .iter()
                        .map(breakpoint)
                        .collect::<Result<Vec<_>, Error>>()?
                } else {
                    vec![None; msg.parts.len()]
                };
                if !responses
                    && !msg.parts.iter().any(|p| matches!(p, Part::File(_)))
                    && breakpoints.iter().all(Option::is_none)
                {
                    let mut entry = json!({"role":"user","content":msg.text()});
                    if let Some(deepseek) = &deepseek {
                        deepseek.name(&mut entry);
                    }
                    out.push(entry);
                    continue;
                }
                let mut content = vec![];
                for ((index, p), breakpoint) in msg.parts.into_iter().enumerate().zip(breakpoints) {
                    let mut item = match p {
                        Part::Text(t) => {
                            json!({"type":if responses {"input_text"}else {"text"},"text":t.text})
                        }
                        Part::File(f) => file(&f, index, wire, &history)?,
                        _ => continue,
                    };
                    if let Some(breakpoint) = breakpoint {
                        item["prompt_cache_breakpoint"] = breakpoint;
                    }
                    content.push(item);
                }
                let mut entry = json!({"role":"user","content":content});
                if let Some(deepseek) = &deepseek {
                    deepseek.name(&mut entry);
                }
                out.push(entry);
            }
            MessageRole::Assistant if !responses => {
                let mut entry = json!({"role":"assistant"});
                let text = msg.text();
                let reasoning = msg.reasoning();
                if !text.is_empty() {
                    entry["content"] = json!(text);
                }
                // On OpenAI's own API a turn whose text has a prompt cache
                // breakpoint goes as its text parts (OAI-026).
                if openai_own {
                    let mut parts = vec![];
                    let mut marked = false;
                    for p in &msg.parts {
                        if let Part::Text(t) = p {
                            let mut part = json!({"type": "text", "text": t.text});
                            if let Some(mark) = breakpoint(p)? {
                                part["prompt_cache_breakpoint"] = mark;
                                marked = true;
                            }
                            parts.push(part);
                        }
                    }
                    if marked {
                        entry["content"] = Value::Array(parts);
                    }
                }
                match dialect {
                    // V4 needs the field on every assistant turn; earlier
                    // models get no reasoning from before the last user turn.
                    Dialect::DeepSeek if !dialect::deepseek_legacy(model) => {
                        entry[reasoning_field] = json!(reasoning);
                    }
                    Dialect::DeepSeek if last_user.is_some_and(|user| index < user) => {}
                    // Mistral sends reasoning as thinking chunks of the content,
                    // in the turn's order, and always sends `content`.
                    Dialect::Mistral => {
                        entry["content"] = if reasoning.is_empty() {
                            json!(text)
                        } else {
                            mistral_content(&msg)
                        };
                        if index == last {
                            entry["prefix"] = json!(true);
                        }
                    }
                    _ if !reasoning.is_empty() => {
                        entry[reasoning_field] = json!(reasoning);
                    }
                    _ => {}
                }
                let mut calls = vec![];
                for p in &msg.parts {
                    match p {
                    Part::ToolCall(c)=>calls.push(json!({"id":c.tool_call_id,"type":"function","function":{"name":c.tool_name,"arguments":c.tool_args}})),
                    Part::BuiltinToolCall(_)|Part::BuiltinToolReturn(_)=>return Err(Error::UnsupportedOperation(format!("{} chat completions does not support BuiltinTool history", wire.api().label))),_=>{}
                }
                }
                if !calls.is_empty() {
                    if dialect == Dialect::Moonshot && text.is_empty() {
                        entry["content"] = Value::Null;
                    }
                    entry["tool_calls"] = calls.into();
                }
                if let Some(deepseek) = &deepseek {
                    deepseek.name(&mut entry);
                    deepseek.prefix(&mut entry, index == last, history.deepseek_beta)?;
                }
                out.push(entry);
            }
            MessageRole::Assistant if wire.responses_dialect() == Some(ResponsesDialect::Xai) => {
                if let Wire::Responses(profile) = wire {
                    xai_assistant(&msg, profile.metadata_key, &mut out);
                }
            }
            MessageRole::Assistant
                if wire.responses_dialect() == Some(ResponsesDialect::HuggingFace) =>
            {
                huggingface_assistant(&msg, &mut out);
            }
            MessageRole::Assistant => openai_assistant(&msg, &history, &mut parallel, &mut out),
            MessageRole::Tool => {
                for p in &msg.parts {
                    if let Part::Hook(hook) = p {
                        approval_response(hook, &history, &mut parallel, &mut out);
                        continue;
                    }
                    if let Part::ToolResult(r) = p
                        && openai_responses
                        && let Some(kind) = history.host_tools.kind(&r.tool_name)
                    {
                        out.push(host_result(
                            kind,
                            &r.tool_call_id,
                            r.get_model_input(),
                            breakpoint(p)?,
                        )?);
                        continue;
                    }
                    if let Part::ToolResult(r) = p
                        && openai_responses
                        && let Some(wrapper) = parallel.member.get(&r.tool_call_id).cloned()
                    {
                        if parallel.emitted_results.insert(wrapper.clone()) {
                            // One output of the results joined by newlines,
                            // as text parts when one has a breakpoint (OAI-026).
                            let mut outputs = vec![];
                            for id in &parallel.groups[&wrapper].1 {
                                let result = &parallel.results[id];
                                let text = match tool_result(result.get_model_input(), wire)? {
                                    Value::String(text) => text,
                                    other => other.to_string(),
                                };
                                let mark = breakpoint(&Part::ToolResult(result.clone()))?;
                                outputs.push((text, mark));
                            }
                            let output = if outputs.iter().any(|(_, mark)| mark.is_some()) {
                                let parts =
                                    outputs.into_iter().enumerate().map(|(i, (text, mark))| {
                                        let text = if i == 0 { text } else { format!("\n{text}") };
                                        let mut part = json!({"type": "input_text", "text": text});
                                        if let Some(mark) = mark {
                                            part["prompt_cache_breakpoint"] = mark;
                                        }
                                        part
                                    });
                                Value::Array(parts.collect())
                            } else {
                                let texts: Vec<String> =
                                    outputs.into_iter().map(|(text, _)| text).collect();
                                json!(texts.join("\n"))
                            };
                            out.push(json!({"type": "function_call_output", "call_id": wrapper, "output": output}));
                        }
                        continue;
                    }
                    if let Part::ToolResult(r) = p {
                        let mut value = match r.get_model_input() {
                            ToolResultValue::Json(Value::String(text))
                                if history.quoted_results.contains(&r.tool_name) =>
                            {
                                json!(serde_json::to_string(text)?)
                            }
                            input => tool_result(input, wire)?,
                        };
                        if openai_own
                            && let Some(breakpoint) = breakpoint(p)?
                            && let Value::String(text) = &value
                        {
                            value = json!([{
                                "type": if responses { "input_text" } else { "text" },
                                "text": text,
                                "prompt_cache_breakpoint": breakpoint,
                            }]);
                        }
                        let mut entry = if responses {
                            json!({"type":"function_call_output","call_id":r.tool_call_id,"output":value})
                        } else {
                            json!({"role":"tool","tool_call_id":r.tool_call_id,"content":value})
                        };
                        if dialect == Dialect::Mistral {
                            entry["name"] = json!(r.tool_name);
                        }
                        if openai_responses
                            && let Some(caller) = caller(metadata(p, "openai"))
                                .or_else(|| callers.get(&r.tool_call_id).cloned())
                        {
                            entry["caller"] = caller;
                        }
                        out.push(entry);
                        if openai_responses
                            && let Some(tools) =
                                metadata(p, "openai").and_then(|m| m.get("additionalTools"))
                        {
                            out.push(additional_tools(tools)?);
                        }
                    }
                }
            }
            // The hooks `repair` keeps: answers to MCP approval requests.
            MessageRole::Internal => {
                for p in &msg.parts {
                    if let Part::Hook(hook) = p {
                        approval_response(hook, &history, &mut parallel, &mut out);
                    }
                }
            }
        }
    }
    if openai_responses {
        // OpenAI rejects an item it is given twice, by reference or in full.
        let (mut referenced, mut given) = (BTreeSet::new(), BTreeSet::new());
        out.retain(|item| {
            let Some(id) = item["id"].as_str() else {
                return true;
            };
            if item["type"] == "item_reference" {
                referenced.insert(id.to_owned())
            } else {
                given.insert(id.to_owned())
            }
        });
    }
    Ok(out)
}

/// A Mistral assistant turn's text and reasoning as content chunks, in order.
fn mistral_content(msg: &Message) -> Value {
    let mut content = vec![];
    for part in &msg.parts {
        match part {
            Part::Text(t) if !t.text.is_empty() => {
                content.push(json!({"type":"text","text":t.text}));
            }
            Part::Reasoning(r) if !r.text.is_empty() => content.push(
                json!({"type":"thinking","thinking":[{"type":"text","text":r.text}],"closed":true}),
            ),
            _ => {}
        }
    }
    content.into()
}
/// Whether the host needs a file of this media type inline, so that a media
/// URL is downloaded first when the protocol has a download transport.
fn downloads(wire: Wire, media_type: &str) -> bool {
    // A Responses server fetches the URLs it takes, and refuses the others.
    if matches!(wire, Wire::Responses(_)) {
        return false;
    }
    match dialect_of(wire) {
        Dialect::OpenAI => media_type.starts_with("audio/") || media_type == "application/pdf",
        Dialect::Moonshot => ["image/", "video/", "text/"]
            .iter()
            .any(|kind| media_type.starts_with(kind)),
        Dialect::DeepSeek | Dialect::Mistral => false,
        Dialect::VertexMaas => true,
        Dialect::VertexXai => !media_type.starts_with("image/"),
    }
}
/// The URL of a user file that the protocol downloads before the request.
fn download_url(file: &FilePart, wire: Wire) -> Option<&str> {
    file.data
        .as_text()
        .filter(|s| downloads(wire, &file.media_type) && media::is_downloadable_url(s))
}
/// The messages with each file the protocol would download given inline as
/// empty bytes, to check the request before any download; `None` when there
/// is no such file.
pub(super) fn without_downloads(messages: &[Message], wire: Wire) -> Option<Vec<Message>> {
    let mut found = false;
    let mut messages = messages.to_vec();
    for msg in messages.iter_mut().filter(|m| m.role == MessageRole::User) {
        for part in &mut msg.parts {
            if let Part::File(file) = part
                && download_url(file, wire).is_some()
            {
                file.data = FileData::Bytes(Vec::new());
                found = true;
            }
        }
    }
    found.then_some(messages)
}
pub(super) async fn download_files(
    mut messages: Vec<Message>,
    wire: Wire,
    transport: Option<&dyn crate::models::files::DownloadTransport>,
) -> Result<Vec<Message>, Error> {
    let Some(transport) = transport else {
        return Ok(messages);
    };
    for msg in &mut messages {
        if msg.role != MessageRole::User {
            continue;
        }
        for part in &mut msg.parts {
            let Part::File(file) = part else {
                continue;
            };
            let Some(url) = download_url(file, wire) else {
                continue;
            };
            let (bytes, _) = crate::models::files::download(
                transport,
                url,
                crate::models::files::DEFAULT_MAX_BYTES,
            )
            .await
            .map_err(|e| {
                crate::errors::ProviderError::new(
                    crate::errors::ProviderErrorKind::Response,
                    format!("{} input download failed", wire.api().label),
                )
                .with_source(e)
            })?;
            file.data = FileData::Bytes(bytes.to_vec());
        }
    }
    Ok(messages)
}
