;; Huffman compressor.
;;
;; Reads exactly one line of lowercase hex text from stdin (the raw bytes of
;; the file to compress, hex-encoded two-chars-per-byte, e.g. via
;; `xxd -p -c 0`), and writes exactly one line of lowercase hex text to
;; stdout: the compressed payload, hex-encoded the same way.
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

(define (parse-hex-bytes s)
  (let ((n (string-length s)))
    (let loop ((i 0) (acc '()))
      (if (>= i n)
          (reverse acc)
          (loop (+ i 2)
                (cons (+ (* (hex-value (string-ref s i)) 16)
                         (hex-value (string-ref s (+ i 1))))
                      acc))))))

;; ---- big-endian integer -> byte-list ----

(define (int->bytes n width)
  (let loop ((n n) (w width) (acc '()))
    (if (= w 0)
        acc
        (loop (quotient n 256) (- w 1) (cons (remainder n 256) acc)))))

;; ---- frequency table ----

(define (count-frequencies data)
  (let ((h (make-hash)))
    (for-each (lambda (b) (hash-set! h b (+ 1 (hash-ref h b 0)))) data)
    h))

;; symbols are byte values 0..255; scanning that fixed range in order gives
;; us the ascending-symbol-order list the format requires, with no need for
;; a general-purpose sort.
(define (symbols-ascending freq-hash)
  (let loop ((i 0) (acc '()))
    (if (> i 255)
        (reverse acc)
        (loop (+ i 1)
              (if (hash-has-key? freq-hash i) (cons i acc) acc)))))

(define (entries-for symbols freq)
  (if (null? symbols)
      '()
      (append (cons (car symbols) (int->bytes (hash-ref freq (car symbols)) 4))
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

(define (walk-tree node path codes)
  (if (eq? (car node) 'leaf)
      (hash-set! codes (caddr node) (reverse path))
      (begin
        (walk-tree (caddr node) (cons 0 path) codes)
        (walk-tree (car (cdr (cdr (cdr node)))) (cons 1 path) codes))))

;; ---- bit packing (no bitwise ops available) ----

;; cons each bit of `bits` onto `acc` in order; after a final `reverse` this
;; yields the bits in their original left-to-right order without any
;; quadratic append-on-a-growing-list behaviour.
(define (append-bits acc bits)
  (fold-left (lambda (a b) (cons b a)) acc bits))

(define (build-bit-list data codes)
  (reverse (fold-left (lambda (acc sym) (append-bits acc (hash-ref codes sym))) '() data)))

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
      (let* ((leaves (map (lambda (s) (list 'leaf (hash-ref freq s) s)) symbols))
             (tree (build-tree leaves))
             (codes (make-hash)))
        (walk-tree tree '() codes)
        (pack-bits (build-bit-list data codes)))
      '()))

(for-each display-hex-byte header)
(for-each display-hex-byte payload)
(newline)
