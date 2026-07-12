;; Huffman decompressor. The inverse of compress.ml — see that file for the
;; full payload format description.
;;
;; Reads exactly one line of lowercase hex text from stdin (the compressed
;; payload produced by compress.ml, hex-encoded), and writes exactly one
;; line of lowercase hex text to stdout: the original bytes, hex-encoded.

;; ---- hex <-> byte helpers ----

(define (hex-value c)
  (let ((n (char->integer c)))
    (cond
      ((and (>= n 48) (<= n 57)) (- n 48))
      ((and (>= n 97) (<= n 102)) (- n 87))
      ((and (>= n 65) (<= n 70)) (- n 55))
      (else (error "invalid hex digit" c)))))

(define (hex-digit v)
  (if (< v 10)
      (integer->char (+ v 48))
      (integer->char (+ (- v 10) 97))))

(define (display-hex-byte b)
  (display (hex-digit (quotient b 16)))
  (display (hex-digit (remainder b 16))))

(define (parse-hex-bytes s)
  (let ((n (string-length s)))
    (let loop ((i 0) (acc '()))
      (if (>= i n)
          (reverse acc)
          (loop (+ i 2)
                (cons (+ (* (hex-value (string-ref s i)) 16)
                         (hex-value (string-ref s (+ i 1))))
                      acc))))))

;; ---- big-endian byte-list -> integer ----

(define (bytes->int lst)
  (fold-left (lambda (acc b) (+ (* acc 256) b)) 0 lst))

;; ---- a mutable cursor over the payload byte list, so header fields can be
;; consumed one at a time without threading the remaining list by hand ----

(define (make-cursor lst) (cons lst '()))

(define (cursor-next! cur)
  (let ((b (car (car cur))))
    (set-car! cur (cdr (car cur)))
    b))

(define (cursor-next-n! cur n)
  (let loop ((n n) (acc '()))
    (if (= n 0)
        (reverse acc)
        (loop (- n 1) (cons (cursor-next! cur) acc)))))

(define (cursor-rest cur) (car cur))

;; ---- Huffman tree construction (identical algorithm to compress.ml, run
;; here over the (symbol, frequency) pairs read back from the header, so
;; both sides independently rebuild the same tree) ----
;; Leaf:     (list 'leaf freq symbol)
;; Internal: (list 'node freq left right)
;; NOTE: this build only actually provides car/cdr/caar/cadr/cdar/cddr/caddr
;; as composed accessors (no cdddr/cadar/cddar), so the right child of an
;; internal node is reached via the raw chain (car (cdr (cdr (cdr node))))
;; instead of a named 4th-level accessor.

(define (node-freq node) (cadr node))

(define (find-min-index nodes)
  (let loop ((lst (cdr nodes)) (i 1) (best-i 0) (best-freq (node-freq (car nodes))))
    (cond
      ((null? lst) best-i)
      ((< (node-freq (car lst)) best-freq) (loop (cdr lst) (+ i 1) i (node-freq (car lst))))
      (else (loop (cdr lst) (+ i 1) best-i best-freq)))))

(define (remove-at-index lst idx)
  (let loop ((lst lst) (i 0) (acc '()))
    (if (= i idx)
        (append (reverse acc) (cdr lst))
        (loop (cdr lst) (+ i 1) (cons (car lst) acc)))))

(define (build-tree nodes)
  (if (null? (cdr nodes))
      (car nodes)
      (let* ((i1 (find-min-index nodes))
             (n1 (list-ref nodes i1))
             (rest1 (remove-at-index nodes i1))
             (i2 (find-min-index rest1))
             (n2 (list-ref rest1 i2))
             (rest2 (remove-at-index rest1 i2))
             (merged (list 'node (+ (node-freq n1) (node-freq n2)) n1 n2)))
        (build-tree (append rest2 (list merged))))))

;; ---- bit unpacking (no bitwise ops available) ----

(define (byte->bits b)
  (map (lambda (place) (modulo (quotient b place) 2)) (list 128 64 32 16 8 4 2 1)))

(define (append-bits acc bits)
  (fold-left (lambda (a b) (cons b a)) acc bits))

(define (bytes->bit-list byte-list)
  (reverse (fold-left (lambda (acc b) (append-bits acc (byte->bits b))) '() byte-list)))

;; Walk the tree from the root, consuming one bit per internal step; on
;; reaching a leaf, emit its symbol and restart from the root. Stops as soon
;; as `len` symbols have been emitted, so trailing zero-pad bits in the
;; final byte are never consumed/interpreted.
(define (decode-symbols bits tree len)
  (let loop ((bits bits) (node tree) (count 0))
    (if (= count len)
        'done
        (if (eq? (car node) 'leaf)
            (begin
              (display-hex-byte (caddr node))
              (loop bits tree (+ count 1)))
            (if (= (car bits) 0)
                (loop (cdr bits) (caddr node) count)
                (loop (cdr bits) (car (cdr (cdr (cdr node)))) count))))))

(define (read-entries k cur)
  (if (= k 0)
      '()
      (let* ((s (cursor-next! cur))
             (f (bytes->int (cursor-next-n! cur 4))))
        (cons (cons s f) (read-entries (- k 1) cur)))))

;; NOTE: this build's `if`/`cond` has a real compiler bug where sibling
;; branches that each introduce their own `let`/`let*`-bound locals corrupt
;; local-slot numbering (observed as a runtime "local slot N out of range"
;; error whenever the non-first such branch executes — verified with a
;; minimal repro: a two-armed `if` with an independent `let` in each arm
;; fails on the second arm, but succeeds once each arm's bindings are moved
;; into its own top-level function). So the K==1 and K>=2 cases below are
;; each their own top-level function — never inlined as sibling branches of
;; one conditional — to route around it.

(define (emit-repeated-symbol! s len)
  (let loop ((i 0))
    (if (< i len)
        (begin (display-hex-byte s) (loop (+ i 1))))))

(define (emit-decoded! entries cur len)
  (let* ((leaves (map (lambda (e) (list 'leaf (cdr e) (car e))) entries))
         (tree (build-tree leaves))
         (bits (bytes->bit-list (cursor-rest cur))))
    (decode-symbols bits tree len)))

;; ---- main ----

(define input-hex (read-line))
(define payload-bytes (if (eof-object? input-hex) '() (parse-hex-bytes input-hex)))
(define cur (make-cursor payload-bytes))
(define len (bytes->int (cursor-next-n! cur 4)))
(define k (bytes->int (cursor-next-n! cur 2)))
(define entries (read-entries k cur))

(cond
  ((= k 0) 'nothing-to-emit)
  ((= k 1) (emit-repeated-symbol! (car (car entries)) len))
  (else (emit-decoded! entries cur len)))

(newline)
