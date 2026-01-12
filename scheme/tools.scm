;;; tools.scm - Tool Definitions and Execution
;;;
;;; This module bridges Scheme and Rust tool execution.
;;; Built-in tools are registered in Rust (tools.rs).
;;; Custom tools can be defined here in Scheme.

;; Load permissions
(load "permissions.scm")

;; === Tool Registry ===

;; Scheme-defined tools (in addition to Rust built-ins)
(define *scheme-tools* '())

;; Register a Scheme tool
;; Usage: (define-tool "name" "description" handler)
;; where handler is (lambda (input) ...)
(define (define-tool name description handler)
  (set! *scheme-tools*
        (cons (list name description handler)
              *scheme-tools*)))

;; Get a Scheme tool by name
(define (get-scheme-tool name)
  (let ((entry (assoc name *scheme-tools*)))
    (if entry
        (caddr entry)  ; The handler
        #f)))

;; === Tool Execution ===

;; Handle a tool use request from the LLM
;; This is called from chat.scm when a tool use event arrives
(define (handle-tool-request tool-id tool-name tool-input)
  (let ((permission (check-permission tool-name)))
    (cond
      ((eq? permission 'allow)
       (execute-tool tool-id tool-name tool-input))
      ((eq? permission 'deny)
       (send-tool-result tool-id #f "Tool access denied by policy"))
      ((eq? permission 'ask)
       ;; Store for async permission prompt
       (set! *pending-tool-execution*
             (list tool-id tool-name tool-input))
       (request-permission tool-name tool-input)))))

;; Pending tool execution (waiting for permission)
(define *pending-tool-execution* #f)

;; Execute a tool (after permission check)
(define (execute-tool tool-id tool-name tool-input)
  ;; First check if it's a Scheme tool
  (let ((scheme-handler (get-scheme-tool tool-name)))
    (if scheme-handler
        ;; Execute Scheme tool
        (let ((result (scheme-handler tool-input)))
          (send-tool-result tool-id #t result))
        ;; Call Rust built-in tool
        (execute-rust-tool tool-id tool-name tool-input))))

;; Called when user grants permission
(define (on-permission-granted)
  (when *pending-tool-execution*
    (let ((tool-id (car *pending-tool-execution*))
          (tool-name (cadr *pending-tool-execution*))
          (tool-input (caddr *pending-tool-execution*)))
      (set! *pending-tool-execution* #f)
      (execute-tool tool-id tool-name tool-input))))

;; Called when user denies permission
(define (on-permission-denied)
  (when *pending-tool-execution*
    (let ((tool-id (car *pending-tool-execution*))
          (tool-name (cadr *pending-tool-execution*)))
      (set! *pending-tool-execution* #f)
      (send-tool-result tool-id #f "Tool access denied by user"))))

;; Send tool result back (stub - Rust will handle actual sending)
(define (send-tool-result tool-id success content)
  ;; This adds to chat messages and triggers continuation
  (chat-add-tool-result tool-id (if success "success" "error") content))

;; === Example Scheme Tools ===

;; Buffer tools (access current buffer)
(define-tool "get_buffer_text"
  "Get the text content of the current buffer"
  (lambda (input)
    (buffer-text)))

(define-tool "get_selection"
  "Get the currently selected text"
  (lambda (input)
    (let ((selection (buffer-selection)))
      (if selection
          selection
          "No selection"))))

(define-tool "buffer_info"
  "Get information about the current buffer"
  (lambda (input)
    (let ((name (buffer-name))
          (modified (buffer-modified-p))
          (lines (buffer-line-count)))
      (string-append
        "Name: " name "\n"
        "Modified: " (if modified "yes" "no") "\n"
        "Lines: " (number->string lines)))))
