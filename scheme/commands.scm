;;; commands.scm - Editor commands

;; === Hooks ===
(define *before-quit* (lambda () #t))
(define *before-save* (lambda () #t))
(define *after-save* (lambda () #t))

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

;; === Aliases for primitives ===

(define switch-buffer switch-buffer!)
(define keyboard-quit keyboard-quit!)
(define newline newline!)
(define delete-backward-char buffer-delete)
