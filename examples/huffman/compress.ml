;; Huffman compressor.
;;
;; Reads exactly one line of hex text from stdin (upper- or lowercase digits
;; both accepted; the raw bytes of the file to compress, hex-encoded
;; two-chars-per-byte, e.g. via `xxd -p -c 0`), and writes exactly one line
;; of lowercase hex text to stdout: the compressed payload, hex-encoded the
;; same way.
;;
;; Compressed payload byte layout (before hex-encoding):
;;   4 bytes  original length, big-endian u32
;;   2 bytes  K = number of distinct byte values present, big-endian u16
;;   K * 5 bytes  (symbol byte, 4-byte big-endian frequency) in ascending
;;                symbol order
;;   then, only if K >= 2: the Huffman bitstream, MSB-first, zero-padded in
;;   the final byte. K == 0 (empty input) or K == 1 (single repeated byte)
;;   carry no further payload bytes.

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

;; Walks the string's characters as a list (one single O(n) string->list
;; pass) rather than repeated string-ref-by-index calls: string-ref is
;; O(current index) (it scans from the start each time, since strings are
;; indexed by Unicode scalar value, not by byte offset), so indexing forward
;; through the whole string in a loop would be O(n^2) -- exactly the
;; scaling cliff warden security-review msg #53 measured (misattributed
;; there to count-frequencies's hash-table use, which was a real but
;; secondary inefficiency; this loop was the dominant cost).
(define (parse-hex-bytes s)
  (let loop ((chars (string->list s)) (acc '()))
    (if (null? chars)
        (reverse acc)
        (loop (cddr chars)
              (cons (+ (* (hex-value (car chars)) 16)
                       (hex-value (cadr chars)))
                    acc)))))

;; ---- big-endian integer -> byte-list ----

(define (int->bytes n width)
  (let loop ((n n) (w width) (acc '()))
    (if (= w 0)
        acc
        (loop (quotient n 256) (- w 1) (cons (remainder n 256) acc)))))

;; ---- frequency table ----

;; Symbols are byte values 0..255, a small fixed range -- a 256-slot vector
;; indexed directly by symbol gives O(1) truly-constant-time counting,
;; unlike a general-purpose hash table. Part of the fix for warden
;; security-review msg #53's superlinear-performance finding -- see
;; parse-hex-bytes and build-bit-list below for the two effects that turned
;; out to dominate that finding's actual measurements.
(define (count-frequencies data)
  (let ((freq (make-vector 256 0)))
    (for-each (lambda (b) (vector-set! freq b (+ 1 (vector-ref freq b)))) data)
    freq))

;; symbols are byte values 0..255; scanning that fixed range in order gives
;; us the ascending-symbol-order list the format requires, with no need for
;; a general-purpose sort.
(define (symbols-ascending freq)
  (let loop ((i 0) (acc '()))
    (if (> i 255)
        (reverse acc)
        (loop (+ i 1)
              (if (> (vector-ref freq i) 0) (cons i acc) acc)))))

(define (entries-for symbols freq)
  (if (null? symbols)
      '()
      (append (cons (car symbols) (int->bytes (vector-ref freq (car symbols)) 4))
              (entries-for (cdr symbols) freq))))

;; ---- Huffman tree construction ----
;; Leaf:     (list 'leaf freq symbol)
;; Internal: (list 'node freq left right)
;; freq is always (cadr node) for either shape.
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

;; Deterministic construction: repeatedly take the first strictly-minimum
;; node, then the first strictly-minimum of what remains, merge them
;; (first-found becomes left, second-found becomes right), and append the
;; merged node to the end of the list. Both compress.ml and decompress.ml
;; run this exact procedure over the same (symbol, frequency) pairs, so the
;; tree never needs to be transmitted.
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

;; ---- code generation ----

;; codes is a 256-slot vector, one entry per possible symbol (unused slots
;; stay '() and are never read, since build-bit-list only looks up symbols
;; that are actually present in data) -- same rationale as count-frequencies
;; above.
(define (walk-tree node path codes)
  (if (eq? (car node) 'leaf)
      (vector-set! codes (caddr node) (reverse path))
      (begin
        (walk-tree (caddr node) (cons 0 path) codes)
        (walk-tree (car (cdr (cdr (cdr node)))) (cons 1 path) codes))))

;; ---- bit packing (no bitwise ops available) ----

;; Plain recursion, deliberately NOT fold-left+lambda (as an earlier version
;; of this function used): every closure this build creates captures its
;; ENTIRE enclosing call frame eagerly, regardless of which of that frame's
;; variables the closure body actually references, and every closure
;; creation registers with the cycle-safety collector (EnvGc). A closure
;; created once per data element -- as `(lambda (a b) (cons b a))` passed to
;; an inner fold-left was, one call per byte of input -- means its captured
;; frame (this function's own `acc`/`bits` parameters) gets tracked too; a
;; later sweep, triggered while that call's closure is still the only live
;; registration, must then walk that frame's `acc` -- which by then holds
;; the entire encoded bitstream so far -- checking its whole (purely
;; acyclic, closure-free) pair chain for nested carriers. Repeated once per
;; input byte, that is a real, measured O(n^2) cost (warden security-review
;; msg #53's "count-frequencies" diagnosis was a real but secondary
;; inefficiency, since it was fixed above without resolving the actual
;; slowdown; this closure-per-element pattern, now removed, was the
;; dominant one). Plain recursion creates no closures at all, sidestepping
;; the whole mechanism.
(define (prepend-bits bits acc)
  (if (null? bits)
      acc
      (prepend-bits (cdr bits) (cons (car bits) acc))))

(define (build-bit-list data codes)
  (let loop ((data data) (acc '()))
    (if (null? data)
        (reverse acc)
        (loop (cdr data) (prepend-bits (vector-ref codes (car data)) acc)))))

(define (pack-bits bits)
  (let loop ((bits bits) (acc 0) (count 0) (out '()))
    (if (null? bits)
        (reverse (if (= count 0) out (cons (* acc (expt 2 (- 8 count))) out)))
        (let ((acc2 (+ (* acc 2) (car bits)))
              (count2 (+ count 1)))
          (if (= count2 8)
              (loop (cdr bits) 0 0 (cons acc2 out))
              (loop (cdr bits) acc2 count2 out))))))

;; ---- main ----

(define input-hex (read-line))
(define data (if (eof-object? input-hex) '() (parse-hex-bytes input-hex)))
(define len (length data))
(define freq (count-frequencies data))
(define symbols (symbols-ascending freq))
(define k (length symbols))

(define header (append (append (int->bytes len 4) (int->bytes k 2)) (entries-for symbols freq)))

(define payload
  (if (>= k 2)
      (let* ((leaves (map (lambda (s) (list 'leaf (vector-ref freq s) s)) symbols))
             (tree (build-tree leaves))
             (codes (make-vector 256 '())))
        (walk-tree tree '() codes)
        (pack-bits (build-bit-list data codes)))
      '()))

(for-each display-hex-byte header)
(for-each display-hex-byte payload)
(newline)
