//! EtherCAT 邮箱协议最小实现（B7 子项 1）：邮箱头解析 + CoE SDO Abort 应答。
//!
//! 目的：让主站能通过 SM0/SM1 邮箱与从站跑通 **CoE SDO 数据链路**：
//! 对 `0x40`（initiate upload）请求从 [`crate::obj_dict`] 查表回 expedited upload 响应，
//! 对象不存在回 **SDO Abort**（`0x06020000`），写/其他请求回 Abort（`0x06010000`，只读）。
//! SDO Info 与分段传输暂不实现。验证方法与后续扩展见
//! `docs/spikes/spike-3-minimal-esc-responder.md`。
//!
//! 依据（两家**未修改**主站的实现，只读参考、未复制代码）：
//! - IgH 1.6.13 `master/mailbox.c`：邮箱头 6 字节 =
//!   `length(u16, 服务数据长度，不含头)`、`address(u16)`、`channel/priority(u8)`、`type(u8)`；
//!   发送用 FPWR 写 `configured_rx_mailbox_offset`；收包前读 `0x0808` 起 8 字节，
//!   取偏移 +5（= `0x080D`，SM1 Status）的 **bit3 = mailbox full**，再 FPRD
//!   `configured_tx_mailbox_offset` 取邮箱头 + 服务数据（`master/fsm_coe.c` 的
//!   `EC_MBOX_TYPE_COE`、`0x2 << 12` 等 CoE 字段）。
//! - SOEM `src/ec_main.c: ecx_mbxsend/ecx_mbxreceive`：同样 FPWR 写 SM0（`mbx_wo`）、
//!   轮询 `ECT_REG_SM1STAT`（= 0x080D）bit3、FPRD SM1 缓冲区（`mbx_ro`）；
//!   `include/soem/ec_type.h`：`ECT_MBXT_COE = 3`、CoE 服务命令在 u16 的 bit12–15
//!   （`ECT_COES_SDOREQ = 0x2`、`ECT_COES_SDORES = 0x3`、`ECT_COES_SDOINFO = 0x8`）、
//!   `ECT_SDO_UP_REQ = 0x40`、`ECT_SDO_ABORT = 0x80`；
//!   `src/ec_coe.c: ecx_SDOread()` 接受 `Command == ECT_SDO_ABORT` 的响应，
//!   并从 CoE 数据 +6 处读 4 字节 abort code。
//!
//! ⚠️ 尚未实现：SDO Info（`0x8`）与对象字典、邮箱计数位（counter，高 4 位，主站不校验）、
//! 邮箱重试/通信状态机、watchdog 与 SM 触发的邮箱中断。

use crate::Registers;

/// 邮箱头长度（字节）。
pub const MAILBOX_HEADER_LEN: usize = 6;

/// 接收邮箱（SM0）缓冲区起始地址：主站 → 从站（DL 用户内存内）。
pub const MAILBOX_RX_OFFSET: u16 = 0x1000;

/// 发送邮箱（SM1）缓冲区起始地址：从站 → 主站。
pub const MAILBOX_TX_OFFSET: u16 = 0x1080;

/// 默认邮箱容量（字节，两个方向相同）。
pub const MAILBOX_SIZE: usize = 0x80;

/// SM1 Status 寄存器地址（决定"发送邮箱有数据"的 mailbox full 位）。
pub const SM1_STATUS_ADDRESS: u16 =
    crate::sm::SYNC_MANAGER_ADDRESS + crate::sm::SYNC_MANAGER_SIZE + 5;

/// SM Status 的 mailbox full 位（bit3）。
pub const SM_STATUS_MAILBOX_FULL: u8 = 0x08;

/// 邮箱协议类型：CoE（CANopen over EtherCAT）。
pub const MAILBOX_TYPE_COE: u8 = 0x03;

/// CoE 服务命令（CoE 头 u16 的 bit12–15）：SDO 请求。
pub const COE_COMMAND_SDO_REQUEST: u16 = 0x2;
/// CoE 服务命令：SDO 响应。
pub const COE_COMMAND_SDO_RESPONSE: u16 = 0x3;

/// SDO 命令字节：Abort SDO transfer。
pub const SDO_COMMAND_ABORT: u8 = 0x80;

/// SDO 命令字节：initiate upload request（读对象）。
pub const SDO_COMMAND_INITIATE_UPLOAD_REQUEST: u8 = 0x40;

/// SDO Abort 码：对象不在对象字典中（ETG.1000.6 / CiA 301）。
pub const SDO_ABORT_OBJECT_NOT_EXIST: u32 = 0x0602_0000;

/// SDO Abort 码：不支持的访问（本实现只读，因此 Download 等请求回该码）。
pub const SDO_ABORT_UNSUPPORTED_ACCESS: u32 = 0x0601_0000;

/// 邮箱头（6 字节）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MailboxHeader {
    /// 服务数据长度（不含 6 字节邮箱头）。
    pub length: u16,
    /// 地址字段（IgH 写站地址、SOEM 写 0；从站回包原样带回）。
    pub address: u16,
    /// 通道与优先级。
    pub priority: u8,
    /// 协议类型（低 4 位；高 4 位是 mailbox counter，主站不校验）。
    pub mbox_type: u8,
}

impl MailboxHeader {
    /// 字节数。
    pub const BYTES: usize = MAILBOX_HEADER_LEN;

    /// 解析邮箱头；数据不足返回 [`None`]。
    #[must_use]
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < Self::BYTES {
            return None;
        }
        Some(Self {
            length: u16::from_le_bytes([data[0], data[1]]),
            address: u16::from_le_bytes([data[2], data[3]]),
            priority: data[4],
            mbox_type: data[5],
        })
    }

    /// 编码为 6 字节。
    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::BYTES] {
        let mut bytes = [0u8; Self::BYTES];
        bytes[0..2].copy_from_slice(&self.length.to_le_bytes());
        bytes[2..4].copy_from_slice(&self.address.to_le_bytes());
        bytes[4] = self.priority;
        bytes[5] = self.mbox_type;
        bytes
    }

    /// 协议类型（低 4 位）。
    #[must_use]
    pub fn protocol(self) -> u8 {
        self.mbox_type & 0x0F
    }
}

/// 构造 **SDO Abort 响应**的服务数据（CoE 头 + SDO Abort）。
#[must_use]
pub fn sdo_abort_payload(index: u16, subindex: u8, abort_code: u32) -> Vec<u8> {
    let mut service = Vec::with_capacity(10);
    service.extend_from_slice(&(COE_COMMAND_SDO_RESPONSE << 12).to_le_bytes());
    service.push(SDO_COMMAND_ABORT);
    service.extend_from_slice(&index.to_le_bytes());
    service.push(subindex);
    service.extend_from_slice(&abort_code.to_le_bytes());
    service
}

/// 对 CoE SDO 请求构造响应服务数据（expedited upload 或 Abort）。
///
/// - `0x40`（initiate upload）且对象在 [`crate::obj_dict`] 中 → SDO upload response
///   （命令 0x43/0x47/0x4B/0x4F 视数据长度，值 4 字节补齐）；
/// - `0x40` 但对象不存在 → Abort `0x06020000`；
/// - 其他 SDO 命令（Download / Abort 等）→ Abort `0x06010000`（本实现只读）；
/// - 非 CoE SDO 请求（如 SDO Info `0x8`）→ [`None`]（暂不应答）。
#[must_use]
pub fn sdo_response(service: &[u8]) -> Option<Vec<u8>> {
    // CoE 头 2 字节 + SDO 命令/索引/子索引 4 字节。
    if service.len() < 6 {
        return None;
    }
    let coe = u16::from_le_bytes([service[0], service[1]]);
    if coe >> 12 != COE_COMMAND_SDO_REQUEST {
        return None;
    }
    let sdo_command = service[2];
    // SDO 请求布局：CoE 头(2) + Command(1) + Index(2) + SubIndex(1) + data…
    let index = u16::from_le_bytes([service[3], service[4]]);
    let subindex = service[5];
    if sdo_command != SDO_COMMAND_INITIATE_UPLOAD_REQUEST {
        return Some(sdo_abort_payload(
            index,
            subindex,
            SDO_ABORT_UNSUPPORTED_ACCESS,
        ));
    }
    match crate::obj_dict::read_object(index, subindex) {
        Some(entry) => match entry.sdo_upload_payload() {
            Some((_, sdo_payload)) => {
                let mut response = Vec::with_capacity(10);
                response.extend_from_slice(&(COE_COMMAND_SDO_RESPONSE << 12).to_le_bytes());
                response.extend_from_slice(&sdo_payload);
                Some(response)
            }
            None => Some(sdo_abort_payload(
                index,
                subindex,
                SDO_ABORT_OBJECT_NOT_EXIST,
            )),
        },
        None => Some(sdo_abort_payload(
            index,
            subindex,
            SDO_ABORT_OBJECT_NOT_EXIST,
        )),
    }
}

/// 把服务数据封装成完整邮箱报文（头 + 数据）；超过 [`MAILBOX_SIZE`] 返回 [`None`]。
#[must_use]
pub fn build_mailbox(header: MailboxHeader, service: &[u8]) -> Option<Vec<u8>> {
    if MAILBOX_HEADER_LEN + service.len() > MAILBOX_SIZE {
        return None;
    }
    let header = MailboxHeader {
        length: u16::try_from(service.len()).ok()?,
        ..header
    };
    let mut message = Vec::with_capacity(MAILBOX_HEADER_LEN + service.len());
    message.extend_from_slice(&header.to_bytes());
    message.extend_from_slice(service);
    Some(message)
}

/// 从 SM1 Status 寄存器值判断发送邮箱是否有待主站取走的数据。
#[must_use]
pub fn mailbox_full(status: u8) -> bool {
    status & SM_STATUS_MAILBOX_FULL != 0
}

/// 把 SM1 Status 置为"邮箱有数据"（供模拟器内部写只读状态位）。
pub fn set_mailbox_full(registers: &mut Registers) {
    registers.set_internal(SM1_STATUS_ADDRESS, &[SM_STATUS_MAILBOX_FULL]);
}

/// 清除 SM1 Status 的 mailbox full 位（主站取走数据后模拟从站已消费）。
pub fn clear_mailbox_full(registers: &mut Registers) {
    registers.set_internal(SM1_STATUS_ADDRESS, &[0]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip_and_protocol_masking() {
        let header = MailboxHeader {
            length: 0x0010,
            address: 0x1001,
            priority: 0,
            mbox_type: MAILBOX_TYPE_COE | 0x30, // 高 4 位 = mailbox counter
        };
        let bytes = header.to_bytes();
        assert_eq!(&bytes[..2], &[0x10, 0x00]);
        assert_eq!(&bytes[2..4], &[0x01, 0x10]);
        assert_eq!(MailboxHeader::parse(&bytes), Some(header));
        assert_eq!(header.protocol(), MAILBOX_TYPE_COE);
        assert_eq!(MailboxHeader::parse(&bytes[..5]), None);
    }

    #[test]
    fn build_mailbox_checks_capacity() {
        let header = MailboxHeader {
            length: 0,
            address: 0,
            priority: 0,
            mbox_type: MAILBOX_TYPE_COE,
        };
        let service = [0u8; 10];
        let message = build_mailbox(header, &service).expect("fits");
        assert_eq!(message.len(), MAILBOX_HEADER_LEN + 10);
        assert_eq!(&message[..2], &[0x0A, 0x00], "长度 = 服务数据长度");
        let too_big = [0u8; MAILBOX_SIZE];
        assert_eq!(build_mailbox(header, &too_big), None);
    }

    #[test]
    fn sdo_upload_response_for_known_object() {
        // SOEM `ecx_SDOread()` 读 0x1C12:01（RxPDO assign，2 字节）：
        // 邮箱头 10、CoE `0x2 << 12`、Command 0x40、Index 0x1C12、SubIndex 01。
        let mut service = vec![0x00, 0x20, 0x40]; // CoE 0x2000 + initiate upload request
        service.extend_from_slice(&0x1C12u16.to_le_bytes());
        service.push(0x01);
        service.extend_from_slice(&[0u8; 4]);
        let response = sdo_response(&service).expect("SDO request is answerable");
        // CoE 响应命令 0x3 << 12 = 0x3000（小端 [0x00, 0x30]）。
        assert_eq!(&response[..2], &[0x00, 0x30]);
        // expedited upload，2 字节 → 命令 0x4B。
        assert_eq!(response[2], 0x4B);
        assert_eq!(u16::from_le_bytes([response[3], response[4]]), 0x1C12);
        assert_eq!(response[5], 0x01);
        assert_eq!(u16::from_le_bytes([response[6], response[7]]), 0x1600);
        assert_eq!(&response[8..10], &[0, 0], "未用的数据字节补零");
        assert_eq!(response.len(), 10);
    }

    #[test]
    fn sdo_abort_for_unknown_object() {
        let mut service = vec![0x00, 0x20, 0x40];
        service.extend_from_slice(&0x2000u16.to_le_bytes());
        service.push(0x00);
        service.extend_from_slice(&[0u8; 4]);
        let response = sdo_response(&service).expect("answerable");
        assert_eq!(response[2], SDO_COMMAND_ABORT);
        assert_eq!(u16::from_le_bytes([response[3], response[4]]), 0x2000);
        assert_eq!(
            u32::from_le_bytes([response[6], response[7], response[8], response[9]]),
            SDO_ABORT_OBJECT_NOT_EXIST
        );
    }

    #[test]
    fn sdo_download_request_gets_read_only_abort() {
        // Download initiate request（SDO 命令 0x20）：本实现只读 → Abort 0x06010000。
        let mut service = vec![0x00, 0x20, 0x20];
        service.extend_from_slice(&0x1C12u16.to_le_bytes());
        service.push(0x01);
        service.extend_from_slice(&[0u8; 4]);
        let response = sdo_response(&service).expect("answerable");
        assert_eq!(response[2], SDO_COMMAND_ABORT);
        assert_eq!(
            u32::from_le_bytes([response[6], response[7], response[8], response[9]]),
            SDO_ABORT_UNSUPPORTED_ACCESS
        );
    }

    #[test]
    fn non_sdo_requests_are_not_answered() {
        // SDO Info（CoE 命令 0x8）暂不实现 → 不应答，主站超时后回退 SII。
        let sdo_info = [0x00, 0x80, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(sdo_response(&sdo_info), None);
        // 过短服务数据。
        assert_eq!(sdo_response(&[0x00, 0x20, 0x40]), None);
    }

    #[test]
    fn mailbox_full_bit_helpers() {
        let mut registers = Registers::new();
        assert!(!mailbox_full(0));
        set_mailbox_full(&mut registers);
        let status = registers
            .read_u8(SM1_STATUS_ADDRESS)
            .expect("SM1 status in range");
        assert!(mailbox_full(status));
        clear_mailbox_full(&mut registers);
        assert_eq!(registers.read_u8(SM1_STATUS_ADDRESS), Some(0));
    }
}
