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

/// SDO Abort 码：尝试写只读对象（CiA 301）。
pub const SDO_ABORT_READ_ONLY: u32 = 0x0601_0002;

/// SDO 命令字节：expedited download（1 字节数据）。
const SDO_COMMAND_DOWNLOAD_1: u8 = 0x2F;
/// SDO 命令字节：expedited download（2 字节数据）。
const SDO_COMMAND_DOWNLOAD_2: u8 = 0x2B;
/// SDO 命令字节：expedited download（3 字节数据）。
const SDO_COMMAND_DOWNLOAD_3: u8 = 0x27;
/// SDO 命令字节：expedited download（4 字节数据）。
const SDO_COMMAND_DOWNLOAD_4: u8 = 0x23;
/// SDO 命令字节：download initiate response（成功）。
pub const SDO_COMMAND_DOWNLOAD_RESPONSE: u8 = 0x60;

/// 解析后的 CoE SDO 请求（[`parse_sdo_request`] 的返回值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdoRequest<'a> {
    /// initiate upload（读对象）。
    Upload {
        /// 对象索引。
        index: u16,
        /// 子索引。
        subindex: u8,
    },
    /// expedited download（写对象，1..=4 字节）。
    Download {
        /// 对象索引。
        index: u16,
        /// 子索引。
        subindex: u8,
        /// 写入的数据（小端原样）。
        data: &'a [u8],
    },
    /// 其他 CoE SDO 命令（如 master 发起的 Abort、分段传输）——不应答。
    Other,
}

/// 解析 CoE SDO 请求服务数据；非 CoE SDO 请求返回 [`None`]（SDO Info 等暂不实现）。
#[must_use]
pub fn parse_sdo_request(service: &[u8]) -> Option<SdoRequest<'_>> {
    // CoE 头 2 字节 + SDO 命令/索引/子索引 4 字节。
    if service.len() < 6 {
        return None;
    }
    let coe = u16::from_le_bytes([service[0], service[1]]);
    if coe >> 12 != COE_COMMAND_SDO_REQUEST {
        return None;
    }
    let command = service[2];
    // SDO 请求布局：CoE 头(2) + Command(1) + Index(2) + SubIndex(1) + data…
    let index = u16::from_le_bytes([service[3], service[4]]);
    let subindex = service[5];
    if command == SDO_COMMAND_INITIATE_UPLOAD_REQUEST {
        return Some(SdoRequest::Upload { index, subindex });
    }
    let size = match command {
        SDO_COMMAND_DOWNLOAD_1 => 1,
        SDO_COMMAND_DOWNLOAD_2 => 2,
        SDO_COMMAND_DOWNLOAD_3 => 3,
        SDO_COMMAND_DOWNLOAD_4 => 4,
        _ => return Some(SdoRequest::Other),
    };
    if service.len() < 6 + size {
        return Some(SdoRequest::Other);
    }
    Some(SdoRequest::Download {
        index,
        subindex,
        data: &service[6..6 + size],
    })
}

/// 构造 SDO **expedited upload 响应**服务数据（CoE 头 + 命令 + 索引/子索引 + 4 字节值）。
#[must_use]
pub fn sdo_upload_service(index: u16, subindex: u8, value: &[u8]) -> Vec<u8> {
    let size = value.len().min(4);
    // CiA 301 expedited upload: 0x40 | exp(0x02) | (4-size)<<2 | size_indicated(0x01)。
    let command = 0x40u8 | 0x02 | (((4 - size) as u8) << 2) | 0x01;
    let mut service = Vec::with_capacity(10);
    service.extend_from_slice(&(COE_COMMAND_SDO_RESPONSE << 12).to_le_bytes());
    service.push(command);
    service.extend_from_slice(&index.to_le_bytes());
    service.push(subindex);
    let mut padded = [0u8; 4];
    padded[..size].copy_from_slice(&value[..size]);
    service.extend_from_slice(&padded);
    service
}

/// 构造 SDO **download 成功响应**服务数据（命令 0x60 + 索引/子索引，共 6 字节）。
///
/// 两家主站都只校验 CoE 响应类型、命令高 3 位（0x60 >> 5 = 3）与索引/子索引：
/// IgH `master/fsm_coe.c: ec_fsm_coe_down_response()`、SOEM `src/ec_coe.c: ecx_SDOwrite()`。
#[must_use]
pub fn sdo_download_ok_service(index: u16, subindex: u8) -> Vec<u8> {
    let mut service = Vec::with_capacity(6);
    service.extend_from_slice(&(COE_COMMAND_SDO_RESPONSE << 12).to_le_bytes());
    service.push(SDO_COMMAND_DOWNLOAD_RESPONSE);
    service.extend_from_slice(&index.to_le_bytes());
    service.push(subindex);
    service
}

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

// ---------------------------------------------------------------------------
// CoE SDO Information 服务（B7 子项：`ethercat sdos` / `slaveinfo -sdo` 所需）
// ---------------------------------------------------------------------------
//
// 线格式按两家**未修改**主站的实现逐字节复核（只读参考、未复制代码）：
// - IgH `master/fsm_coe.c`：服务字 `0x8 << 12`（SDO Info），请求布局
//   `[CoE 2][Opcode 1][Reserved 1][Fragments 2][payload]`；
//   Get OD List 响应首帧索引表偏移 8（后帧 6，见 `index_list_offset`）、
//   Get OD 响应名称在 +12、Get Entry Description 响应名称在 +16；
// - SOEM `src/ec_coe.c` + `include/soem/ec_type.h`：`ECT_COES_SDOINFO = 0x08`、
//   `ECT_GET_ODLIST_REQ/RES = 0x01/0x02`、`ECT_GET_OD_REQ/RES = 0x03/0x04`、
//   `ECT_GET_OE_REQ/RES = 0x05/0x06`、`ECT_SDOINFO_ERROR = 0x07`；
//   `ecx_readODlist/ecx_readODdescription/ecx_readOEsingle` 的字段偏移与上面一致。

/// CoE 服务命令（CoE 头 u16 的 bit12–15）：SDO Information。
pub const COE_COMMAND_SDO_INFO: u16 = 0x8;

/// SDO Info 操作码：Get OD List 请求。
pub const SDO_INFO_GET_OD_LIST_REQUEST: u8 = 0x01;
/// SDO Info 操作码：Get OD List 响应。
pub const SDO_INFO_GET_OD_LIST_RESPONSE: u8 = 0x02;
/// SDO Info 操作码：Get OD（对象描述）请求。
pub const SDO_INFO_GET_OD_REQUEST: u8 = 0x03;
/// SDO Info 操作码：Get OD 响应。
pub const SDO_INFO_GET_OD_RESPONSE: u8 = 0x04;
/// SDO Info 操作码：Get Entry Description 请求。
pub const SDO_INFO_GET_ENTRY_REQUEST: u8 = 0x05;
/// SDO Info 操作码：Get Entry Description 响应。
pub const SDO_INFO_GET_ENTRY_RESPONSE: u8 = 0x06;
/// SDO Info 操作码：错误响应。
pub const SDO_INFO_ERROR_RESPONSE: u8 = 0x07;

/// SDO Info 首个响应帧在索引表前多出的 2 字节字段。
///
/// IgH `ec_fsm_coe_dict_response()`（`index_list_offset = first_segment ? 8 : 6`）
/// 与 SOEM `ecx_readODlist()`（首帧 `n = (length - (6 + 2)) / 2`）都**跳过**它、
/// 直接按偏移 8 读索引；ETG.1000.6 对该字段的定义未从公开资料核实（⚠️ 待验证），
/// 本实现填 0 并保持两家主站解析一致。
pub const SDO_INFO_OD_LIST_FIRST_FRAME_PAD: [u8; 2] = [0x00, 0x00];

/// 解析后的 CoE SDO Info 请求。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdoInfoRequest {
    /// Get OD List（`deliver_all` 来自请求第 6–7 字节，两家主站都发 1）。
    OdList {
        /// 请求中的对象筛选参数（1 = 全部对象）。
        deliver_all: u16,
    },
    /// Get OD：读某个索引的对象描述。
    ObjectDescription {
        /// 对象索引。
        index: u16,
    },
    /// Get Entry Description：读某索引/子索引的条目描述。
    EntryDescription {
        /// 对象索引。
        index: u16,
        /// 子索引。
        subindex: u8,
        /// 请求要返回的附加信息位（访问权限/对象类别/PDO，SOEM 发 0x07）。
        value_info: u8,
    },
}

/// 构造 SDO Info 响应的公共头（CoE 服务字 + 操作码 + 保留 + 分段计数）。
fn sdo_info_response_header(opcode: u8) -> Vec<u8> {
    let mut service = Vec::with_capacity(6);
    service.extend_from_slice(&(COE_COMMAND_SDO_INFO << 12).to_le_bytes());
    service.push(opcode);
    service.push(0x00); // Reserved
    service.extend_from_slice(&[0x00, 0x00]); // Fragments left（单帧响应 = 0）
    service
}

/// 解析 CoE SDO Info 请求服务数据；非 SDO Info 或未知操作码返回 [`None`]。
#[must_use]
pub fn parse_sdo_info_request(service: &[u8]) -> Option<SdoInfoRequest> {
    if service.len() < 6 {
        return None;
    }
    let coe = u16::from_le_bytes([service[0], service[1]]);
    if coe >> 12 != COE_COMMAND_SDO_INFO {
        return None;
    }
    let opcode = service[2] & 0x7F;
    match opcode {
        SDO_INFO_GET_OD_LIST_REQUEST => {
            if service.len() < 8 {
                return None;
            }
            Some(SdoInfoRequest::OdList {
                deliver_all: u16::from_le_bytes([service[6], service[7]]),
            })
        }
        SDO_INFO_GET_OD_REQUEST => {
            if service.len() < 8 {
                return None;
            }
            Some(SdoInfoRequest::ObjectDescription {
                index: u16::from_le_bytes([service[6], service[7]]),
            })
        }
        SDO_INFO_GET_ENTRY_REQUEST => {
            if service.len() < 10 {
                return None;
            }
            Some(SdoInfoRequest::EntryDescription {
                index: u16::from_le_bytes([service[6], service[7]]),
                subindex: service[8],
                value_info: service[9],
            })
        }
        _ => None,
    }
}

/// 构造 **Get OD List 响应**服务数据（索引列表，单帧；两家主站按偏移 8 读）。
#[must_use]
pub fn build_sdo_info_od_list(indices: &[u16]) -> Vec<u8> {
    let mut service = sdo_info_response_header(SDO_INFO_GET_OD_LIST_RESPONSE);
    service.extend_from_slice(&SDO_INFO_OD_LIST_FIRST_FRAME_PAD);
    for index in indices {
        service.extend_from_slice(&index.to_le_bytes());
    }
    service
}

/// 构造 **Get OD 响应**服务数据（对象描述 + 名称）。
///
/// 字段：index +6、data_type +8、max_subindex +10、object_code +11、名称 +12
/// （IgH `fsm_coe.c` 与 SOEM `ec_coe.c` 一致）。
#[must_use]
pub fn build_sdo_info_object_description(
    index: u16,
    data_type: u16,
    max_subindex: u8,
    object_code: u8,
    name: &str,
) -> Vec<u8> {
    let mut service = sdo_info_response_header(SDO_INFO_GET_OD_RESPONSE);
    service.extend_from_slice(&index.to_le_bytes());
    service.extend_from_slice(&data_type.to_le_bytes());
    service.push(max_subindex);
    service.push(object_code);
    service.extend_from_slice(name.as_bytes());
    service
}

/// 构造 **Get Entry Description 响应**服务数据（条目描述 + 名称）。
///
/// 字段：index +6、subindex +8、value_info +9、data_type +10、bit_length +12、
/// access +14、名称 +16（IgH `fsm_coe.c` 与 SOEM `ec_coe.c` 一致）。
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn build_sdo_info_entry_description(
    index: u16,
    subindex: u8,
    value_info: u8,
    data_type: u16,
    bit_length: u16,
    access: u16,
    name: &str,
) -> Vec<u8> {
    let mut service = sdo_info_response_header(SDO_INFO_GET_ENTRY_RESPONSE);
    service.extend_from_slice(&index.to_le_bytes());
    service.push(subindex);
    service.push(value_info);
    service.extend_from_slice(&data_type.to_le_bytes());
    service.extend_from_slice(&bit_length.to_le_bytes());
    service.extend_from_slice(&access.to_le_bytes());
    service.extend_from_slice(name.as_bytes());
    service
}

/// 构造 **SDO Info 错误响应**（Get OD/Entry 请求了不存在的对象或条目）。
///
/// 布局按两家主站的错误判断：`0x8<<12` + 操作码 0x07，abort code 从 +6 开始
/// （IgH `fsm_coe.c` 读 `data + 6`；SOEM `ecx_SDOinfoerror` 读 `ldata[0]` = 服务数据 +6）。
#[must_use]
pub fn build_sdo_info_error(abort_code: u32) -> Vec<u8> {
    let mut service = sdo_info_response_header(SDO_INFO_ERROR_RESPONSE);
    service.extend_from_slice(&abort_code.to_le_bytes());
    service
}

/// 把服务数据封装成完整邮箱报文（头 + 数据）；超过 [`MAILBOX_SIZE`] 返回 [`None`]。#[must_use]
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
    fn parse_sdo_upload_request_and_build_response() {
        // SOEM `ecx_SDOread()` 读 0x1C12:01（RxPDO assign，2 字节）：
        // 邮箱头 10、CoE `0x2 << 12`、Command 0x40、Index 0x1C12、SubIndex 01。
        let mut service = vec![0x00, 0x20, 0x40]; // CoE 0x2000 + initiate upload request
        service.extend_from_slice(&0x1C12u16.to_le_bytes());
        service.push(0x01);
        service.extend_from_slice(&[0u8; 4]);
        assert_eq!(
            parse_sdo_request(&service),
            Some(SdoRequest::Upload {
                index: 0x1C12,
                subindex: 0x01
            })
        );

        let response = sdo_upload_service(0x1C12, 0x01, &[0x00, 0x16]);
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
    fn parse_sdo_download_request_and_build_ok_response() {
        // SOEM `ecx_SDOwrite()` 写 0x1C12:01：命令 0x2B（expedited，2 字节）。
        let mut service = vec![0x00, 0x20, 0x2B];
        service.extend_from_slice(&0x1C12u16.to_le_bytes());
        service.push(0x01);
        service.extend_from_slice(&[0x00, 0x16]);
        assert_eq!(
            parse_sdo_request(&service),
            Some(SdoRequest::Download {
                index: 0x1C12,
                subindex: 0x01,
                data: &[0x00, 0x16]
            })
        );

        // 1/3/4 字节命令宽度。
        for (command, size) in [(0x2F, 1), (0x27, 3), (0x23, 4)] {
            let mut service = vec![0x00, 0x20, command];
            service.extend_from_slice(&0x1600u16.to_le_bytes());
            service.push(0x01);
            service.extend_from_slice(&[0xAA; 4]);
            match parse_sdo_request(&service) {
                Some(SdoRequest::Download { data, .. }) => assert_eq!(data.len(), size),
                other => panic!("expected download, got {other:?}"),
            }
        }

        // 成功响应：6 字节，命令 0x60 + 索引/子索引。
        let response = sdo_download_ok_service(0x1C12, 0x01);
        assert_eq!(&response[..2], &[0x00, 0x30]);
        assert_eq!(response[2], SDO_COMMAND_DOWNLOAD_RESPONSE);
        assert_eq!(u16::from_le_bytes([response[3], response[4]]), 0x1C12);
        assert_eq!(response[5], 0x01);
        assert_eq!(response.len(), 6);
    }

    #[test]
    fn sdo_abort_payload_encodes_index_and_code() {
        let response = sdo_abort_payload(0x2000, 0x00, SDO_ABORT_OBJECT_NOT_EXIST);
        assert_eq!(&response[..2], &[0x00, 0x30]);
        assert_eq!(response[2], SDO_COMMAND_ABORT);
        assert_eq!(u16::from_le_bytes([response[3], response[4]]), 0x2000);
        assert_eq!(
            u32::from_le_bytes([response[6], response[7], response[8], response[9]]),
            SDO_ABORT_OBJECT_NOT_EXIST
        );
    }

    #[test]
    fn non_sdo_requests_are_not_parsed() {
        // SDO Info（CoE 命令 0x8）暂不实现 → None（不应答）。
        let sdo_info = [0x00, 0x80, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(parse_sdo_request(&sdo_info), None);
        // 过短服务数据。
        assert_eq!(parse_sdo_request(&[0x00, 0x20, 0x40]), None);
        // master 发起的 Abort（SDO 命令 0x80）→ Other（不应答）。
        let master_abort = [0x00, 0x20, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(parse_sdo_request(&master_abort), Some(SdoRequest::Other));
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

    #[test]
    fn sdo_info_requests_are_parsed_like_both_masters_send_them() {
        // IgH `fsm_coe.c: ec_fsm_coe_prepare_dict()`：0x8<<12、opcode 0x01、deliver all = 1。
        let od_list = [0x00, 0x80, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00];
        assert_eq!(
            parse_sdo_info_request(&od_list),
            Some(SdoInfoRequest::OdList { deliver_all: 1 })
        );

        // IgH `ec_fsm_coe_dict_prepare_desc()`：opcode 0x03，index 在 +6。
        let od = [0x00, 0x80, 0x03, 0x00, 0x00, 0x00, 0x12, 0x1C];
        assert_eq!(
            parse_sdo_info_request(&od),
            Some(SdoInfoRequest::ObjectDescription { index: 0x1C12 })
        );

        // SOEM `ecx_readOEsingle()`：opcode 0x05，index +6、subindex +8、value_info +9。
        let entry = [0x00, 0x80, 0x05, 0x00, 0x00, 0x00, 0x12, 0x1C, 0x01, 0x07];
        assert_eq!(
            parse_sdo_info_request(&entry),
            Some(SdoInfoRequest::EntryDescription {
                index: 0x1C12,
                subindex: 0x01,
                value_info: 0x07
            })
        );

        // 非 SDO Info（SDO 请求 0x2<<12）、未知操作码、过短 → None。
        assert_eq!(
            parse_sdo_info_request(&[0x00, 0x20, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00]),
            None
        );
        assert_eq!(
            parse_sdo_info_request(&[0x00, 0x80, 0x7F, 0x00, 0x00, 0x00, 0x00, 0x00]),
            None
        );
        assert_eq!(parse_sdo_info_request(&[0x00, 0x80, 0x01]), None);
    }

    #[test]
    fn sdo_info_responses_match_master_field_offsets() {
        // Get OD List：索引从 +8 开始（IgH index_list_offset = 8 / SOEM 首帧 (len-8)/2）。
        let response = build_sdo_info_od_list(&[0x1018, 0x1C00, 0x1600]);
        assert_eq!(&response[..2], &[0x00, 0x80], "CoE 服务 = SDO Info");
        assert_eq!(response[2], SDO_INFO_GET_OD_LIST_RESPONSE);
        assert_eq!(&response[4..6], &[0, 0], "fragments left = 0");
        assert_eq!(&response[6..8], &SDO_INFO_OD_LIST_FIRST_FRAME_PAD);
        assert_eq!(response.len(), 8 + 3 * 2);
        assert_eq!(
            u16::from_le_bytes([response[8], response[9]]),
            0x1018,
            "首帧第一个索引在 +8"
        );

        // Get OD：index +6、data_type +8、max_subindex +10、object_code +11、名称 +12。
        let description = crate::obj_dict::object_description(0x1C12).expect("object");
        let response = build_sdo_info_object_description(
            description.index,
            description.data_type,
            description.max_subindex,
            description.object_code,
            description.name,
        );
        assert_eq!(response[2], SDO_INFO_GET_OD_RESPONSE);
        assert_eq!(u16::from_le_bytes([response[6], response[7]]), 0x1C12);
        assert_eq!(response[10], 1, "max_subindex");
        assert_eq!(
            response[11],
            crate::obj_dict::object_code::ARRAY,
            "object_code"
        );
        assert_eq!(&response[12..], b"RxPDO assign".as_slice());

        // Get Entry Description：index +6、subindex +8、value_info +9、data_type +10、
        // bit_length +12、access +14、名称 +16。
        let entry = crate::obj_dict::entry_description(0x1C12, 1).expect("entry");
        let response = build_sdo_info_entry_description(
            entry.index,
            entry.subindex,
            entry.value_info,
            entry.data_type,
            entry.bit_length,
            entry.access,
            entry.name,
        );
        assert_eq!(response[2], SDO_INFO_GET_ENTRY_RESPONSE);
        assert_eq!(u16::from_le_bytes([response[6], response[7]]), 0x1C12);
        assert_eq!(response[8], 1);
        assert_eq!(
            u16::from_le_bytes([response[10], response[11]]),
            crate::obj_dict::data_type::UNSIGNED16
        );
        assert_eq!(u16::from_le_bytes([response[12], response[13]]), 16);
        assert_eq!(
            u16::from_le_bytes([response[14], response[15]]),
            crate::obj_dict::access::READ_WRITE_PREOP
        );
        assert_eq!(&response[16..], b"RxPDO assign".as_slice());

        // 错误响应：opcode 0x07，abort code 从 +6 开始（IgH 读 data+6 / SOEM 读 ldata[0]）。
        let error = build_sdo_info_error(SDO_ABORT_OBJECT_NOT_EXIST);
        assert_eq!(error[2], SDO_INFO_ERROR_RESPONSE);
        assert_eq!(
            u32::from_le_bytes([error[6], error[7], error[8], error[9]]),
            SDO_ABORT_OBJECT_NOT_EXIST
        );
    }
}
