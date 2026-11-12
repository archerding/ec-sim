//! ESC 内核（EtherCAT Slave Controller）。
//!
//! 职责：寄存器阵列、FMMU/SM、AL 状态机、EEPROM-SII、邮箱、分布式时钟（DC）、watchdog。
//! 约束（AGENTS.md §1）：ESC 层由核心**内置**，插件只做"对象字典声明 + 应用层逻辑"，
//! 且插件永不进入实时环。
//!
//! B1 只固化状态/错误类型与模块边界；寄存器地址、FMMU/SM 细节、AL 握手与错误位 ACK
//! 在 B5 按 **ESC Sec.II / ETG.1000** 逐项落地——不凭记忆写地址。

pub mod al;
pub mod error;
