;;; completion.scm - File completion for Aimax
;;;
;;; Provides completion candidates for find-file minibuffer.
;;; Uses Rust primitives: ls, fuzzy-match, directory?, expand-path

;; Parse user input into (directory . prefix)
;; "core/sr" -> ("/path/to/cwd/core" . "sr")
;; "/etc/pas" -> ("/etc" . "pas")
;; "~/.config/ai" -> ("/Users/x/.config" . "ai")
(define (parse-file-input input cwd)
  (let* ((expanded (expand-path input cwd))
         (last-slash (string-last-index-of expanded "/")))
    (if last-slash
        (let ((dir (substring expanded 0 (+ last-slash 1)))
              (prefix (substring expanded (+ last-slash 1))))
          (cons dir prefix))
        ;; No slash - prefix in cwd
        (cons cwd input))))

;; Find last index of char in string, or #f
(define (string-last-index-of str char)
  (let loop ((i (- (string-length str) 1)))
    (cond ((< i 0) #f)
          ((string=? (substring str i (+ i 1)) char) i)
          (else (loop (- i 1))))))

;; Get file completions for the given input
;; Returns list of (display-text . full-path) pairs
(define (file-completions input cwd)
  (let* ((parsed (parse-file-input input cwd))
         (dir (car parsed))
         (prefix (cdr parsed))
         (files (ls dir))
         (matches (if (string=? prefix "")
                      files
                      (fuzzy-match prefix files))))
    ;; Return list of completion candidates
    ;; Each is display text that when selected replaces input
    (map (lambda (f)
           ;; Full path for selection
           (let ((full (string-append dir f)))
             (cons f full)))
         matches)))

;; Get just the display names (for rendering)
(define (file-completion-names input cwd)
  (map car (file-completions input cwd)))

;; Get completion that would result from selecting candidate
(define (file-completion-select input cwd candidate)
  (let* ((parsed (parse-file-input input cwd))
         (dir (car parsed)))
    (string-append dir candidate)))
