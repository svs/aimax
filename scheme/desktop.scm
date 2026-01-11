;;; desktop.scm - Save and restore open buffers
;;;
;;; Saves the list of open files to ~/.aimax/desktop
;;; Restores them on startup.

(define *desktop-file*
  (string-append (expand-path "~" ".") "/.aimax/desktop"))

;; Save current desktop (list of open files)
(define (desktop-save)
  (let ((paths (buffer-file-paths)))
    (if (null? paths)
        #t  ; Nothing to save
        (begin
          (write-file *desktop-file* (paths->string paths))
          (message "Desktop saved")
          #t))))

;; Convert list of paths to newline-separated string
(define (paths->string paths)
  (if (null? paths)
      ""
      (if (null? (cdr paths))
          (car paths)
          (string-append (car paths) "\n" (paths->string (cdr paths))))))

;; Restore desktop (open saved files)
(define (desktop-restore)
  (let ((content (read-file *desktop-file*)))
    (if (string=? content "")
        #t  ; No desktop to restore
        (begin
          (open-paths-from-string content)
          (message "Desktop restored")
          #t))))

;; Open files from newline-separated string
(define (open-paths-from-string content)
  (let ((paths (split-lines content)))
    (open-each paths)))

;; Open each path in list
(define (open-each paths)
  (if (null? paths)
      #t
      (begin
        (let ((p (car paths)))
          (if (not (string=? p ""))
              (buffer-open p)
              #t))
        (open-each (cdr paths)))))

;; Split string by newlines using substring
(define (split-lines str)
  (split-lines-helper str 0 0 '()))

(define (split-lines-helper str start idx result)
  (if (>= idx (string-length str))
      (reverse (cons (substring str start idx) result))
      (if (string=? (substring str idx (+ idx 1)) "\n")
          (split-lines-helper str (+ idx 1) (+ idx 1)
                              (cons (substring str start idx) result))
          (split-lines-helper str start (+ idx 1) result))))
