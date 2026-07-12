;; Huffman decompressor. The inverse of compress.ml — see that file for the
;; full payload format description.
;;
;; Reads exactly one line of hex text from stdin (upper- or lowercase digits
;; both accepted; the compressed payload produced by compress.ml,
;; hex-encoded), and writes exactly one line of lowercase hex text to
;; stdout: the original bytes, hex-encoded.

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
;; pass) rather than repeated string-ref-by-index calls -- see
;; compress.ml's own copy of this function for why: string-ref is O(current
;; index), so indexing forward through the whole string in a loop is
;; O(n^2).
(define (parse-hex-bytes s)
  (let loop ((chars (string->list s)) (acc '()))
    (if (null? chars)
        (reverse acc)
        (loop (cddr chars)
              (cons (+ (* (hex-value (car chars)) 16)
                       (hex-value (cadr chars)))
                    acc)))))

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

;; A top-level (define ...), deliberately NOT a named let -- empirically
;; confirmed (not just theorized): a named let here creates a closure whose
;; captured frame includes `cur`, the mutable cursor pair whose car
;; ultimately points at the whole remaining payload list. Once registered
;; with the cycle-safety collector, a later sweep -- triggered by anything
;; else in the program, arbitrarily far away in execution, such as
;; bytes->bit-list's own per-element closure creation -- must walk that
;; entire remaining list looking for nested carriers. A plain top-level
;; recursive function creates no closure here at all, so nothing captures
;; `cur`.
(define (cursor-next-n! cur n)
  (if (= n 0)
      '()
      (cons (cursor-next! cur) (cursor-next-n! cur (- n 1)))))

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

;; Plain recursion, deliberately NOT fold-left+lambda -- see compress.ml's
;; own copy of this function for the full rationale: a closure created once
;; per input byte (as an inner fold-left's lambda argument was) captures
;; its whole enclosing frame, including this function's own `acc`
;; parameter -- which by then holds the entire decoded bit-list so far --
;; and registers that capture with the cycle-safety collector; a sweep
;; triggered while that call's closure is still live must walk `acc`'s
;; whole pair chain looking for nested carriers, which is the real O(n^2)
;; cost warden security-review msg #53 measured. Plain recursion creates no
;; closures at all, sidestepping the whole mechanism.
(define (prepend-bits bits acc)
  (if (null? bits)
      acc
      (prepend-bits (cdr bits) (cons (car bits) acc))))

;; A top-level (define ...), deliberately NOT a named let: byte->bits
;; itself creates one closure per call (its own internal map+lambda), and
;; -- empirically confirmed, not just theorized -- a *named let*'s own
;; self-referential letrec closure is itself a tracked cycle-collector
;; candidate, so pairing it with a per-iteration closure-creating call
;; while this function's own accumulator grows reproduces the same O(n^2)
;; sweep cost the prior fold-left version had, even though this loop's own
;; body creates no closures directly. An ordinary top-level recursive
;; function has no such self-referential closure to track, and measures as
;; linear.
(define (bytes->bit-list-from byte-list acc)
  (if (null? byte-list)
      (reverse acc)
      (bytes->bit-list-from (cdr byte-list) (prepend-bits (byte->bits (car byte-list)) acc))))

(define (bytes->bit-list byte-list)
  (bytes->bit-list-from byte-list '()))

;; Walk the tree from the root, consuming one bit per internal step; on
;; reaching a leaf, emit its symbol and restart from the root. Stops as soon
;; as `len` symbols have been emitted, so trailing zero-pad bits in the
;; final byte are never consumed/interpreted.
;;
;; A top-level (define ...), deliberately NOT a named let -- as with every
;; other rewrite in this file, a named let here would create a closure
;; whose captured frame includes `bits` itself, the full multi-million-
;; element decoded bitstream; the first sweep to fire anywhere near this
;; call would then have to walk the whole thing.
(define (decode-symbols-loop bits tree node count len)
  (if (= count len)
      'done
      (if (eq? (car node) 'leaf)
          (begin
            (display-hex-byte (caddr node))
            (decode-symbols-loop bits tree tree (+ count 1) len))
          (if (= (car bits) 0)
              (decode-symbols-loop (cdr bits) tree (caddr node) count len)
              (decode-symbols-loop (cdr bits) tree (car (cdr (cdr (cdr node)))) count len)))))

(define (decode-symbols bits tree len)
  (decode-symbols-loop bits tree tree 0 len))

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

;; A top-level (define ...), deliberately NOT map+lambda: entries is only
;; ever K (<=256) elements, but the lambda passed to map here would be
;; created while `cur` -- the mutable cursor pair whose car points at the
;; whole remaining, n-sized payload list -- is still one of this call's own
;; parameters, hence part of the closure's eagerly-captured frame. Plain
;; recursion creates no closure, so nothing captures `cur` here.
(define (entries->leaves entries)
  (if (null? entries)
      '()
      (cons (list 'leaf (cdr (car entries)) (car (car entries)))
            (entries->leaves (cdr entries)))))

(define (emit-decoded! entries cur len)
  (let* ((leaves (entries->leaves entries))
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
