;;; log.scm - Structured log viewing and querying
;;;
;;; Logs are s-expressions that Scheme can read directly:
;;; (log LEVEL MODULE EVENT "TIMESTAMP" ((key . "value") ...))

;; Path to log file
(define *log-path* "/tmp/aimax.log")

;; === Writing Logs ===

;; Format a single field as string
(define (format-log-field f)
  (string-append "(" (symbol->string (car f)) " . \""
                 (if (string? (cdr f)) (cdr f) (symbol->string (cdr f)))
                 "\")"))

;; Format fields as string for Rust logging
(define (format-log-fields fields)
  (if (null? fields)
      "()"
      (string-append "(" (string-join (map format-log-field fields) " ") ")")))

;; Convenience functions for different log levels (calls into Rust)
(define (log-debug module event fields)
  (scheme-log "debug" module event (format-log-fields fields)))

(define (log-info module event fields)
  (scheme-log "info" module event (format-log-fields fields)))

(define (log-warn module event fields)
  (scheme-log "warn" module event (format-log-fields fields)))

(define (log-error module event fields)
  (scheme-log "error" module event (format-log-fields fields)))

;; === Reading Logs ===

;; Read all log entries from file
(define (log-read-all)
  (let ((content (read-file *log-path*)))
    (if (string=? content "")
        '()
        (log-parse-entries content))))

;; Parse log entries from string
(define (log-parse-entries content)
  (let ((port (open-input-string (string-append "(" content ")"))))
    (let ((entries (read port)))
      (if (eof-object? entries)
          '()
          entries))))

;; === Log Entry Accessors ===

;; (log level mod-name event timestamp fields)
(define (log-level entry) (list-ref entry 1))
(define (log-module entry) (list-ref entry 2))
(define (log-event entry) (list-ref entry 3))
(define (log-timestamp entry) (list-ref entry 4))
(define (log-fields entry) (list-ref entry 5))

;; Get field value from entry
(define (log-field entry key)
  (let ((fields (log-fields entry)))
    (let ((pair (assoc key fields)))
      (if pair (cdr pair) #f))))

;; === Querying Logs ===

;; Filter logs by level
(define (log-filter-level entries level)
  (filter (lambda (e) (eq? (log-level e) level)) entries))

;; Filter logs by module
(define (log-filter-module entries mod-name)
  (filter (lambda (e) (eq? (log-module e) mod-name)) entries))

;; Filter logs by event
(define (log-filter-event entries event)
  (filter (lambda (e) (eq? (log-event e) event)) entries))

;; Filter logs by predicate
(define (log-filter entries pred)
  (filter pred entries))

;; Get all errors
(define (log-errors)
  (log-filter-level (log-read-all) 'error))

;; Get all warnings
(define (log-warnings)
  (log-filter-level (log-read-all) 'warn))

;; Get logs from a specific module
(define (log-from-module mod-name)
  (log-filter-module (log-read-all) mod-name))

;; === Display ===

;; Format a single entry for display
(define (log-format-entry entry)
  (let ((ts (log-timestamp entry))
        (lvl (symbol->string (log-level entry)))
        (mod (symbol->string (log-module entry)))
        (evt (symbol->string (log-event entry)))
        (fields (log-fields entry)))
    (string-append
      ts " "
      (string-upcase lvl) " "
      "[" mod "] "
      evt " "
      (log-format-fields fields))))

;; Format fields for display
(define (log-format-fields fields)
  (if (null? fields)
      ""
      (string-join
        (map (lambda (f)
               (string-append (symbol->string (car f)) "=" (cdr f)))
             fields)
        " ")))

;; Convert string to uppercase
(define (string-upcase s)
  (list->string
    (map char-upcase (string->list s))))

;; Join strings
(define (string-join strs sep)
  (if (null? strs)
      ""
      (let loop ((rest (cdr strs)) (result (car strs)))
        (if (null? rest)
            result
            (loop (cdr rest) (string-append result sep (car rest)))))))

;; === Commands ===

;; View all logs in buffer
(define (view-log)
  (buffer-create "*log*")
  (let ((entries (log-read-all)))
    (for-each
      (lambda (entry)
        (buffer-insert (log-format-entry entry))
        (buffer-insert "\n"))
      entries))
  (beginning-of-buffer))

;; View errors only
(define (view-log-errors)
  (buffer-create "*log-errors*")
  (let ((entries (log-errors)))
    (if (null? entries)
        (buffer-insert "No errors found.\n")
        (for-each
          (lambda (entry)
            (buffer-insert (log-format-entry entry))
            (buffer-insert "\n"))
          entries)))
  (beginning-of-buffer))

;; Clear log file
(define (log-clear)
  (write-file *log-path* "")
  (message "Log cleared"))

;; Tail log - show last N entries
(define (log-tail n)
  (let ((entries (log-read-all)))
    (let ((last-n (take-right entries n)))
      (buffer-create "*log*")
      (for-each
        (lambda (entry)
          (buffer-insert (log-format-entry entry))
          (buffer-insert "\n"))
        last-n)
      (end-of-buffer))))

;; Take last n elements from list
(define (take-right lst n)
  (let ((len (length lst)))
    (if (<= len n)
        lst
        (list-tail lst (- len n)))))
