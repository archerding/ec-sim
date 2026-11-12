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

/// SDO 命令字节：expedited upload request（读对象）。
pub const SDO_COMMAND_INITIATE_UPLOAD_REQUEST: u8 = 0x40;

/// SDO 命令字节：upload segment request（分段上传的后续段请求；低 4 位为 toggle）。
pub const SDO_COMMAND_UPLOAD_SEGMENT_REQUEST: u8 = 0x60;

/// SDO 命令字节：initiate upload 响应（非 expedited、带长度指示，随后可分段）。
pub const SDO_COMMAND_UPLOAD_NORMAL_RESPONSE: u8 = 0x41;

/// 分段上传响应中每段的固定数据字节数（CiA 301：7 字节，末段用 unused 位补齐）。
pub const SDO_SEGMENT_DATA_LEN: usize = 7;

/// 分段上传请求（[`parse_upload_segment_request`] 的返回值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadSegmentRequest {
    /// 对象索引。
    pub index: u16,
    /// 子索引。
    pub subindex: u8,
    /// toggle 位（0 或 1；主站每段翻转）。
    pub toggle: u8,
}

/// 解析 SDO upload segment request（命令字节 `0x60 | toggle<<4`）。
///
/// 布局按两家未修改主站：CoE 头 2 字节 + Command + Index(2) + SubIndex(1) + 1 字节数据
/// （SOEM `src/ec_coe.c: ecx_SDOread()` 分段循环；IgH `master/fsm_coe.c`
/// `ec_fsm_coe_up_prepare_segment_request()`）。
#[must_use]
pub fn parse_upload_segment_request(service: &[u8]) -> Option<UploadSegmentRequest> {
    if service.len() < 6 {
        return None;
    }
    let coe = u16::from_le_bytes([service[0], service[1]]);
    if coe >> 12 != COE_COMMAND_SDO_REQUEST {
        return None;
    }
    let command = service[2];
    // bit7..5 = 0b011（upload segment），bit1（download 方向）与 bit0（保留）必须为 0；
    // 只有 bit4 是 toggle。
    if command & 0xE3 != SDO_COMMAND_UPLOAD_SEGMENT_REQUEST {
        return None;
    }
    Some(UploadSegmentRequest {
        index: u16::from_le_bytes([service[3], service[4]]),
        subindex: service[5],
        toggle: (command >> 4) & 0x01,
    })
}

/// 构造 **分段上传的 initiate 响应**（非 expedited、带 4 字节长度指示，随后跟首段数据）。
///
/// 布局（两家主站一致）：CoE 头 2 + Command 0x41 + Index 2 + SubIndex 1 + Size(u32) +
/// 首段数据。IgH `fsm_coe.c` 从 `data + 10` 取首段；SOEM `ec_coe.c` 从 `ldata[1]` 取。
#[must_use]
pub fn sdo_upload_initiate_service(
    index: u16,
    subindex: u8,
    total_size: usize,
    first_chunk: &[u8],
) -> Vec<u8> {
    let mut service = Vec::with_capacity(10 + first_chunk.len());
    service.extend_from_slice(&(COE_COMMAND_SDO_RESPONSE << 12).to_le_bytes());
    service.push(SDO_COMMAND_UPLOAD_NORMAL_RESPONSE);
    service.extend_from_slice(&index.to_le_bytes());
    service.push(subindex);
    let total = u32::try_from(total_size).unwrap_or(u32::MAX);
    service.extend_from_slice(&total.to_le_bytes());
    service.extend_from_slice(first_chunk);
    service
}

/// 构造 **分段上传的 segment 响应**：`Command = toggle<<4 | unused<<1 | last` + 7 字节数据。
///
/// `chunk` 最多 [`SDO_SEGMENT_DATA_LEN`] 字节，末段不足时用 0 补齐并在 `unused` 位标出。
#[must_use]
pub fn sdo_upload_segment_service(toggle: u8, chunk: &[u8], last: bool) -> Vec<u8> {
    let used = chunk.len().min(SDO_SEGMENT_DATA_LEN);
    let unused = u8::try_from(SDO_SEGMENT_DATA_LEN - used).unwrap_or(0);
    let command = ((toggle & 0x01) << 4) | (unused << 1) | u8::from(last);
    let mut service = Vec::with_capacity(3 + SDO_SEGMENT_DATA_LEN);
    service.extend_from_slice(&(COE_COMMAND_SDO_RESPONSE << 12).to_le_bytes());
    service.push(command);
    let mut payload = [0u8; SDO_SEGMENT_DATA_LEN];
    payload[..used].copy_from_slice(&chunk[..used]);
    service.extend_from_slice(&payload);
    service
}

/// SDO Abort 码：对象不在对象字典中（ETG.1000.6 / CiA 301）。
pub const SDO_ABORT_OBJECT_NOT_EXIST: u32 = 0x0602_0000;

/// SDO Abort 码：不支持的访问（本实现只读，因此 Download 等请求回该码）。
pub const SDO_ABORT_UNSUPPORTED_ACCESS: u32 = 0x0601_0000;

/// SDO 命令字节：download segment request（分段下载的后续段；`0x20 | toggle<<4 | unused<<1 | last`）。
pub const SDO_COMMAND_DOWNLOAD_SEGMENT_REQUEST: u8 = 0x20;

/// 分段下载的 initiate download 请求（**normal、非 expedited**，命令字节 `0x21`）。
///
/// 布局按 IgH `master/fsm_coe.c: ec_fsm_coe_prepare_down_start()`：
/// CoE 头 2 + Command + Index 2 + SubIndex 1 + Size(u32) + 首段数据（最多到邮箱容量）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitiateDownload {
    /// 对象索引。
    pub index: u16,
    /// 子索引。
    pub subindex: u8,
    /// 数据总长度（u32）。
    pub total_size: u32,
    /// 首段数据（可能小于总长度）。
    pub data: Vec<u8>,
}

/// 解析 download initiate 请求（normal、size indicated，命令字节必须恰为 `0x21`）。
///
/// ⚠️ 命令字节 `0x20|...` 与 segment 请求共享 bit7-5，因此这里只接受 `0x21`；
/// 从站在有进行中的分段下载时应优先按 segment 请求解析（见 `esc-node`）。
#[must_use]
pub fn parse_initiate_download(service: &[u8]) -> Option<InitiateDownload> {
    if service.len() < 10 {
        return None;
    }
    let coe = u16::from_le_bytes([service[0], service[1]]);
    if coe >> 12 != COE_COMMAND_SDO_REQUEST {
        return None;
    }
    if service[2] != (SDO_COMMAND_DOWNLOAD_SEGMENT_REQUEST | 0x01) {
        return None;
    }
    Some(InitiateDownload {
        index: u16::from_le_bytes([service[3], service[4]]),
        subindex: service[5],
        total_size: u32::from_le_bytes([service[6], service[7], service[8], service[9]]),
        data: service[10..].to_vec(),
    })
}

/// 分段下载请求的解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentDownload {
    /// toggle 位。
    pub toggle: u8,
    /// 末段未使用的字节数（0..7）。
    pub unused: u8,
    /// 是否末段。
    pub last: bool,
    /// 本段数据（长度 = 7 - unused）。
    pub data: Vec<u8>,
}

/// 解析 download segment 请求（命令字节 `0x20 | toggle<<4 | unused<<1 | last`）。
///
/// 布局按 IgH `master/fsm_coe.c: ec_fsm_coe_down_prepare_segment_request()`：
/// CoE 头 2 + Command + 最多 7 字节数据（不足 7 字节时补零并用 `unused` 标注）。
#[must_use]
pub fn parse_segment_download(service: &[u8]) -> Option<SegmentDownload> {
    if service.len() < 3 {
        return None;
    }
    let coe = u16::from_le_bytes([service[0], service[1]]);
    if coe >> 12 != COE_COMMAND_SDO_REQUEST {
        return None;
    }
    let command = service[2];
    if command & 0xE0 != SDO_COMMAND_DOWNLOAD_SEGMENT_REQUEST {
        return None;
    }
    let toggle = (command >> 4) & 0x01;
    let unused = (command >> 1) & 0x07;
    let last = command & 0x01 != 0;
    let used = SDO_SEGMENT_DATA_LEN.saturating_sub(usize::from(unused));
    let payload = &service[3..];
    if payload.len() < used {
        return None;
    }
    Some(SegmentDownload {
        toggle,
        unused,
        last,
        data: payload[..used].to_vec(),
    })
}

/// 构造分段下载的 segment 响应：CoE 0x3<<12 + `0x20 | toggle<<4`（IgH 校验 bit7-5 == 0x01）。
#[must_use]
pub fn sdo_download_segment_ok_service(toggle: u8) -> Vec<u8> {
    let mut service = Vec::with_capacity(3);
    service.extend_from_slice(&(COE_COMMAND_SDO_RESPONSE << 12).to_le_bytes());
    service.push(SDO_COMMAND_DOWNLOAD_SEGMENT_REQUEST | ((toggle & 0x01) << 4));
    service
}

/// SDO Abort 码：尝试写只读对象（CiA 301）。
pub const SDO_ABORT_READ_ONLY: u32 = 0x0601_0002;

/// SDO Abort 码：SDO 协议超时（例如分段上传的段请求没有进行中的传输，CiA 301）。
pub const SDO_ABORT_PROTOCOL_TIMEOUT: u32 = 0x0504_0000;

/// Toggle 位未按 CiA 301 交替（0x05030000，分段传输的 toggle 错误）。
pub const SDO_ABORT_TOGGLE: u32 = 0x0503_0000;

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

/// 解析 **Complete Access 上传请求**（命令字节  x50：initiate upload + CA 位 bit4）。
///
/// ETG.1000.6：CA 上传一次返回对象从子索引 0 到最大子索引的全部内容（每个子索引按其
/// 自身数据类型定长拼接，**不做 4 字节对齐填充**）。
pub fn parse_complete_access_upload(service: &[u8]) -> Option<u16> {
    if service.len() < 6 {
        return None;
    }
    let coe = u16::from_le_bytes([service[0], service[1]]);
    if coe >> 12 != COE_COMMAND_SDO_REQUEST {
        return None;
    }
    if service[2] != 0x50 {
        return None;
    }
    Some(u16::from_le_bytes([service[3], service[4]]))
}

/// 解析 **Complete Access 下载请求**（normal、size indicated，命令字节  x31：
/// initiate download + CA 位 bit4）。返回 (index, 数据)；数据长度必须与服务长度一致。
///
/// ⚠️ 仅支持单帧 CA 下载（数据全部在 initiate 请求里）；更大的 CA 下载应分多帧，
/// 当前回 Abort 0x05040000（见 Responder::handle_download_complete_access）。
pub fn parse_complete_access_download(service: &[u8]) -> Option<(u16, &[u8])> {
    if service.len() < 10 {
        return None;
    }
    let coe = u16::from_le_bytes([service[0], service[1]]);
    if coe >> 12 != COE_COMMAND_SDO_REQUEST {
        return None;
    }
    if service[2] != 0x31 {
        return None;
    }
    let index = u16::from_le_bytes([service[3], service[4]]);
    let size = u32::from_le_bytes([service[6], service[7], service[8], service[9]]) as usize;
    if service.len() < 10 + size {
        return None;
    }
    Some((index, &service[10..10 + size]))
}

/// 解析 **Complete Access 下载 initiate 请求**（normal、size indicated，命令字节 `0x31`）。
///
/// 返回 `(index, total_size, 首帧携带的数据)`；CA 数据超过首帧容量时，剩余部分由后续
/// segment 请求（`0x20|toggle<<4|last`）携带，与普通分段下载一致。
pub fn parse_complete_access_download_initiate(service: &[u8]) -> Option<(u16, u32, &[u8])> {
    if service.len() < 10 {
        return None;
    }
    let coe = u16::from_le_bytes([service[0], service[1]]);
    if coe >> 12 != COE_COMMAND_SDO_REQUEST {
        return None;
    }
    if service[2] != 0x31 {
        return None;
    }
    let index = u16::from_le_bytes([service[3], service[4]]);
    let size = u32::from_le_bytes([service[6], service[7], service[8], service[9]]);
    Some((index, size, &service[10..]))
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
    fn parsers_never_panic_on_arbitrary_bytes() {
        // 邮箱服务数据来自网络：任意字节输入到各解析入口都不得 panic。
        let mut state = 0xC0FF_EE11u32;
        for len in 0..48usize {
            for _ in 0..48 {
                let mut service = vec![0u8; len];
                for byte in &mut service {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    *byte = (state >> 24) as u8;
                }
                let _ = parse_sdo_request(&service);
                let _ = parse_initiate_download(&service);
                let _ = parse_segment_download(&service);
                let _ = parse_upload_segment_request(&service);
                let _ = parse_complete_access_upload(&service);
                let _ = parse_complete_access_download_initiate(&service);
                let _ = parse_sdo_info_request(&service);
                let _ = MailboxHeader::parse(&service);
            }
        }
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
    fn segmented_download_requests_and_response_layout() {
        // initiate download（normal、size indicated，命令字节恰为 0x21）。
        let mut service = vec![0x00, 0x20, 0x21];
        service.extend_from_slice(&0x2100u16.to_le_bytes());
        service.push(0x00);
        service.extend_from_slice(&20u32.to_le_bytes());
        service.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let initiate = parse_initiate_download(&service).expect("initiate download");
        assert_eq!(initiate.index, 0x2100);
        assert_eq!(initiate.subindex, 0);
        assert_eq!(initiate.total_size, 20);
        assert_eq!(initiate.data, vec![1, 2, 3, 4, 5, 6, 7, 8]);

        // segment download 请求：0x20 | toggle<<4 | unused<<1 | last。
        // toggle=1、unused=4（3 字节有效）、last=1 → 0x20|0x10|0x08|0x01 = 0x39。
        let segment_request = [0x00, 0x20, 0x39, 9, 10, 11, 0, 0, 0, 0];
        let segment = parse_segment_download(&segment_request).expect("segment download");
        assert_eq!(segment.toggle, 1);
        assert_eq!(segment.unused, 4);
        assert!(segment.last);
        assert_eq!(segment.data, vec![9, 10, 11]);

        // segment 响应：CoE 0x3<<12 + 0x20 | toggle<<4（IgH 校验 bit7-5 == 0x01）。
        assert_eq!(sdo_download_segment_ok_service(0), vec![0x00, 0x30, 0x20]);
        assert_eq!(sdo_download_segment_ok_service(1), vec![0x00, 0x30, 0x30]);
    }

    #[test]
    fn segmented_upload_request_and_responses_layout() {
        // 段请求：CoE 0x2<<12 + Command 0x60|toggle + Index + SubIndex + 1 字节。
        assert_eq!(
            parse_upload_segment_request(&[0x00, 0x20, 0x70, 0x08, 0x10, 0x00, 0x00]),
            Some(UploadSegmentRequest {
                index: 0x1008,
                subindex: 0x00,
                toggle: 1
            })
        );
        assert_eq!(
            parse_upload_segment_request(&[0x00, 0x20, 0x60, 0x08, 0x10, 0x00, 0x00]),
            Some(UploadSegmentRequest {
                index: 0x1008,
                subindex: 0x00,
                toggle: 0
            })
        );
        // segment download 响应（0x22/0x62 类）与过短请求不是 upload segment request。
        assert_eq!(
            parse_upload_segment_request(&[0x00, 0x20, 0x22, 0x08, 0x10, 0x00, 0x00]),
            None
        );
        assert_eq!(parse_upload_segment_request(&[0x00, 0x20, 0x60]), None);

        // initiate 响应（非 expedited）：Command 0x41 + index/subindex + u32 长度 + 首段数据。
        let data = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let service = sdo_upload_initiate_service(0x1008, 0, data.len(), &data);
        assert_eq!(&service[..2], &[0x00, 0x30], "CoE 服务 = SDO 响应");
        assert_eq!(service[2], SDO_COMMAND_UPLOAD_NORMAL_RESPONSE);
        assert_eq!(u16::from_le_bytes([service[3], service[4]]), 0x1008);
        assert_eq!(service[5], 0x00);
        assert_eq!(
            u32::from_le_bytes([service[6], service[7], service[8], service[9]]),
            10,
            "长度字段在 +6（IgH 从 data+6 读；SOEM 从 ldata[0] 读）"
        );
        assert_eq!(&service[10..], &data, "首段数据从 +10 开始");

        // 段响应：Command = toggle<<4 | unused<<1 | last；数据固定 7 字节（末段补零）。
        let full = sdo_upload_segment_service(0, &[1, 2, 3, 4, 5, 6, 7], false);
        assert_eq!(full[2], 0x00, "满 7 字节、非末段");
        assert_eq!(&full[3..10], &[1, 2, 3, 4, 5, 6, 7]);
        let last = sdo_upload_segment_service(1, &[8, 9, 10], true);
        assert_eq!(last[2], 0x10 | (4 << 1) | 1, "toggle=1、unused=4、last=1");
        assert_eq!(&last[3..6], &[8, 9, 10]);
        assert_eq!(&last[6..10], &[0, 0, 0, 0]);
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
