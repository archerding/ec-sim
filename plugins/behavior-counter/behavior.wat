;; EC-Sim 行为插件参考画像（自写示例，B44 W2 子项 4）。
;;
;; 宿主契约见 crates/plugin-host/src/wasm.rs：
;;   env.read_pdo(offset) -> i32     越界 -1
;;   env.write_pdo(offset, value) -> i32   写 value 低 8 位
;;   env.log(ptr, len) -> i32            从插件线性内存追加日志
;; 导出 tick()，由 worker 线程调用，**不进入实时环**。
;;
;; 语义（PDO arena 布局 = [SM2 | SM3]）：
;;   SM2（4 字节，小端 u32）= 主站写入的计数值；
;;   SM3（4 字节，小端 u32）= SM2 + 1（插件每 tick 计算一次）。
;;   首次 tick 写一行日志 "counter tick"，证明 log 导入可用且不重复刷日志缓冲。
;; 完全自写，无第三方代码；仅作模拟器示例，不代表任何真实设备行为。
(module
  (import "env" "read_pdo" (func $read_pdo (param i32) (result i32)))
  (import "env" "write_pdo" (func $write_pdo (param i32 i32) (result i32)))
  (import "env" "log" (func $log (param i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 8) "counter tick")
  ;; 0 = 尚未记录首 tick 日志。
  (global $logged (mut i32) (i32.const 0))
  (func (export "tick")
    (local $v i32)
    ;; 读 SM2 的 4 个字节，拼成小端 u32；manifest 声明 SM2 恰为 4 字节，
    ;; 因此 arena 中 offset 0..=3 一定有效（越界时才可能返回 -1）。
    (local.set $v (call $read_pdo (i32.const 0)))
    (local.set $v (i32.or (local.get $v)
      (i32.shl (call $read_pdo (i32.const 1)) (i32.const 8))))
    (local.set $v (i32.or (local.get $v)
      (i32.shl (call $read_pdo (i32.const 2)) (i32.const 16))))
    (local.set $v (i32.or (local.get $v)
      (i32.shl (call $read_pdo (i32.const 3)) (i32.const 24))))
    (local.set $v (i32.add (local.get $v) (i32.const 1)))
    ;; 写 SM3：arena[4..=7]（= SM2 容量 4 之后的 SM3 段），小端 u32。
    (drop (call $write_pdo (i32.const 4)
      (i32.and (local.get $v) (i32.const 0xFF))))
    (drop (call $write_pdo (i32.const 5)
      (i32.and (i32.shr_u (local.get $v) (i32.const 8)) (i32.const 0xFF))))
    (drop (call $write_pdo (i32.const 6)
      (i32.and (i32.shr_u (local.get $v) (i32.const 16)) (i32.const 0xFF))))
    (drop (call $write_pdo (i32.const 7)
      (i32.and (i32.shr_u (local.get $v) (i32.const 24)) (i32.const 0xFF))))
    (if (i32.eqz (global.get $logged))
      (then
        ;; 长度 12 = "counter tick" 的字节数。
        (drop (call $log (i32.const 8) (i32.const 12)))
        (global.set $logged (i32.const 1))))))
