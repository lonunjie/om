;; SPDX-License-Identifier: MPL-2.0
;; Core implementation of the three OM WIT exports. No capability imports.
;; Fixed return area: result tag at byte 0, list/string pointer at 4, length at 8.
(core module $guest
  (memory (export "memory") 1 4)
  (data (i32.const 32) "empty surface")
  (global $next (mut i32) (i32.const 128))

  ;; Fixture inputs only. The canonical ABI passes fresh allocations here.
  (func (export "cabi_realloc") (param $old i32) (param i32) (param $align i32)
    (param $size i32) (result i32) (local $start i32)
    (if (local.get $old) (then unreachable))
    (local.set $start
      (i32.and (i32.add (global.get $next) (i32.sub (local.get $align) (i32.const 1)))
               (i32.sub (i32.const 0) (local.get $align))))
    (global.set $next (i32.add (local.get $start) (local.get $size)))
    (if (i32.gt_u (global.get $next) (i32.const 65536)) (then unreachable))
    (local.get $start))

  (func $result (param $tag i32) (param $ptr i32) (param $len i32) (result i32)
    (i32.store8 (i32.const 0) (local.get $tag))
    (i32.store (i32.const 4) (local.get $ptr))
    (i32.store (i32.const 8) (local.get $len))
    (i32.const 0))

  (func (export "init") (result i32)
    (call $result (i32.const 0) (i32.const 0) (i32.const 0)))

  ;; Echo event bytes, except for two synthetic one-byte resource probes.
  (func (export "on-event") (param $ptr i32) (param $len i32) (result i32)
    (if (i32.eq (local.get $len) (i32.const 1))
      (then
        (if (i32.eq (i32.load8_u (local.get $ptr)) (i32.const 255))
          (then (loop $spin (br $spin))))
        (if (i32.eq (i32.load8_u (local.get $ptr)) (i32.const 254))
          (then (return (call $grow))))))
    (call $result (i32.const 0) (local.get $ptr) (local.get $len)))

  ;; Record [initial pages, successful grows, final pages, denied grows].
  (func $grow (result i32) (local $attempts i32) (local $denied i32)
    (i32.store8 (i32.const 64) (memory.size))
    (loop $try
      (if (i32.eq (memory.grow (i32.const 1)) (i32.const -1))
        (then (local.set $denied (i32.add (local.get $denied) (i32.const 1)))))
      (local.set $attempts (i32.add (local.get $attempts) (i32.const 1)))
      (br_if $try (i32.lt_u (local.get $attempts) (i32.const 8))))
    (i32.store8 (i32.const 65) (i32.sub (local.get $attempts) (local.get $denied)))
    (i32.store8 (i32.const 66) (memory.size))
    (i32.store8 (i32.const 67) (local.get $denied))
    (call $result (i32.const 0) (i32.const 64) (i32.const 4)))

  (func (export "render") (param $ptr i32) (param $len i32) (result i32)
    (if (i32.eqz (local.get $len))
      (then (return (call $result (i32.const 1) (i32.const 32) (i32.const 13)))))
    (call $result (i32.const 0) (local.get $ptr) (local.get $len)))
)
