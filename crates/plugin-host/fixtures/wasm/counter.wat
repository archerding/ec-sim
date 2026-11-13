;; EC-Sim 行为插件示例（WAT 源码，自写夹具；B43 / M3-W1）。
;;
;; 宿主契约见 crates/plugin-host/src/wasm.rs：
;;   env.read_pdo(offset) -> i32     越界 -1
;;   env.write_pdo(offset, value) -> i32
;;   env.log(ptr, len) -> i32
;;
;; 本插件：arena[0] 是宿主写入的命令字节（SM2 输出快照的第 1 字节），
;; tick 时把 arena[4]（SM3 输入快照的第 1 字节）设为 arena[0] + 1，并写一行日志。
;; 演示用，不代表任何真实设备行为；不进入实时环。
(module
  (import "env" "read_pdo" (func $read_pdo (param i32) (result i32)))
  (import "env" "write_pdo" (func $write_pdo (param i32 i32) (result i32)))
  (import "env" "log" (func $log (param i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "tick")
  (func (export "tick")
    (local $v i32)
    (local.set $v (call $read_pdo (i32.const 0)))
    (if (i32.ge_s (local.get $v) (i32.const 0))
      (then
        (drop (call $write_pdo (i32.const 4) (i32.add (local.get $v) (i32.const 1))))
        (drop (call $log (i32.const 0) (i32.const 4)))))))
