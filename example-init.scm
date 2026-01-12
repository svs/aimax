;;; Example ~/.aimax/init.scm
;;;
;;; Copy this to ~/.aimax/init.scm and customize.

;; AI Configuration
;; Set your API key from environment (recommended) or hardcode (not recommended)
(set! *ai-api-key* (getenv "ANTHROPIC_API_KEY"))
(set! *ai-model* "claude-sonnet-4-20250514")
(set! *ai-system* "You are a helpful programming assistant. Be concise.")

;; Example: Custom face colors
;; (set-face-attribute "font-lock-keyword-face" ":foreground" "#ff79c6")
;; (set-face-attribute "font-lock-string-face" ":foreground" "#f1fa8c")

;; Example: Custom hook before quit
;; (set! *before-quit* (lambda () (desktop-save)))
