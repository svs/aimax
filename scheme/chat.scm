;;; chat.scm - AI Chat with Multi-Turn Support
;;;
;;; User configures these in ~/.aimax/init.scm:
;;;   (set! *ai-provider* "anthropic")  ; or "openai", "ollama", "gemini", etc.
;;;   (set! *ai-api-key* (getenv "ANTHROPIC_API_KEY"))
;;;   (set! *ai-model* "claude-sonnet-4-20250514")
;;;   (set! *ai-system* "You are a helpful assistant.")

;; === String helpers ===
;; Simple string-trim (removes leading/trailing spaces)
(define (string-trim str)
  (let* ((len (string-length str))
         (start (let loop ((i 0))
                  (if (>= i len) len
                      (if (char=? (string-ref str i) #\space)
                          (loop (+ i 1))
                          i))))
         (end (let loop ((i (- len 1)))
                (if (< i start) start
                    (if (char=? (string-ref str i) #\space)
                        (loop (- i 1))
                        (+ i 1))))))
    (substring str start end)))

;; Configuration (user overrides in init.scm)
(define *ai-provider* "anthropic")
(define *ai-api-key* (getenv "CLAUDE_API_KEY"))
(define *ai-model* "claude-sonnet-4-20250514")
(define *ai-system* "You are a helpful assistant.")

;; Conversation history for multi-turn chat
;; Each message is a list: (role content) where role is "user" or "assistant"
(define *chat-messages* '())

;; Add a message to conversation history
(define (chat-add-message role content)
  (set! *chat-messages*
        (append *chat-messages*
                (list (list role content)))))

;; Clear conversation history (start fresh)
(define (chat-clear)
  (set! *chat-messages* '()))

;; Called by Rust when assistant response completes
;; This adds the response to history for multi-turn
(define (chat-on-response-complete response)
  (chat-add-message "assistant" response)
  (message (string-append "Chat history: " (number->string (length *chat-messages*)) " messages")))

;; Send a message in a multi-turn conversation
;; Adds user message to history and sends full history to LLM
(define (chat-send prompt)
  (chat-add-message "user" prompt)
  (message (string-append "Sending " (number->string (length *chat-messages*)) " messages..."))
  (ai-chat-messages *ai-provider* *ai-api-key* *ai-model*
                    *ai-system* *chat-messages*))

;; Simple chat - single turn, doesn't use history
;; Kept for backwards compatibility
(define (chat prompt)
  (chat-clear)  ; Clear history before single-turn chat
  (chat-send prompt))

;; Chat with context - include current buffer content
(define (chat-with-context prompt)
  (let* ((context (buffer-text))
         (full-prompt (string-append
                        "Context:\n```\n" context "\n```\n\n"
                        prompt)))
    (chat-send full-prompt)))

;; Continue an existing conversation (multi-turn)
(define (chat-continue prompt)
  (chat-send prompt))

;; Convenience functions for different providers

(define (chat-openai prompt)
  (let ((*ai-provider* "openai")
        (*ai-api-key* (getenv "OPENAI_API_KEY"))
        (*ai-model* "gpt-4o"))
    (chat prompt)))

(define (chat-ollama prompt)
  (let ((*ai-provider* "ollama")
        (*ai-api-key* "")
        (*ai-model* "llama3.2"))
    (chat prompt)))

(define (chat-gemini prompt)
  (let ((*ai-provider* "gemini")
        (*ai-api-key* (getenv "GOOGLE_API_KEY"))
        (*ai-model* "gemini-2.0-flash"))
    (chat prompt)))

;; Interactive commands

;; Start a new chat - prompts for input, clears history
(define (ask-ai)
  (chat-clear)
  (minibuffer-prompt "Ask AI: " chat-send))

;; Continue conversation - prompts for input, keeps history
(define (ask-ai-continue)
  (minibuffer-prompt "Continue: " chat-send))

;; Send from buffer - grab text after last >>> and send
;; Bound to C-c C-c in chat buffers
(define (chat-send-from-buffer)
  (message "chat-send-from-buffer called")
  (let* ((text (buffer-text))
         (prompt-pos (string-last-index-of text ">>> ")))
    (message (string-append "prompt-pos: " (if prompt-pos (number->string prompt-pos) "false")))
    (if prompt-pos
        (let ((input (substring text (+ prompt-pos 4))))
          (message (string-append "input: [" (string-trim input) "]"))
          (if (> (string-length (string-trim input)) 0)
              (chat-send (string-trim input))
              (message "No input after >>>")))
        (message "No >>> prompt found"))))

;; Helper: find last occurrence of substring
(define (string-last-index-of str substr)
  (let loop ((pos 0) (last-found #f))
    (let ((found (string-find str substr pos)))
      (if found
          (loop (+ found 1) found)
          last-found))))

;; Helper: find substring starting at pos
(define (string-find str substr start)
  (let ((len (string-length str))
        (sublen (string-length substr)))
    (let loop ((i start))
      (cond
        ((> (+ i sublen) len) #f)
        ((string=? (substring str i (+ i sublen)) substr) i)
        (else (loop (+ i 1)))))))

;; Get message count for debugging
(define (chat-message-count)
  (length *chat-messages*))

;; === Tool Use Support ===

;; Pending tool calls waiting for execution/permission
(define *pending-tool-calls* '())

;; Called by Rust when the LLM requests a tool use
;; id: unique identifier for this tool call
;; name: the tool name (e.g., "read_file")
;; input: JSON string of arguments
(define (chat-on-tool-use id name input)
  ;; Add to pending queue
  (set! *pending-tool-calls*
        (append *pending-tool-calls*
                (list (list id name input))))
  ;; Try to execute (will check permissions)
  (process-next-tool))

;; Process the next pending tool call
(define (process-next-tool)
  (when (not (null? *pending-tool-calls*))
    (let* ((tool-call (car *pending-tool-calls*))
           (id (car tool-call))
           (name (cadr tool-call))
           (input (caddr tool-call)))
      ;; Remove from queue
      (set! *pending-tool-calls* (cdr *pending-tool-calls*))
      ;; Execute tool via Rust
      (execute-rust-tool id name input))))

;; Get count of pending tool calls
(define (pending-tool-count)
  (length *pending-tool-calls*))

;; Clear pending tool calls
(define (clear-pending-tools)
  (set! *pending-tool-calls* '()))

;; Add a tool result to the conversation
;; This is called after a tool executes (from Rust)
(define (chat-add-tool-result id status content)
  (chat-add-message "tool_result" content)
  ;; Note: In the future, this will trigger continuation of the chat
  (message (string-append "Tool result: " (if (string=? status "success") "OK" "ERROR"))))

;; === Keybindings ===
;; C-c C-c to send chat from buffer (like gptel)
(global-set-key "C-c C-c" "chat-send-from-buffer")
