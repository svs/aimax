//! LLM Client - Multi-provider streaming
//!
//! Rust does the heavy lifting: HTTP, streaming, parsing.
//! Scheme defines: provider, model, system prompt, context, handlers.

use futures::StreamExt;
use llm::{builder::{LLMBackend, LLMBuilder}, chat::ChatMessage};
use std::sync::mpsc::Sender;

/// Message in a conversation
#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,      // "user" or "assistant"
    pub content: String,
}

/// Streamed event from LLM
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Text chunk
    Text(String),
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
}

impl Default for ChatConfig {
    fn default() -> Self {
        ChatConfig {
            provider: Provider::Anthropic,
            api_key: String::new(),
            model: "claude-sonnet-4-20250514".to_string(),
            system: None,
            max_tokens: 4096,
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
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            if let Err(e) = stream_chat(&config, &messages, &tx).await {
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

    let llm = builder.build()?;

    // Convert messages to ChatMessage format
    let chat_messages: Vec<ChatMessage> = messages.iter()
        .map(|m| {
            if m.role == "assistant" {
                ChatMessage::assistant().content(&m.content).build()
            } else {
                ChatMessage::user().content(&m.content).build()
            }
        })
        .collect();

    // Stream the response
    let mut stream = llm.chat_stream(&chat_messages).await?;

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
}
