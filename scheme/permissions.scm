;;; permissions.scm - Tool Permission System
;;;
;;; Permission levels:
;;; - 'allow     : Always allow (no prompt)
;;; - 'deny      : Always deny (no prompt)
;;; - 'ask       : Prompt each time
;;; - 'ask-once  : Ask once per session, then allow
;;;
;;; Users configure in ~/.aimax/init.scm:
;;;   (set! *tool-permissions*
;;;     '((read_file . allow)
;;;       (write_file . ask)
;;;       (run_command . ask-once)))

;; Default permission policy (can be overridden in init.scm)
(define *tool-permissions*
  '((read_file . allow)
    (list_directory . allow)
    (search_files . allow)
    (read_buffer . allow)
    (write_file . ask)
    (insert_text . ask-once)
    (run_command . ask)))

;; Session grants: tools approved this session (for 'ask-once)
(define *session-grants* '())

;; Pending permission request (for async UI)
(define *pending-permission* #f)

;; === Permission Checking ===

;; Get permission level for a tool
(define (tool-permission-level tool-name)
  (let ((entry (assoc tool-name *tool-permissions*)))
    (if entry
        (cdr entry)
        'ask)))  ; Default to ask if not configured

;; Check if tool has session grant
(define (has-session-grant? tool-name)
  (member tool-name *session-grants*))

;; Grant session permission
(define (grant-session-permission tool-name)
  (unless (has-session-grant? tool-name)
    (set! *session-grants* (cons tool-name *session-grants*))))

;; Clear session grants (e.g., on new chat)
(define (clear-session-grants)
  (set! *session-grants* '()))

;; Check permission for a tool
;; Returns: 'allow, 'deny, or 'ask (needs user prompt)
(define (check-permission tool-name)
  (let ((level (tool-permission-level tool-name)))
    (cond
      ((eq? level 'allow) 'allow)
      ((eq? level 'deny) 'deny)
      ((eq? level 'ask-once)
       (if (has-session-grant? tool-name)
           'allow
           'ask))
      (else 'ask))))  ; 'ask

;; === Permission UI ===

;; Request permission from user (async - sets pending state)
;; Returns immediately, UI will call permission-respond
(define (request-permission tool-name tool-input)
  (set! *pending-permission*
        (list tool-name tool-input))
  (message (string-append "Tool permission needed: " tool-name)))

;; Called when user responds to permission prompt
;; response: 'allow, 'allow-session, or 'deny
(define (permission-respond response)
  (when *pending-permission*
    (let ((tool-name (car *pending-permission*))
          (tool-input (cadr *pending-permission*)))
      (set! *pending-permission* #f)
      (cond
        ((eq? response 'allow-session)
         (grant-session-permission tool-name)
         (execute-pending-tool tool-name tool-input))
        ((eq? response 'allow)
         (execute-pending-tool tool-name tool-input))
        ((eq? response 'deny)
         (tool-denied tool-name))))))

;; Execute a tool after permission granted (stub - will be implemented)
(define (execute-pending-tool tool-name tool-input)
  (message (string-append "Executing: " tool-name)))

;; Called when tool is denied
(define (tool-denied tool-name)
  (message (string-append "Tool denied: " tool-name)))

;; Check if there's a pending permission request
(define (permission-pending?)
  (not (not *pending-permission*)))

;; Get pending permission details
(define (get-pending-permission)
  *pending-permission*)

;; === Configuration Helpers ===

;; Set permission for a tool
(define (set-tool-permission tool-name level)
  (let ((entry (assoc tool-name *tool-permissions*)))
    (if entry
        (set-cdr! entry level)
        (set! *tool-permissions*
              (cons (cons tool-name level) *tool-permissions*)))))

;; Allow a tool by default
(define (allow-tool tool-name)
  (set-tool-permission tool-name 'allow))

;; Deny a tool by default
(define (deny-tool tool-name)
  (set-tool-permission tool-name 'deny))

;; Require asking for a tool
(define (ask-tool tool-name)
  (set-tool-permission tool-name 'ask))
