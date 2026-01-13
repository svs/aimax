;;; commands.scm - Editor commands

;; === Hooks ===
(define *before-quit* (lambda () #t))
(define *before-save* (lambda () #t))
(define *after-save* (lambda () #t))

;; === Command Registry ===
;; Commands available via M-x
(define *commands* '(
  "quit"
  "save-buffer"
  "find-file"
  "switch-buffer"
  "kill-buffer"
  "kill-process-command"
  "eval-expression"
  "keyboard-quit"
  "ask-ai"
  "ask-ai-continue"
  "chat-send-from-buffer"
  "chat-clear"
  "shell-command"
  "shell-command-to-buffer"
  "forward-char"
  "backward-char"
  "next-line"
  "previous-line"
  "beginning-of-line"
  "end-of-line"
  "beginning-of-buffer"
  "end-of-buffer"
  "view-log"
  "refresh-log"
  "log-stream"
  "log-stream-errors"
  "log-stream-stop"
))

;; === Commands ===

(define (quit)
  (if (*before-quit*)
      (quit!)
      #f))

(define (save-buffer)
  (if (*before-save*)
      (begin
        (save-buffer!)
        (*after-save*))
      #f))

(define (find-file)
  (find-file!))

;; Shell command with prompt - shows output in message
(define (shell-command)
  (minibuffer-prompt "Shell command: "
    (lambda (cmd)
      (message (shell-command-to-string cmd)))))

;; Shell command to buffer - streams output in real-time
(define (shell-command-to-buffer)
  (minibuffer-prompt "Shell command: "
    (lambda (cmd)
      (buffer-create "*Shell Output*")
      (buffer-insert (string-append "$ " cmd "\n\n"))
      (start-process-simple "*Shell Output*" "sh" (list "-c" cmd)))))

;; M-x: prompt for command with completion
(define (execute-extended-command)
  (set! *minibuffer-completions* *commands*)
  (minibuffer-prompt "M-x "
    (lambda (cmd)
      (eval (read (open-input-string (string-append "(" cmd ")")))))))

;; M-: eval-expression - evaluate arbitrary Scheme
(define (eval-expression)
  (minibuffer-prompt "Eval: "
    (lambda (expr)
      (let ((result (eval (read (open-input-string expr)))))
        (message (to-string result))))))

;; === Aliases for primitives ===

(define switch-buffer switch-buffer!)
(define keyboard-quit keyboard-quit!)
(define newline newline!)
(define delete-backward-char buffer-delete)

;; === Process helpers ===

;; Check if a process name is in the running processes list
(define (process-running? name)
  (member name (process-names)))

;; Get the process name associated with a buffer (or "" if none)
(define (buffer-process-name buf-name)
  (buffer-local-p buf-name "process-name"))

;; === kill-buffer ===
;; Prompt for buffer, handle modified buffers and process buffers

(define (kill-buffer)
  (let ((names (buffer-names))
        (current (buffer-name)))
    (set! *minibuffer-completions* names)
    (minibuffer-prompt (string-append "Kill buffer (default " current "): ")
      (lambda (input)
        (let ((name (if (string=? input "") current input)))
          (kill-buffer-with-checks name))))))

;; Internal: kill buffer with modification and process checks
(define (kill-buffer-with-checks name)
  (let ((proc-name (buffer-process-name name))
        (modified (buffer-modified-p name)))
    (cond
      ;; Modified buffer with running process
      ((and modified (process-running? proc-name))
       (y-or-no-p (string-append "Buffer " name " modified; kill anyway?")
         (lambda (kill-modified)
           (when kill-modified
             (y-or-no-p (string-append "Kill process " proc-name " too?")
               (lambda (kill-proc)
                 (when kill-proc (kill-process proc-name))
                 (kill-buffer-named! name)))))))
      ;; Just modified
      (modified
       (y-or-no-p (string-append "Buffer " name " modified; kill anyway?")
         (lambda (confirmed)
           (when confirmed (kill-buffer-named! name)))))
      ;; Just has running process
      ((process-running? proc-name)
       (y-or-no-p (string-append "Buffer has active process " proc-name ". Kill process too?")
         (lambda (kill-proc)
           (when kill-proc (kill-process proc-name))
           (kill-buffer-named! name))))
      ;; No special handling needed
      (else
       (kill-buffer-named! name)))))

;; === view-log - observe aimax logs ===

(define (view-log)
  (buffer-create "*log*")
  (buffer-insert (read-file "/tmp/aimax.log")))

;; Refresh log buffer
(define (refresh-log)
  (when (string=? (buffer-name) "*log*")
    (let ((content (read-file "/tmp/aimax.log")))
      (beginning-of-buffer)
      ;; Clear and reload - crude but works
      (buffer-create "*log*")
      (buffer-insert content)
      (end-of-buffer))))

;; === kill-process command ===

(define (kill-process-command)
  (let ((procs (process-names)))
    (if (null? procs)
        (message "No running processes")
        (begin
          (set! *minibuffer-completions* procs)
          (minibuffer-prompt "Kill process: "
            (lambda (name)
              (if (process-running? name)
                  (kill-process name)
                  (message (string-append "No process: " name)))))))))
