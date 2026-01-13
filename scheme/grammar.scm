;;; grammar.scm - Tree-sitter grammar definition in Scheme
;;;
;;; Define grammars in Scheme, compile to tree-sitter JSON format,
;;; then to C and shared libraries.

;; Registry of defined grammars
(define *grammars* '())

;; === Grammar DSL ===

;; (seq rule1 rule2 ...) - sequence of rules
(define (seq . rules)
  `(seq ,@rules))

;; (choice rule1 rule2 ...) - one of the rules
(define (choice . rules)
  `(choice ,@rules))

;; (repeat rule) - zero or more
(define (repeat rule)
  `(repeat ,rule))

;; (repeat1 rule) - one or more
(define (repeat1 rule)
  `(repeat1 ,rule))

;; (optional rule) - zero or one
(define (optional rule)
  `(optional ,rule))

;; (pattern regex) - terminal matching regex
(define (pattern regex)
  `(pattern ,regex))

;; (field name rule) - named field
(define (field name rule)
  `(field ,name ,rule))

;; (prec n rule) - precedence
(define (prec n rule)
  `(prec ,n ,rule))

;; (prec-left n rule) - left associative precedence
(define (prec-left n rule)
  `(prec-left ,n ,rule))

;; (prec-right n rule) - right associative precedence
(define (prec-right n rule)
  `(prec-right ,n ,rule))

;; === Grammar Definition ===

;; Internal: create grammar structure
(define (make-grammar name rules)
  `((name . ,name)
    (rules . ,rules)))

;; Define a grammar and register it
;; Usage: (define-grammar name (rule-name rule-body) ...)
(define-syntax define-grammar
  (syntax-rules ()
    ((_ name (rule-name rule-body) ...)
     (let ((g (make-grammar 'name '((rule-name . rule-body) ...))))
       (set! *grammars* (cons (cons 'name g) *grammars*))
       g))))

;; Get a grammar by name
(define (get-grammar name)
  (let ((entry (assoc name *grammars*)))
    (if entry (cdr entry) #f)))

;; === Compile to JSON ===

;; Convert rule to tree-sitter JSON format
(define (rule->json rule)
  (cond
    ;; String literal
    ((string? rule)
     (string-append "{\"type\":\"STRING\",\"value\":\"" rule "\"}"))

    ;; Symbol reference
    ((symbol? rule)
     (string-append "{\"type\":\"SYMBOL\",\"name\":\"" (symbol->string rule) "\"}"))

    ;; Compound rules
    ((pair? rule)
     (let ((type (car rule)))
       (case type
         ((seq)
          (string-append "{\"type\":\"SEQ\",\"members\":["
                        (string-join (map rule->json (cdr rule)) ",")
                        "]}"))
         ((choice)
          (string-append "{\"type\":\"CHOICE\",\"members\":["
                        (string-join (map rule->json (cdr rule)) ",")
                        "]}"))
         ((repeat)
          (string-append "{\"type\":\"REPEAT\",\"content\":"
                        (rule->json (cadr rule))
                        "}"))
         ((repeat1)
          (string-append "{\"type\":\"REPEAT1\",\"content\":"
                        (rule->json (cadr rule))
                        "}"))
         ((optional)
          (string-append "{\"type\":\"CHOICE\",\"members\":["
                        (rule->json (cadr rule))
                        ",{\"type\":\"BLANK\"}]}"))
         ((pattern)
          (string-append "{\"type\":\"PATTERN\",\"value\":\""
                        (escape-json-string (cadr rule))
                        "\"}"))
         ((field)
          (string-append "{\"type\":\"FIELD\",\"name\":\""
                        (symbol->string (cadr rule))
                        "\",\"content\":"
                        (rule->json (caddr rule))
                        "}"))
         ((prec)
          (string-append "{\"type\":\"PREC\",\"value\":"
                        (number->string (cadr rule))
                        ",\"content\":"
                        (rule->json (caddr rule))
                        "}"))
         ((prec-left)
          (string-append "{\"type\":\"PREC_LEFT\",\"value\":"
                        (number->string (cadr rule))
                        ",\"content\":"
                        (rule->json (caddr rule))
                        "}"))
         ((prec-right)
          (string-append "{\"type\":\"PREC_RIGHT\",\"value\":"
                        (number->string (cadr rule))
                        ",\"content\":"
                        (rule->json (caddr rule))
                        "}"))
         (else
          (string-append "{\"type\":\"SYMBOL\",\"name\":\""
                        (symbol->string type) "\"}")))))

    (else "{\"type\":\"BLANK\"}")))

;; Escape string for JSON
(define (escape-json-string s)
  (let loop ((chars (string->list s)) (result '()))
    (if (null? chars)
        (list->string (reverse result))
        (let ((c (car chars)))
          (cond
            ((char=? c #\\) (loop (cdr chars) (cons #\\ (cons #\\ result))))
            ((char=? c #\") (loop (cdr chars) (cons #\" (cons #\\ result))))
            ((char=? c #\newline) (loop (cdr chars) (cons #\n (cons #\\ result))))
            ((char=? c #\tab) (loop (cdr chars) (cons #\t (cons #\\ result))))
            (else (loop (cdr chars) (cons c result))))))))

;; Join strings with separator
(define (string-join strs sep)
  (if (null? strs)
      ""
      (let loop ((rest (cdr strs)) (result (car strs)))
        (if (null? rest)
            result
            (loop (cdr rest) (string-append result sep (car rest)))))))

;; Convert grammar to tree-sitter JSON
(define (grammar->json grammar)
  (let ((name (cdr (assoc 'name grammar)))
        (rules (cdr (assoc 'rules grammar))))
    (string-append
      "{\n"
      "  \"name\": \"" (symbol->string name) "\",\n"
      "  \"rules\": {\n"
      (string-join
        (map (lambda (rule)
               (string-append "    \"" (symbol->string (car rule)) "\": "
                            (rule->json (cdr rule))))
             rules)
        ",\n")
      "\n  }\n"
      "}")))

;; === Compile to shared library ===

;; Write grammar JSON to file
(define (grammar-write-json grammar path)
  (let ((json (grammar->json grammar)))
    (write-file path json)))

;; Compile grammar to shared library
;; Requires tree-sitter CLI installed
(define (grammar-compile name output-dir)
  (let ((grammar (get-grammar name)))
    (if (not grammar)
        (error "Grammar not found:" name)
        (let* ((work-dir (string-append "/tmp/ts-" (symbol->string name)))
               (json-path (string-append work-dir "/grammar.json"))
               (lib-name (string-append "libtree-sitter-" (symbol->string name)))
               (lib-ext (if (string=? (getenv "OSTYPE") "darwin") ".dylib" ".so"))
               (lib-path (string-append output-dir "/" lib-name lib-ext)))
          ;; Create work directory
          (shell-command-to-string (string-append "mkdir -p " work-dir))
          ;; Write grammar JSON
          (grammar-write-json grammar json-path)
          ;; Generate C code
          (shell-command-to-string
            (string-append "cd " work-dir " && tree-sitter generate --abi 14 " json-path))
          ;; Compile to shared library
          (shell-command-to-string
            (string-append "cc -shared -fPIC -o " lib-path " " work-dir "/src/parser.c -I" work-dir "/src"))
          ;; Return path
          lib-path))))

;; === Log grammar (s-expression format) ===
;; Format: (log LEVEL MODULE EVENT "TIMESTAMP" ((key . "value") ...))

(define-grammar log
  (source_file (repeat entry))

  (entry (seq "(" "log" level module event timestamp fields ")"))

  (level (choice "debug" "info" "warn" "error"))

  (module (pattern "[a-z_]+"))

  (event (pattern "[a-z_-]+"))

  (timestamp (pattern "\"[^\"]*\""))

  (fields (seq "(" (repeat field) ")"))

  (field (seq "(" key "." value ")"))

  (key (pattern "[a-z_-]+"))

  (value (choice string number symbol))

  (string (pattern "\"([^\"\\\\]|\\\\.)*\""))

  (number (pattern "\\d+"))

  (symbol (pattern "[a-zA-Z_][a-zA-Z0-9_-]*")))
