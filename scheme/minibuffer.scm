;;; minibuffer.scm - Minibuffer UI logic
;;;
;;; Controls the minibuffer: prompts, input, completion.
;;; Rust provides primitives, Scheme controls the behavior.

;; Minibuffer state
(define *minibuffer-active* #f)
(define *minibuffer-prompt* "")
(define *minibuffer-input* "")
(define *minibuffer-point* 0)
(define *minibuffer-matches* '())
(define *minibuffer-selected* 0)
(define *minibuffer-callback* #f)

;; Completion mode
(define *completing-file* #f)
(define *cwd* ".")

;; Activate minibuffer with prompt
(define (minibuffer-prompt prompt callback)
  (set! *minibuffer-active* #t)
  (set! *minibuffer-prompt* prompt)
  (set! *minibuffer-input* "")
  (set! *minibuffer-point* 0)
  (set! *minibuffer-matches* '())
  (set! *minibuffer-selected* 0)
  (set! *minibuffer-callback* callback))

;; Start file completion
(define (minibuffer-find-file cwd callback)
  (set! *cwd* cwd)
  (set! *completing-file* #t)
  (minibuffer-prompt "Find file: " callback)
  (minibuffer-update-completions))

;; Cancel minibuffer
(define (minibuffer-cancel)
  (set! *minibuffer-active* #f)
  (set! *completing-file* #f))

;; Complete with selected match
(define (minibuffer-complete)
  (when (and *minibuffer-active*
             (not (null? *minibuffer-matches*)))
    (let ((selected (list-ref *minibuffer-matches* *minibuffer-selected*)))
      (if *completing-file*
          ;; File completion: append to directory part
          (let ((dir-part (get-directory-part *minibuffer-input*)))
            (set! *minibuffer-input* (string-append dir-part selected))
            (set! *minibuffer-point* (string-length *minibuffer-input*))
            (minibuffer-update-completions))
          ;; Other completion: replace input
          (begin
            (set! *minibuffer-input* selected)
            (set! *minibuffer-point* (string-length *minibuffer-input*)))))))

;; Get the result value (uses selected completion if available)
(define (minibuffer-get-result)
  (if (null? *minibuffer-matches*)
      *minibuffer-input*
      (let* ((selected (list-ref *minibuffer-matches* *minibuffer-selected*))
             (dir-part (get-directory-part *minibuffer-input*)))
        (string-append dir-part selected))))

;; Submit minibuffer
(define (minibuffer-submit)
  (when *minibuffer-active*
    (let ((result (minibuffer-get-result))
          (callback *minibuffer-callback*))
      (minibuffer-cancel)
      (when callback
        (callback result)))))

;; Insert character
(define (minibuffer-insert char)
  (set! *minibuffer-input*
        (string-append
          (substring *minibuffer-input* 0 *minibuffer-point*)
          (string char)
          (substring *minibuffer-input* *minibuffer-point*)))
  (set! *minibuffer-point* (+ *minibuffer-point* 1))
  (when *completing-file*
    (minibuffer-update-completions)))

;; Delete backward
(define (minibuffer-delete-backward)
  (when (> *minibuffer-point* 0)
    (set! *minibuffer-input*
          (string-append
            (substring *minibuffer-input* 0 (- *minibuffer-point* 1))
            (substring *minibuffer-input* *minibuffer-point*)))
    (set! *minibuffer-point* (- *minibuffer-point* 1))
    (when *completing-file*
      (minibuffer-update-completions))))

;; Navigation
(define (minibuffer-next-completion)
  (when (< *minibuffer-selected* (- (length *minibuffer-matches*) 1))
    (set! *minibuffer-selected* (+ *minibuffer-selected* 1))))

(define (minibuffer-prev-completion)
  (when (> *minibuffer-selected* 0)
    (set! *minibuffer-selected* (- *minibuffer-selected* 1))))

;; Update completions (for file mode)
(define (minibuffer-update-completions)
  (when *completing-file*
    (let ((matches (file-completion-names *minibuffer-input* *cwd*)))
      (set! *minibuffer-matches* matches)
      (set! *minibuffer-selected* 0))))

;; Get directory part of path (everything up to and including last /)
(define (get-directory-part path)
  (let ((last-slash (string-last-index-of path "/")))
    (if last-slash
        (substring path 0 (+ last-slash 1))
        "")))

;; Get state for rendering (Rust calls this)
(define (minibuffer-get-state)
  (list *minibuffer-active*
        *minibuffer-prompt*
        *minibuffer-input*
        *minibuffer-point*
        *minibuffer-matches*
        *minibuffer-selected*))

;; Individual getters (Rust calls these)
(define (minibuffer-active?) *minibuffer-active*)
(define (minibuffer-get-prompt) *minibuffer-prompt*)
(define (minibuffer-get-value) *minibuffer-input*)
(define (minibuffer-get-matches) *minibuffer-matches*)
(define (minibuffer-get-selected) *minibuffer-selected*)

;; Handle key event (Rust calls this)
;; Returns: 'continue, 'submit, 'cancel
(define (minibuffer-handle-key key)
  (cond
    ((string=? key "return")
     (minibuffer-submit)
     'submit)
    ((string=? key "escape")
     (minibuffer-cancel)
     'cancel)
    ((string=? key "tab")
     (minibuffer-complete)
     'continue)
    ((string=? key "C-n")
     (minibuffer-next-completion)
     'continue)
    ((string=? key "C-p")
     (minibuffer-prev-completion)
     'continue)
    ((string=? key "down")
     (minibuffer-next-completion)
     'continue)
    ((string=? key "up")
     (minibuffer-prev-completion)
     'continue)
    ((string=? key "backspace")
     (minibuffer-delete-backward)
     'continue)
    ((string=? key "C-g")
     (minibuffer-cancel)
     'cancel)
    ;; Single character - insert it
    ((= (string-length key) 1)
     (minibuffer-insert (string-ref key 0))
     'continue)
    (else 'continue)))
