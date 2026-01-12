;;; chat.scm - AI Chat Configuration
;;;
;;; User configures these in ~/.aimax/init.scm:
;;;   (set! *ai-provider* "anthropic")  ; or "openai", "ollama", "gemini", etc.
;;;   (set! *ai-api-key* (getenv "ANTHROPIC_API_KEY"))
;;;   (set! *ai-model* "claude-sonnet-4-20250514")
;;;   (set! *ai-system* "You are a helpful assistant.")

;; Configuration (user overrides in init.scm)
(define *ai-provider* "anthropic")
(define *ai-api-key* (getenv "CLAUDE_API_KEY"))
(define *ai-model* "claude-sonnet-4-20250514")
(define *ai-system* "You are a helpful assistant.")

;; Simple chat - send a prompt, get streaming response in *chat* buffer
(define (chat prompt)
  (ai-chat *ai-provider* *ai-api-key* *ai-model* *ai-system* prompt))

;; Chat with context - include current buffer content
(define (chat-with-context prompt)
  (let* ((context (buffer-text))
         (full-prompt (string-append
                        "Context:\n```\n" context "\n```\n\n"
                        prompt)))
    (chat full-prompt)))

;; Convenience functions for different providers

(define (chat-openai prompt)
  (ai-chat "openai" (getenv "OPENAI_API_KEY") "gpt-4o" *ai-system* prompt))

(define (chat-ollama prompt)
  (ai-chat "ollama" "" "llama3.2" *ai-system* prompt))

(define (chat-gemini prompt)
  (ai-chat "gemini" (getenv "GOOGLE_API_KEY") "gemini-2.0-flash" *ai-system* prompt))

;; Interactive command - prompts for input in minibuffer
(define (ask-ai)
  (minibuffer-prompt "Ask AI: " chat))
