//! OpenAI's tool factories, and the tools of a call as OpenAI's APIs take
//! them. Arguments use the Python wire snake_case keys.
use super::{
    capabilities::{Capabilities, dropped},
    dialect,
    params::invalid,
    profile::{Dialect, ResponsesDialect},
    protocol::Wire,
};
use crate::{
    Error,
    types::tools::{Tool, ToolConfig, ToolKind},
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

fn make(name: &str, args: Value) -> Result<Tool, Error> {
    let mut args = args
        .as_object()
        .cloned()
        .ok_or_else(|| invalid("OpenAI tool arguments must be an object"))?;
    args.retain(|_, v| !v.is_null());
    Tool::provider(
        name,
        ToolConfig {
            id: Some(format!("openai.{name}")),
            args,
        },
    )
    .map_err(|e| invalid(e.to_string()))
}
macro_rules! factory {($($name:ident),*)=>{$(
    pub fn $name(args:Value)->Result<Tool,Error> {make(stringify!($name),args)}
)*};}
factory!(
    web_search,
    web_search_preview,
    file_search,
    code_interpreter,
    image_generation,
    shell,
    mcp,
    tool_search
);
pub fn local_shell() -> Tool {
    Tool {
        kind: ToolKind::Provider,
        name: "local_shell".into(),
        spec: None,
        tool_config: Some(ToolConfig {
            id: Some("openai.local_shell".into()),
            args: Map::new(),
        }),
        require_approval: false,
    }
}
/// The computer use tool: the application runs its `computer_call`s and
/// answers each with a screenshot.
pub fn computer() -> Tool {
    Tool {
        name: "computer".into(),
        tool_config: Some(ToolConfig {
            id: Some("openai.computer".into()),
            args: Map::new(),
        }),
        ..local_shell()
    }
}
/// Programmatic tool calling: OpenAI runs code that calls the call's
/// function tools marked with `allowed_callers` `["programmatic"]`.
pub fn programmatic_tool_calling() -> Tool {
    Tool {
        name: "programmatic_tool_calling".into(),
        tool_config: Some(ToolConfig {
            id: Some("openai.programmatic_tool_calling".into()),
            args: Map::new(),
        }),
        ..local_shell()
    }
}
/// A custom tool named `name`, whose calls carry free text that the
/// application runs. `args` may hold a `description`, `async` and a
/// `format`: `{"type": "grammar", "syntax": "regex" | "lark",
/// "definition"}` or `{"type": "text"}` (`tool/custom.ts`).
pub fn custom(name: impl Into<String>, args: Value) -> Result<Tool, Error> {
    let args = args
        .as_object()
        .cloned()
        .ok_or_else(|| invalid("OpenAI tool arguments must be an object"))?;
    CustomArgs::parse(&args)?;
    Tool::provider(
        name,
        ToolConfig {
            id: Some("openai.custom".into()),
            args,
        },
    )
    .map_err(|e| invalid(e.to_string()))
}
pub fn apply_patch() -> Tool {
    Tool {
        name: "apply_patch".into(),
        tool_config: Some(ToolConfig {
            id: Some("openai.apply_patch".into()),
            args: Map::new(),
        }),
        ..local_shell()
    }
}
/// The arguments of a custom tool (`customArgsSchema`).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CustomArgs {
    description: Option<String>,
    #[serde(rename = "async")]
    is_async: Option<bool>,
    format: Option<CustomFormat>,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
enum CustomFormat {
    Grammar {
        syntax: GrammarSyntax,
        definition: String,
    },
    Text,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
enum GrammarSyntax {
    Regex,
    Lark,
}
impl CustomArgs {
    fn parse(args: &Map<String, Value>) -> Result<Self, Error> {
        serde_json::from_value(Value::Object(args.clone()))
            .map_err(|e| invalid(format!("invalid OpenAI custom tool arguments: {e}")))
    }
}

/// The OpenAI options a function tool's `tool_config` arguments hold, under
/// their wire names (`OpenAIToolOptions` of `openai-responses-prepare-tools.ts`).
/// Other arguments, such as another provider's, are left alone.
#[derive(Default, Deserialize)]
struct FunctionOptions {
    namespace: Option<Namespace>,
    defer_loading: Option<bool>,
    #[serde(rename = "async")]
    is_async: Option<bool>,
    output_schema: Option<Map<String, Value>>,
    allowed_callers: Option<Vec<Value>>,
    omit_from_request_tools: Option<bool>,
}
#[derive(Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Namespace {
    name: String,
    description: Option<String>,
}
impl FunctionOptions {
    fn of(tool: &Tool) -> Result<Self, Error> {
        let Some(config) = &tool.tool_config else {
            return Ok(Self::default());
        };
        serde_json::from_value(Value::Object(config.args.clone())).map_err(|e| {
            invalid(format!(
                "invalid OpenAI options of function tool {:?}: {e}",
                tool.name
            ))
        })
    }
    fn omitted(&self) -> bool {
        self.omit_from_request_tools == Some(true)
    }
}

/// The names of the function tools whose text results go JSON-quoted,
/// because they declare an `output_schema` (OAI-009).
pub(super) fn output_schema_tools(tools: &[Tool]) -> Result<Vec<String>, Error> {
    let mut names = vec![];
    for tool in tools.iter().filter(|t| t.kind == ToolKind::Function) {
        if FunctionOptions::of(tool)?.output_schema.is_some() {
            names.push(tool.name.clone());
        }
    }
    Ok(names)
}

/// `async` goes out on GPT-6 and later; elsewhere a true value is left out
/// and logged (`resolveAsyncToolOption`).
fn async_flag(value: Option<bool>, tool: &str, model: &str) -> Option<bool> {
    if value == Some(true) && !Capabilities::of(model).gpt6 {
        dropped(
            "async",
            model,
            &format!("async tool calling for {tool:?} is only supported by GPT-6 and later models"),
        );
        return None;
    }
    value
}

/// The OpenAI name of a provider tool: its id without `openai.`.
fn provider_name(tool: &Tool) -> Result<&str, Error> {
    let id = tool
        .tool_config
        .as_ref()
        .and_then(|config| config.id.as_deref())
        .ok_or_else(|| invalid("provider tool has no config"))?;
    id.strip_prefix("openai.")
        .ok_or_else(|| invalid("provider tool is not an OpenAI tool"))
}

/// A provider tool as OpenAI's Responses API takes it.
fn provider_item(tool: &Tool, model: &str) -> Result<Value, Error> {
    let name = provider_name(tool)?;
    let args = &tool
        .tool_config
        .as_ref()
        .expect("provider_name checked it")
        .args;
    match name {
        "local_shell" | "apply_patch" | "computer" | "programmatic_tool_calling" => {
            return Ok(json!({"type": name}));
        }
        "custom" => {
            let custom = CustomArgs::parse(args)?;
            let mut item = json!({"type": "custom", "name": tool.name});
            if let Some(description) = custom.description {
                item["description"] = json!(description);
            }
            if let Some(flag) = async_flag(custom.is_async, &tool.name, model) {
                item["async"] = json!(flag);
            }
            if let Some(format) = custom.format {
                item["format"] = serde_json::to_value(format)?;
            }
            return Ok(item);
        }
        "web_search" | "web_search_preview" | "file_search" | "code_interpreter"
        | "image_generation" | "shell" | "mcp" | "tool_search" => {}
        _ => {
            return Err(Error::UnsupportedOperation(format!(
                "unsupported OpenAI provider tool openai.{name}"
            )));
        }
    }
    let mut args = args.clone();
    if name == "file_search"
        && let Some(ranking) = args.remove("ranking").filter(|v| !v.is_null())
    {
        args.insert("ranking_options".into(), ranking);
    }
    if name == "code_interpreter" {
        args.entry("container")
            .or_insert_with(|| json!({"type":"auto"}));
    }
    // Python uses {"type": name, **args}, so an explicit type in args wins.
    args.entry("type").or_insert_with(|| json!(name));
    Ok(args.into())
}

/// A function tool as OpenAI's own Responses API takes it, and the
/// namespace it goes in (`prepareFunctionTool`).
fn responses_function(tool: &Tool, options: &FunctionOptions, model: &str) -> Result<Value, Error> {
    let spec = tool
        .spec
        .as_ref()
        .ok_or_else(|| invalid("function tool has no spec"))?;
    let mut item = json!({
        "type": "function",
        "name": tool.name,
        "description": spec.description.as_deref().unwrap_or(""),
        "parameters": super::schema::normalize(&serde_json::to_value(&spec.params)?, model)?,
        "strict": tool.is_strict(),
    });
    if let Some(flag) = async_flag(options.is_async, &tool.name, model) {
        item["async"] = json!(flag);
    }
    if let Some(defer) = options.defer_loading {
        item["defer_loading"] = json!(defer);
    }
    if let Some(callers) = &options.allowed_callers {
        item["allowed_callers"] = json!(callers);
    }
    if let Some(schema) = &options.output_schema {
        item["output_schema"] = super::schema::normalize(&Value::Object(schema.clone()), model)?;
    }
    Ok(item)
}

pub(super) fn convert(tools: &[Tool], wire: Wire, model: &str) -> Result<Vec<Value>, Error> {
    let openai = wire.responses_dialect() == Some(ResponsesDialect::OpenAI);
    let mut result = vec![];
    // The index in `result` of each namespace group, with its description.
    let mut namespaces: BTreeMap<String, (usize, Option<String>)> = BTreeMap::new();
    for tool in tools {
        tool.validate_shape().map_err(|e| invalid(e.to_string()))?;
        if tool.kind == ToolKind::Function {
            let spec = tool
                .spec
                .as_ref()
                .ok_or_else(|| invalid("function tool has no spec"))?;
            match wire {
                Wire::Responses(_) if openai => {
                    let options = FunctionOptions::of(tool)?;
                    if options.omitted() {
                        continue;
                    }
                    let item = responses_function(tool, &options, model)?;
                    let Some(namespace) = &options.namespace else {
                        result.push(item);
                        continue;
                    };
                    match namespaces.get(&namespace.name) {
                        Some((_, description)) if *description != namespace.description => {
                            return Err(invalid(format!(
                                "conflicting descriptions for OpenAI tool namespace {:?}",
                                namespace.name
                            )));
                        }
                        Some((index, _)) => {
                            if let Some(group) = result[*index]["tools"].as_array_mut() {
                                group.push(item);
                            }
                        }
                        None => {
                            namespaces.insert(
                                namespace.name.clone(),
                                (result.len(), namespace.description.clone()),
                            );
                            let mut group = json!({"type": "namespace", "name": namespace.name, "tools": [item]});
                            if let Some(description) = &namespace.description {
                                group["description"] = json!(description);
                            }
                            result.push(group);
                        }
                    }
                }
                Wire::Responses(_) => result.push(json!({
                    "type": "function",
                    "name": tool.name,
                    "description": spec.description.as_deref().unwrap_or(""),
                    "parameters": serde_json::to_value(&spec.params)?,
                })),
                Wire::Chat(profile) => {
                    let mut parameters = serde_json::to_value(&spec.params)?;
                    if profile.openai_own {
                        parameters = super::schema::normalize(&parameters, model)?;
                    }
                    let parameters = dialect::tool_parameters(profile, parameters)?;
                    let mut function = json!({"name":tool.name,"description":spec.description.as_deref().unwrap_or(""),"parameters":parameters});
                    // OpenAI and Mistral take strict function tools
                    // (HST-042); DeepSeek's go through `strict_tools`.
                    if (profile.openai_own || profile.dialect == Dialect::Mistral)
                        && tool.is_strict()
                    {
                        function["strict"] = json!(true);
                    }
                    result.push(json!({"type":"function","function":function}));
                }
            }
            continue;
        }
        match wire {
            Wire::Chat(_) => {
                return Err(Error::UnsupportedOperation(
                    "OpenAI built-in tools require the Responses API".into(),
                ));
            }
            Wire::Responses(profile) if !openai => {
                return Err(Error::UnsupportedOperation(format!(
                    "{} does not support provider tools",
                    profile.api.label
                )));
            }
            Wire::Responses(_) => result.push(provider_item(tool, model)?),
        }
    }
    Ok(result)
}

/// A named tool choice on OpenAI's own Responses API: `{"type": <tool>}` for
/// the OpenAI tools that take it, `{"type": "custom", "name"}` for a custom
/// tool and `{"type": "function", "name"}` otherwise. A tool left out of the
/// request cannot be forced.
pub(super) fn named_choice(name: &str, tools: &[Tool]) -> Result<Value, Error> {
    let tool = tools.iter().find(|tool| tool.name == name);
    if let Some(tool) = tool
        && tool.kind == ToolKind::Function
        && FunctionOptions::of(tool)?.omitted()
    {
        return Err(invalid(format!(
            "cannot force tool {name:?}, which is omitted from the request tools; use automatic tool choice for history-loaded tools"
        )));
    }
    let provider = match tool {
        Some(tool) if tool.kind == ToolKind::Provider => Some(provider_name(tool)?),
        _ => None,
    };
    Ok(match provider {
        Some(
            kind @ ("code_interpreter"
            | "file_search"
            | "image_generation"
            | "web_search_preview"
            | "web_search"
            | "mcp"
            | "apply_patch"
            | "computer"
            | "programmatic_tool_calling"),
        ) => json!({"type": kind}),
        Some("custom") => json!({"type": "custom", "name": name}),
        Some(kind) => json!({"type": "function", "name": kind}),
        None => json!({"type": "function", "name": name}),
    })
}

/// How a tool of the call can appear in `tool_choice.allowed_tools`.
#[derive(Clone, Debug, PartialEq)]
enum Allowed {
    Entry(Value),
    Unsupported(String),
}

/// A tool's entry in `allowed_tools`, and its OpenAI name when that differs
/// from its own (`toAllowedToolResolution`).
fn allowed_entry(tool: &Tool) -> Result<(Allowed, Option<String>), Error> {
    if tool.kind == ToolKind::Function {
        let options = FunctionOptions::of(tool)?;
        let unsupported = |reason: &str| Ok((Allowed::Unsupported(reason.into()), None));
        if options.omitted() {
            return unsupported(
                "tools omitted from the request tools array cannot be allow-listed",
            );
        }
        if options.namespace.is_some() {
            return unsupported(
                "tools inside an OpenAI tool namespace are not visible to tool_choice.allowed_tools",
            );
        }
        if options.defer_loading == Some(true) {
            return unsupported("deferred tools are not visible to tool_choice.allowed_tools");
        }
        return Ok((
            Allowed::Entry(json!({"type": "function", "name": tool.name})),
            None,
        ));
    }
    let kind = provider_name(tool)?;
    let alias = (kind != tool.name && kind != "custom").then(|| kind.to_owned());
    let entry = match kind {
        "custom" => Allowed::Entry(json!({"type": "custom", "name": tool.name})),
        "mcp" => Allowed::Entry(json!({
            "type": "mcp",
            "server_label": tool.tool_config.as_ref().and_then(|c| c.args.get("server_label")),
        })),
        "file_search"
        | "web_search"
        | "web_search_preview"
        | "image_generation"
        | "code_interpreter"
        | "computer"
        | "apply_patch"
        | "shell"
        | "local_shell"
        | "programmatic_tool_calling" => Allowed::Entry(json!({"type": kind})),
        _ => Allowed::Unsupported(format!(
            "OpenAI does not support {kind} tools in tool_choice.allowed_tools"
        )),
    };
    Ok((entry, alias))
}

/// The tool choice of the option `allowedTools`: each named tool of the call
/// as OpenAI lists it, by its name or by the OpenAI name of a provider tool,
/// and a name the call does not declare as a function. A tool that cannot be
/// listed is left out and logged; a list left empty fails
/// (`openai-responses-prepare-tools.ts`).
pub(super) fn allowed_tools(
    allowed: &super::options::AllowedTools,
    tools: &[Tool],
    model: &str,
) -> Result<Value, Error> {
    // By tool name, and by the OpenAI name of a provider tool named otherwise;
    // `None` marks an OpenAI name that several different tools share.
    let mut by_name = BTreeMap::new();
    let mut aliases: BTreeMap<String, Option<Allowed>> = BTreeMap::new();
    for tool in tools {
        let (resolution, alias) = allowed_entry(tool)?;
        by_name.insert(tool.name.clone(), resolution.clone());
        if let Some(alias) = alias {
            aliases
                .entry(alias)
                .and_modify(|known| {
                    if known.as_ref() != Some(&resolution) {
                        *known = None;
                    }
                })
                .or_insert(Some(resolution));
        }
    }
    let mut entries = vec![];
    let mut left_out = vec![];
    for name in &allowed.tool_names {
        let setting = format!("allowedTools entry {name:?}");
        let resolution = match (by_name.get(name), aliases.get(name)) {
            (Some(direct), _) => direct.clone(),
            (None, Some(Some(alias))) => alias.clone(),
            (None, Some(None)) => {
                dropped(
                    &setting,
                    model,
                    "several tools of the call share this OpenAI tool name",
                );
                left_out.push(name.as_str());
                continue;
            }
            (None, None) => Allowed::Entry(json!({"type": "function", "name": name})),
        };
        match resolution {
            Allowed::Entry(entry) => entries.push(entry),
            Allowed::Unsupported(reason) => {
                dropped(&setting, model, &reason);
                left_out.push(name.as_str());
            }
        }
    }
    if entries.is_empty() {
        return Err(invalid(format!(
            "allowedTools names only tools that cannot be allow-listed: {}",
            left_out.join(", ")
        )));
    }
    Ok(json!({
        "type": "allowed_tools",
        "mode": allowed.mode.as_deref().unwrap_or("auto"),
        "tools": entries,
    }))
}

/// What the reader and the history need to know of a call's tools: the
/// provider tools the application runs, rather than OpenAI (OAI-010):
/// `local_shell`, `shell` without a container environment, `apply_patch`,
/// `computer` and custom tools; and the function tools (OAI-011).
#[derive(Clone, Debug, Default)]
pub(super) struct CallTools {
    /// Each declared host-run tool's OpenAI kind, by its tool name.
    kinds: BTreeMap<String, &'static str>,
    /// Whether OpenAI runs the declared `shell` tool in a container.
    shell_in_container: bool,
    /// The names of the function tools.
    functions: std::collections::BTreeSet<String>,
}

/// The host-run tools of a call that declares none.
pub(super) static NO_CALL_TOOLS: CallTools = CallTools {
    kinds: BTreeMap::new(),
    shell_in_container: false,
    functions: std::collections::BTreeSet::new(),
};

impl CallTools {
    pub(super) fn of(tools: &[Tool]) -> Self {
        let mut this = Self {
            functions: tools
                .iter()
                .filter(|t| t.kind == ToolKind::Function)
                .map(|t| t.name.clone())
                .collect(),
            ..Self::default()
        };
        for tool in tools.iter().filter(|t| t.kind == ToolKind::Provider) {
            let Ok(kind) = provider_name(tool) else {
                continue;
            };
            let kind = match kind {
                "local_shell" => "local_shell",
                "apply_patch" => "apply_patch",
                "computer" => "computer",
                "custom" => "custom",
                "shell" => {
                    let environment = tool
                        .tool_config
                        .as_ref()
                        .and_then(|c| c.args.get("environment"))
                        .and_then(|e| e.get("type"))
                        .and_then(Value::as_str);
                    if matches!(environment, Some("container_auto" | "container_reference")) {
                        this.shell_in_container = true;
                        continue;
                    }
                    "shell"
                }
                _ => continue,
            };
            this.kinds.insert(tool.name.clone(), kind);
        }
        this
    }

    /// The tool name of a call of this OpenAI kind: the declared tool's, or
    /// the kind itself.
    pub(super) fn name(&self, kind: &str) -> String {
        self.kinds
            .iter()
            .find(|(_, k)| **k == kind)
            .map_or_else(|| kind.to_owned(), |(name, _)| name.clone())
    }

    /// The OpenAI kind of a declared host-run tool, by its tool name.
    pub(super) fn kind(&self, name: &str) -> Option<&'static str> {
        self.kinds.get(name).copied()
    }

    /// Whether the application runs `shell` calls.
    pub(super) fn shell_on_host(&self) -> bool {
        !self.shell_in_container
    }

    /// Whether the call declares a function tool of this name.
    pub(super) fn has_function(&self, name: &str) -> bool {
        self.functions.contains(name)
    }
}
