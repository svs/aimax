;;; chat.scm - AI Chat with Multi-Turn Support and Agentic Loop
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
;; Each message is a list: (role content) or (role content tool_use_id)
;; - User messages: ("user" "message content")
;; - Assistant messages: ("assistant" "response content")
;; - Tool results: ("tool_result" "result content" "tool_call_id" ())
;; Message format: (role content tool_use_id tool_calls)
;; - tool_calls is a list of (id name input) for assistant messages with tool_use
(define *chat-messages* '())

;; Add a message to conversation history
;; For regular messages (user/assistant), just (role content)
(define (chat-add-message role content)
  (set! *chat-messages*
        (append *chat-messages*
                (list (list role content "" '())))))

;; Add an assistant message that includes a tool_use
;; This is needed for correct API message format
(define (chat-add-assistant-with-tool content tool-id tool-name tool-input)
  (set! *chat-messages*
        (append *chat-messages*
                (list (list "assistant" content ""
                            (list (list tool-id tool-name tool-input)))))))

;; Add a tool result with its ID
(define (chat-add-tool-message id content)
  (set! *chat-messages*
        (append *chat-messages*
                (list (list "tool_result" content id '())))))

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

;; Open the chat buffer
;; Creates *chat* if it doesn't exist, switches to it, positions cursor
(define (open-chat-buffer)
  ;; Create buffer if needed (buffer-create is idempotent-ish for existing buffers)
  (buffer-create "*chat*")
  (buffer-switch "*chat*")
  ;; If buffer is empty, add initial prompt
  (when (= (string-length (buffer-text)) 0)
    (buffer-insert ">>> "))
  (end-of-buffer))

;; Start a new chat - clears history and opens chat buffer
(define (ask-ai)
  (chat-clear)
  (open-chat-buffer))

;; Continue conversation - opens chat buffer, keeps history
(define (ask-ai-continue)
  (open-chat-buffer))

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

;; Add a tool result to the conversation and continue the agentic loop
;; This is called after a tool executes (from Rust)
(define (chat-add-tool-result id status content)
  ;; Add tool result to conversation with its ID
  (chat-add-tool-message id content)
  (message (string-append "Tool result: " (if (string=? status "success") "OK" "ERROR")))

  ;; Continue the agentic loop - send conversation back to LLM
  ;; The LLM will see the tool result and can respond with text or more tool calls
  (chat-continue-after-tool))

;; Continue chat after tool result
;; Sends the full conversation (including tool result) back to the LLM
(define (chat-continue-after-tool)
  (message "Continuing chat after tool result...")
  (ai-chat-messages *ai-provider* *ai-api-key* *ai-model*
                    *ai-system* *chat-messages*))

;; === Keybindings ===
;; C-c C-c to send chat from buffer (like gptel)
(global-set-key "C-c C-c" "chat-send-from-buffer")
