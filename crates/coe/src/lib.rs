//! CoE（CANopen over EtherCAT）子协议。
//!
//! 规划（AGENTS.md §6 B7）：加速 SDO 上传/下载、0x1C12/0x1C13/0x1600/0x1A00 与
//! SDO Abort 码；先**不做** SDO Info 与 Complete Access（SII 对应能力位清 0）。
//!
//! B1 仅占位模块边界：对象字典与邮箱帧的落地在 B6（SII 生成器）与 B7 之间推进。
//! 依赖方向：`coe` → `esc-core`（邮箱/寄存器）与 `wire`（帧），不反向。
