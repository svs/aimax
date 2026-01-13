# Architectural Audit: Aimax AI & Chat System

**Date:** January 13, 2026
**Status:** Critical Issues Identified

## 1. Executive Summary

The `aimax` project exhibits a sophisticated hybrid architecture (Rust core + Scheme scripting), but currently suffers from a critical security vulnerability and a strategic divergence between its documentation and implementation regarding AI agents.

## 2. Architectural Overview

The system currently implements an **"Embedded Agent"** model where:
*   **Rust (`core/src/llm.rs`)** handles the HTTP/Streaming connection to providers (Anthropic, OpenAI, etc.) and exposes a `ToolRegistry`.
*   **Scheme (`scheme/chat.scm`)** manages the conversation state and the "agentic loop" (receiving tool calls, queueing them, and requesting execution).
*   **Bridge (`core/src/scheme.rs`)** passes actions between the two.

This contrasts with the **"Agent Client Protocol (ACP)"** described in `docs/ACP.md`, which outlines a JSON-RPC architecture for connecting to *external* agent processes (like a `claude-cli`). **The ACP system is currently aspirational documentation, while the Embedded Agent is the actual running code.**

## 3. Critical Critique: The Embedded Agent

### 🚨 Security (Major Vulnerability)
The current implementation allows for **unconstrained Remote Code Execution (RCE)** by the LLM.

*   **Mechanism:** The LLM can call `run_command` (executing `sh -c`) or `write_file` via the `execute-rust-tool` primitive.
*   **The Flaw:** There is **zero user confirmation or sandboxing**.
    *   `scheme/chat.scm` immediately calls `(process-next-tool)` which triggers `(execute-rust-tool)`.
    *   `core/src/tools.rs` executes the command immediately.
*   **Risk:** A prompt injection or a "jailbroken" model response could delete files, exfiltrate data, or install malware without the user seeing a prompt.

### ⚠️ Concurrency & Performance
The tool execution path is likely blocking the main UI thread.

*   `Action::ExecuteTool` is pushed to a queue and processed by the main editor loop.
*   `tools.rs` uses synchronous `std::fs::read_to_string` and `std::process::Command::output`.
*   **Result:** If the agent runs `sleep 5` or reads a large file, the entire editor UI will freeze until the operation completes.

### 🧠 Context Management
The context strategy is naive.

*   `chat-with-context` dumps the *entire* current buffer into the prompt.
*   **Issue:** This will quickly exhaust token limits for large files and degrade model performance by flooding it with irrelevant code. There is no "smart context" (e.g., relevant snippets, definitions, or file tree summaries).

## 4. Strategic Divergence: ACP vs. Embedded

The project needs to decide on its primary identity:
1.  **The Host (ACP):** Aimax is a UI for external, powerful agents (Claude Code, Gemini CLI). This requires implementing the JSON-RPC server described in `ACP.md`.
2.  **The Agent (Embedded):** Aimax *is* the agent. This requires hardening the current `llm.rs` implementation.

**Recommendation:** The Embedded approach offers tighter integration (direct buffer manipulation, AST access), while ACP offers access to more mature external tools. A hybrid is possible but complex.

## 5. Summary of Issues & Recommendations

| Severity | Component | Issue | Recommendation |
| :--- | :--- | :--- | :--- |
| **CRITICAL** | **Security** | `run_command` executes without confirmation. | **Implement a `Confirmation` UI.** The Scheme layer *must* prompt the user (via Minibuffer) before calling `execute-rust-tool` for sensitive actions. |
| **HIGH** | **Concurrency** | Tools use blocking I/O on the main thread. | Move tool execution to a background thread/task and report results back via a channel/Action. |
| **MEDIUM** | **Architecture** | ACP docs do not match Codebase. | Mark ACP as "Future/Experimental" or begin implementing the JSON-RPC layer if that is the goal. |
| **MEDIUM** | **Context** | Naive buffer dumping. | Implement `tree-sitter` based context extraction (e.g., "only send function signatures" or "send relevant folding ranges"). |

## 6. Action Plan

1.  **Immediate Fix:** Implement user confirmation in `scheme/chat.scm` for `run_command` and `write_file`.
2.  **Short Term:** Refactor `execute-rust-tool` to run asynchronously.
3.  **Medium Term:** Implement `tree-sitter` queries for smart context extraction.
