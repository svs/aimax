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
  "keyboard-quit"
  "ask-ai"
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

;; === Aliases for primitives ===

(define switch-buffer switch-buffer!)
(define kill-buffer kill-buffer!)
(define keyboard-quit keyboard-quit!)
(define newline newline!)
(define delete-backward-char buffer-delete)
