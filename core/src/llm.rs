//! LLM Client - Multi-provider streaming with tool use
//!
//! Rust does the heavy lifting: HTTP, streaming, parsing.
//! Scheme defines: provider, model, system prompt, context, handlers.

use futures::StreamExt;
use crate::log::log;
use llm::{
    builder::{FunctionBuilder, LLMBackend, LLMBuilder, ParamBuilder},
    chat::{ChatMessage, StreamChunk},
    FunctionCall, LLMProvider, ToolCall,
};
use std::sync::mpsc::Sender;

/// Message in a conversation
#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,      // "user", "assistant", "tool_result"
    pub content: String,
    pub tool_use_id: Option<String>,  // For tool_result messages
    pub tool_calls: Vec<ToolCallInfo>,  // For assistant messages with tool_use
}

/// Info about a tool call made by the assistant
#[derive(Debug, Clone)]
pub struct ToolCallInfo {
    pub id: String,
    pub name: String,
    pub input: String,  // JSON string
}

/// Tool definition for the LLM
#[derive(Debug, Clone)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: Vec<ToolParam>,
}

/// Tool parameter definition
#[derive(Debug, Clone)]
pub struct ToolParam {
    pub name: String,
    pub param_type: String,  // "string", "number", "boolean", "object", "array"
    pub description: String,
    pub required: bool,
}

/// Streamed event from LLM
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Text chunk
    Text(String),
    /// Tool use request from the LLM
    ToolUse {
        id: String,
        name: String,
        input: String,  // JSON string of arguments
    },
    /// Stream complete
    Done,
    /// Error occurred
    Error(String),
}

/// Supported LLM providers
#[derive(Debug, Clone, Default)]
pub enum Provider {
    #[default]
    Anthropic,
    OpenAI,
    Ollama,
    Gemini,
    DeepSeek,
    Groq,
    XAI,
}

impl Provider {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "openai" => Provider::OpenAI,
            "ollama" => Provider::Ollama,
            "gemini" | "google" => Provider::Gemini,
            "deepseek" => Provider::DeepSeek,
            "groq" => Provider::Groq,
            "xai" | "grok" => Provider::XAI,
            _ => Provider::Anthropic,
        }
    }

    fn to_backend(&self) -> LLMBackend {
        match self {
            Provider::Anthropic => LLMBackend::Anthropic,
            Provider::OpenAI => LLMBackend::OpenAI,
            Provider::Ollama => LLMBackend::Ollama,
            Provider::Gemini => LLMBackend::Google,
            Provider::DeepSeek => LLMBackend::DeepSeek,
            Provider::Groq => LLMBackend::Groq,
            Provider::XAI => LLMBackend::XAI,
        }
    }
}

/// Chat configuration
#[derive(Debug, Clone)]
pub struct ChatConfig {
    pub provider: Provider,
    pub api_key: String,
    pub model: String,
    pub system: Option<String>,
    pub max_tokens: u32,
    pub tools: Vec<Tool>,
}

impl Default for ChatConfig {
    fn default() -> Self {
        ChatConfig {
            provider: Provider::Anthropic,
            api_key: String::new(),
            model: "claude-sonnet-4-20250514".to_string(),
            system: None,
            max_tokens: 4096,
            tools: Vec::new(),
        }
    }
}

/// Send a chat message and stream the response
///
/// This spawns a thread that streams chunks back via the channel.
/// Scheme receives chunks and appends to buffer.
pub fn chat_stream(
    config: ChatConfig,
    messages: Vec<Message>,
    tx: Sender<StreamEvent>,
) {
    log("info", "llm", "chat-start", &[
        ("messages", &messages.len().to_string()),
        ("provider", &format!("{:?}", config.provider)),
        ("model", &config.model),
    ]);
    for (i, m) in messages.iter().enumerate() {
        log("debug", "llm", "chat-message", &[
            ("index", &i.to_string()),
            ("role", &m.role),
            ("len", &m.content.len().to_string()),
        ]);
    }

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            if let Err(e) = stream_chat(&config, &messages, &tx).await {
                log("error", "llm", "chat-error", &[("error", &e.to_string())]);
                let _ = tx.send(StreamEvent::Error(e.to_string()));
            }
        });
    });
}

async fn stream_chat(
    config: &ChatConfig,
    messages: &[Message],
    tx: &Sender<StreamEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Build LLM client
    let mut builder = LLMBuilder::new()
        .backend(config.provider.to_backend())
        .api_key(&config.api_key)
        .model(&config.model)
        .max_tokens(config.max_tokens);

    if let Some(ref system) = config.system {
        builder = builder.system(system);
    }

    // Add tools if provided
    for tool in &config.tools {
        let mut func_builder = FunctionBuilder::new(&tool.name)
            .description(&tool.description);

        let mut required_params = Vec::new();

        for param in &tool.parameters {
            func_builder = func_builder.param(
                ParamBuilder::new(&param.name)
                    .type_of(&param.param_type)
                    .description(&param.description)
            );
            if param.required {
                required_params.push(param.name.clone());
            }
        }

        if !required_params.is_empty() {
            func_builder = func_builder.required(required_params);
        }

        builder = builder.function(func_builder);
    }

    let llm = builder.build()?;

    // Convert messages to ChatMessage format
    let chat_messages: Vec<ChatMessage> = messages.iter()
        .map(|m| {
            match m.role.as_str() {
                "assistant" => {
                    let mut builder = ChatMessage::assistant().content(&m.content);
                    // If assistant made tool calls, include them
                    if !m.tool_calls.is_empty() {
                        let tool_calls: Vec<ToolCall> = m.tool_calls.iter()
                            .map(|tc| ToolCall {
                                id: tc.id.clone(),
                                call_type: "function".to_string(),
                                function: FunctionCall {
                                    name: tc.name.clone(),
                                    arguments: tc.input.clone(),
                                },
                            })
                            .collect();
                        builder = builder.tool_use(tool_calls);
                    }
                    builder.build()
                }
                "tool_result" => {
                    // Tool results are sent back with the tool_use_id
                    let id = m.tool_use_id.as_deref().unwrap_or("").to_string();
                    let tool_call = ToolCall {
                        id,
                        call_type: "function".to_string(),
                        function: FunctionCall {
                            name: String::new(),  // Not needed for results
                            arguments: m.content.clone(),
                        },
                    };
                    ChatMessage::user()
                        .tool_result(vec![tool_call])
                        .build()
                }
                _ => ChatMessage::user().content(&m.content).build(),
            }
        })
        .collect();

    // Use tool-enabled streaming if tools are configured
    if !config.tools.is_empty() {
        stream_with_tools(&llm, &chat_messages, tx).await
    } else {
        stream_text_only(&llm, &chat_messages, tx).await
    }
}

/// Stream a response with tool support
async fn stream_with_tools(
    llm: &Box<dyn LLMProvider>,
    messages: &[ChatMessage],
    tx: &Sender<StreamEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tools = llm.tools().map(|t| t.to_vec()).unwrap_or_default();
    let tools_ref: Option<&[_]> = if tools.is_empty() { None } else { Some(&tools) };
    let mut stream = llm.chat_stream_with_tools(messages, tools_ref).await?;

    // Track tool use accumulation (input arrives in chunks)
    let mut current_tool: Option<(String, String, String)> = None; // (id, name, json_buffer)

    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => match chunk {
                StreamChunk::Text(text) => {
                    let _ = tx.send(StreamEvent::Text(text));
                }
                StreamChunk::ToolUseStart { id, name, .. } => {
                    // Start accumulating tool input
                    current_tool = Some((id, name, String::new()));
                }
                StreamChunk::ToolUseInputDelta { partial_json, .. } => {
                    // Accumulate JSON chunks
                    if let Some((_, _, ref mut buffer)) = current_tool {
                        buffer.push_str(&partial_json);
                    }
                }
                StreamChunk::ToolUseComplete { tool_call, .. } => {
                    // Send the complete tool use event
                    let _ = tx.send(StreamEvent::ToolUse {
                        id: tool_call.id,
                        name: tool_call.function.name,
                        input: tool_call.function.arguments,
                    });
                    current_tool = None;
                }
                StreamChunk::Done { .. } => {
                    // If we have an incomplete tool use, send what we have
                    if let Some((id, name, input)) = current_tool.take() {
                        let _ = tx.send(StreamEvent::ToolUse { id, name, input });
                    }
                    let _ = tx.send(StreamEvent::Done);
                }
            },
            Err(e) => {
                let _ = tx.send(StreamEvent::Error(e.to_string()));
            }
        }
    }

    // Ensure Done is sent if stream ends without explicit Done chunk
    let _ = tx.send(StreamEvent::Done);
    Ok(())
}

/// Stream a text-only response (no tools)
async fn stream_text_only(
    llm: &Box<dyn LLMProvider>,
    messages: &[ChatMessage],
    tx: &Sender<StreamEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut stream = llm.chat_stream(messages).await?;

    while let Some(Ok(token)) = stream.next().await {
        let _ = tx.send(StreamEvent::Text(token));
    }

    let _ = tx.send(StreamEvent::Done);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_from_str() {
        assert!(matches!(Provider::from_str("anthropic"), Provider::Anthropic));
        assert!(matches!(Provider::from_str("openai"), Provider::OpenAI));
        assert!(matches!(Provider::from_str("ollama"), Provider::Ollama));
        assert!(matches!(Provider::from_str("gemini"), Provider::Gemini));
        assert!(matches!(Provider::from_str("google"), Provider::Gemini));
    }

    #[test]
    fn test_tool_definition() {
        let tool = Tool {
            name: "read_file".to_string(),
            description: "Read contents of a file".to_string(),
            parameters: vec![
                ToolParam {
                    name: "path".to_string(),
                    param_type: "string".to_string(),
                    description: "The file path to read".to_string(),
                    required: true,
                },
            ],
        };

        assert_eq!(tool.name, "read_file");
        assert_eq!(tool.parameters.len(), 1);
        assert!(tool.parameters[0].required);
    }

    #[test]
    fn test_message_with_tool_use_id() {
        let msg = Message {
            role: "tool_result".to_string(),
            content: r#"{"content": "file contents"}"#.to_string(),
            tool_use_id: Some("call_123".to_string()),
        };

        assert_eq!(msg.role, "tool_result");
        assert!(msg.tool_use_id.is_some());
        assert_eq!(msg.tool_use_id.unwrap(), "call_123");
    }

    #[test]
    fn test_stream_event_variants() {
        let text_event = StreamEvent::Text("Hello".to_string());
        assert!(matches!(text_event, StreamEvent::Text(_)));

        let tool_event = StreamEvent::ToolUse {
            id: "call_123".to_string(),
            name: "read_file".to_string(),
            input: r#"{"path":"/test.txt"}"#.to_string(),
        };
        assert!(matches!(tool_event, StreamEvent::ToolUse { .. }));

        let done_event = StreamEvent::Done;
        assert!(matches!(done_event, StreamEvent::Done));

        let error_event = StreamEvent::Error("Something went wrong".to_string());
        assert!(matches!(error_event, StreamEvent::Error(_)));
    }

    #[test]
    fn test_chat_config_default() {
        let config = ChatConfig::default();
        assert!(matches!(config.provider, Provider::Anthropic));
        assert_eq!(config.max_tokens, 4096);
        assert!(config.tools.is_empty());
    }
}
