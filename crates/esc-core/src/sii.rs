//! 最小 SII（Slave Information Interface / EEPROM）镜像。
//!
//! 仅供 B2b 最小 ESC 应答器使用：固定区字段 + 字符串/通用/SM/TxPDO/RxPDO 类别 + `0xFFFF` 终止。
//! 字段位置以**未修改主站实际读取的 word 偏移**为准（可用源码复核）：
//! - IgH `master/fsm_slave_scan.c`：`sii_words + 0x0004`（alias）、`+0x0008`（Vendor）、
//!   `+0x000A`（Product）、`+0x000C`（Revision）、`+0x000E`（Serial）、
//!   `+0x0014..0x001C`（邮箱）、类别链从 `EC_FIRST_SII_CATEGORY_OFFSET = 0x40` 开始；
//! - SOEM `src/ec_config.c` / `include/soem/ec_type.h`：`ECT_SII_MANUF = 0x0008`、
//!   `ECT_SII_ID = 0x000A`、`ECT_SII_REV = 0x000C`、`ECT_SII_SER = 0x000E`、
//!   `ECT_SII_START = 0x0040`（`ecx_siifind` 用 `ECT_SII_START << 1` 转字节地址）。
//!
//! 已实现（B6 子项，2026-11-12 19:26 班）：**SM 类别 0x0029**（4 个 Sync Manager：
//! SM0/SM1 = mailbox、SM2 = 8 位输出、SM3 = 8 位输入）、**TxPDO 类别 0x0032**
//! （`0x1A00`，8 位输入）与 **RxPDO 类别 0x0033**（`0x1600`，8 位输出）。
//! 已实现（B7 子项 1，2026-11-12 19:36 班）：固定区 **0x0014–0x001C 的 mailbox 参数**
//! （boot/std 两套 rx/tx 偏移与 128 字节容量、`mailbox_protocols = 0x0001` = CoE），
//! SM0 = 0x1000 / SM1 = 0x1080（control 0x26/0x22，与 IgH `fsm_slave_config.c` 默认邮箱
//! 及 SOEM `EC_DEFAULTMBXSM0/1` 一致）。
//! 字节布局按两家**未修改**主站的解析代码逐字节复核（只读参考、未复制代码）：
//! - IgH `master/slave.c`：`ec_slave_fetch_sii_syncs()` 每条 8 字节
//!   （start u16、length u16、control u8、status u8、enable u8、pdi_control u8）；
//!   `ec_slave_fetch_sii_pdos()` PDO 头 8 字节（index u16、entry_count u8、sync_index u8、
//!   保留 u8、name_index u8、保留 u16），每条映射 8 字节（index u16、subindex u8、
//!   name_index u8、保留、bit_length u8、保留），类别 0x0032 = TxPDO（输入方向）、
//!   0x0033 = RxPDO（输出方向），见 `master/fsm_slave_scan.c` 的类别分发；
//! - SOEM `src/ec_main.c`：`ecx_siiSM()` 按类别大小 word 数 /4 得 SM 数、每条 8 字节；
//!   `ecx_siiPDO()` 同样按 8 字节 PDO 头 / 8 字节映射项遍历，读取 +5 处的 bit_length；
//!   `src/ec_config.c: ecx_map_sii()` 在无 mailbox（CoE 不可用）时回退到 SII PDO 类别，
//!   据此设置 SM2/SM3 的长度与类型并映射 PDO（8 位 → 1 字节）。
//!
//! 尚未实现（B6/B7 其余）：FMMU 类别 0x0028、DC 类别、ESI XML → SII 生成器、
//! 对象字典与 SDO Info（邮箱目前只回 SDO Abort，见 [`crate::mailbox`]）。
//!
//! 固定区校验（ETG.1000.6 §5.4 table 16，已按两家未修改主站复核）：
//! - **word 0x0007：SII CRC-8**，覆盖 word 0x0000–0x0006（14 字节），多项式
//!   `x^8 + x^2 + x + 1`（0x07）、初值 0xFF、MSB-first（无反转、无 xorout）；
//! - **word 0x003E：EEPROM 大小**，单位 128 字节（值 0 = 128 字节）；
//! - **word 0x003F：SII 版本**，值为 1。
//!
//! 复核证据：SOEM `test/linux/eepromtool/eepromtool.c` 的 `SIIcrc()` 对前 14 字节求 CRC
//! 并写入 word 0x0007（`eeprom_writealias()`），`-i` 输出同时打印 "Checksum"（word 0x0007）
//! 与 "calculated"；IgH `tool/sii_crc.cpp` 的多项式/初值与此一致
//! （`pycrc --width=8 --poly=0x07 --reflect-in=0 --xor-in=0xff`）。
//! ⚠️ 早期文档把 CRC 记成 word 0x003F，已按上述来源更正；0x003F 是 SII 版本。

/// SII 类别链起始 **word** 偏移（= 字节 0x80）。
pub const SII_FIRST_CATEGORY_WORD: u16 = 0x40;

/// SII 固定区 CRC-8 所在 word（低字节存放校验值，高字节为 0）。
pub const SII_CHECKSUM_WORD: u16 = 0x0007;
/// CRC-8 覆盖的字节数：word 0x0000–0x0006 共 14 字节。
pub const SII_CHECKSUM_BYTES: usize = 14;
/// EEPROM 大小 word（单位 128 字节；值 0 = 128 字节）。
pub const SII_EEPROM_SIZE_WORD: u16 = 0x003E;
/// SII 版本 word。
pub const SII_VERSION_WORD: u16 = 0x003F;
/// 本镜像声明的 EEPROM 大小：值 2 = 384 字节（覆盖当前约 280 字节镜像：固定区 + 5 个类别）。
pub const SIM_EEPROM_SIZE: u16 = 2;
/// 本镜像声明的 SII 版本（ETG.1000.6：固定为 1）。
pub const SIM_SII_VERSION: u16 = 1;

/// 标准/引导模式 mailbox 参数（SII 固定区 word 0x0014–0x001C）。
///
/// IgH `master/fsm_slave_scan.c` 读取偏移：boot rx 0x0014、boot tx 0x0016、
/// std rx 0x0018、std tx 0x001A（偏移/容量各 1 word），协议字 0x001C；任一字段为
/// `0xFFFF` 时 IgH 关闭邮箱。SOEM `src/ec_config.c` 从 `mbx_wo/mbx_ro/mbx_l/mbx_rl`
/// 读取同样字段。标准模式（std）在 PREOP 后生效，boot 模式用于 bootstrap。
pub const SIM_MAILBOX_OFFSET_RX: u16 = crate::mailbox::MAILBOX_RX_OFFSET;
/// 发送邮箱（SM1）偏移。
pub const SIM_MAILBOX_OFFSET_TX: u16 = crate::mailbox::MAILBOX_TX_OFFSET;
/// 邮箱容量（字节，两个方向相同）。
pub const SIM_MAILBOX_SIZE: u16 = crate::mailbox::MAILBOX_SIZE as u16;
/// mailbox protocol 位：bit2 = CoE（`0x0004`；ETG.1000.6 / IgH `EC_MBOX_COE = 0x04` /
/// SOEM `ECT_MBXPROT_COE = 0x0004`）。注意 bit0 = AoE、bit1 = EoE、bit3 = FoE、
/// bit4 = SoE、bit5 = VoE（两家定义一致）——2026-11-12 19:36 班首次写错成 0x0001（AoE），
/// 现象是 SOEM 读到协议 01、不发起任何 CoE 邮箱请求。
pub const SIM_MAILBOX_PROTOCOLS: u16 = 0x0004;

/// SII 类别类型（ETG.1000.6）。
pub mod category {
    /// 字符串类别。
    pub const STRINGS: u16 = 0x000A;
    /// 通用类别（固定 32 字节）。
    pub const GENERAL: u16 = 0x001E;
    /// FMMU 类别（FMMU 用途字节，见 `SIM_FMMU_USAGE`）。
    pub const FMMU: u16 = 0x0028;
    /// 分布式时钟类别（DC，内容见 `SiiDc`）。
    pub const DC: u16 = 0x003C;
    /// Sync Manager 类别（每条 8 字节）。
    pub const SYNC_MANAGER: u16 = 0x0029;
    /// TxPDO 类别（从站 → 主站，输入方向）。
    pub const TX_PDO: u16 = 0x0032;
    /// RxPDO 类别（主站 → 从站，输出方向）。
    pub const RX_PDO: u16 = 0x0033;
    /// 类别链终止字。
    pub const TERMINATOR: u16 = 0xFFFF;
}

/// SII CRC-8：多项式 `x^8 + x^2 + x + 1`（0x07）、初值 0xFF、MSB-first。
///
/// 覆盖范围由调用方给定：固定区校验必须传 word 0x0000–0x0006 的 14 个字节
/// （见 [`SII_CHECKSUM_BYTES`]）。
#[must_use]
pub fn sii_crc8(data: &[u8]) -> u8 {
    let mut crc: u8 = 0xFF;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x07
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// 读取超出镜像范围的 word 时返回的值（SII 未使用区的填充值）。
pub const SII_WORD_OUT_OF_RANGE: u16 = 0xFFFF;

/// 仿真从站 Vendor ID（我方占位值，非 ETG 注册号；R2：不使用厂商 VID/PID）。
pub const SIM_VENDOR_ID: u32 = 0x0000_0EC5;
/// 仿真从站 Product Code（我方占位值）。
pub const SIM_PRODUCT_CODE: u32 = 0x0000_E5C1;
/// 仿真从站 Revision Number。
pub const SIM_REVISION: u32 = 0x0000_0001;
/// 仿真从站 Serial Number。
pub const SIM_SERIAL: u32 = 0x0000_0001;

/// 字符串类别（0x000A）中的设备名（索引 0）与厂商名（索引 1）。
pub const SIM_NAMES: [&str; 2] = ["EC-Sim SIMULATED slave", "SIMULATED"];

/// 本镜像的 PDO 画像：**8 位输入 + 8 位输出**的最小数字 IO（通用占位，不绑定厂商设备）。
pub const SIM_INPUT_BITS: u8 = 8;
/// 输出位数。
pub const SIM_OUTPUT_BITS: u8 = 8;
/// 输入对象索引（占位；对象字典/CoE 属 B7）。
pub const SIM_INPUT_OBJECT: u16 = 0x6000;
/// 输出对象索引（占位；对象字典/CoE 属 B7）。
pub const SIM_OUTPUT_OBJECT: u16 = 0x7000;
/// 输入 PDO 索引（TxPDO，从站 → 主站）。
pub const SIM_TX_PDO_INDEX: u16 = 0x1A00;
/// 输出 PDO 索引（RxPDO，主站 → 从站）。
pub const SIM_RX_PDO_INDEX: u16 = 0x1600;
/// 输出 Sync Manager 通道（SM2）。
pub const SIM_OUTPUT_SM_INDEX: u8 = 2;
/// 输入 Sync Manager 通道（SM3）。
pub const SIM_INPUT_SM_INDEX: u8 = 3;
/// SM2（输出）的物理起始地址（DL 用户内存）。
pub const SIM_OUTPUT_SM_START: u16 = 0x1100;
/// SM3（输入）的物理起始地址（DL 用户内存）。
pub const SIM_INPUT_SM_START: u16 = 0x1110;

/// SII SM 类别（0x0029）中的一个 Sync Manager 条目，固定 8 字节。
///
/// 字段顺序按 IgH `ec_slave_fetch_sii_syncs()`：start u16、length u16、control u8、
/// status u8、enable u8、pdi_control u8；SOEM `ecx_siiSM()` 读取相同布局（忽略 status/pdi）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SiiSyncManager {
    /// 物理起始地址（DL 用户内存地址，如 0x1100）。
    pub physical_start_address: u16,
    /// 默认长度（字节）。
    pub length: u16,
    /// Control 字节（ETG.1000.4 table 58：bit0–1 模式、bit2–3 方向、bit4/5 事件、bit6 watchdog）。
    pub control: u8,
    /// Status 字节（SII 中通常为 0）。
    pub status: u8,
    /// Enable/Activate（bit0 使能）。
    pub enable: u8,
    /// PDI Control 字节（SII 中通常为 0）。
    pub pdi_control: u8,
}

impl SiiSyncManager {
    /// 条目字节数。
    pub const BYTES: usize = 8;

    /// 编码为 8 字节（小端多字节字段）。
    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::BYTES] {
        let mut bytes = [0u8; Self::BYTES];
        bytes[0..2].copy_from_slice(&self.physical_start_address.to_le_bytes());
        bytes[2..4].copy_from_slice(&self.length.to_le_bytes());
        bytes[4] = self.control;
        bytes[5] = self.status;
        bytes[6] = self.enable;
        bytes[7] = self.pdi_control;
        bytes
    }

    /// 从 8 字节解码。
    #[must_use]
    pub fn from_bytes(bytes: &[u8; Self::BYTES]) -> Self {
        Self {
            physical_start_address: u16::from_le_bytes([bytes[0], bytes[1]]),
            length: u16::from_le_bytes([bytes[2], bytes[3]]),
            control: bytes[4],
            status: bytes[5],
            enable: bytes[6],
            pdi_control: bytes[7],
        }
    }
}

/// 本镜像的 4 个 SM 条目。
///
/// SM0 = mailbox out（主站 → 从站，0x1000，128 字节，control `0x26` = mailbox 模式 +
/// 写方向 + 事件位），SM1 = mailbox in（从站 → 主站，0x1080，128 字节，control `0x22`）。
/// 这两个 control 值与 IgH `master/fsm_slave_config.c` 的默认邮箱 SM 配置
/// （`sync.control_register = 0x26 / 0x22`）及 SOEM `src/ec_config.c` 的
/// `EC_DEFAULTMBXSM0 = 0x00010026` / `EC_DEFAULTMBXSM1 = 0x00010022` 一致。
/// SM2 = 输出（buffered、方向 write = `0x04`），SM3 = 输入（buffered、方向 read = `0x00`）；
/// 方向位编码见 `sm.rs` 模块文档（ETG.1000.4 table 58）。
/// SII FMMU 类别内容（4 字节用途码，逐字节对应 FMMU0..FMMU3）。
///
/// 依据公开资料（CSDN kevin1499《EtherCAT 从站 EEPROM 分类附加信息详解：FMMU》，
/// 访问 2026-11-13；只读参考）：FMMU0 `0x03` 用于读取邮箱、FMMU1 `0x01` 过程数据输出、
/// FMMU2 `0x02` 过程数据输入、FMMU3 `0xFF` 未使用；类别头 `0x0028`、长度字 = 2（4 字节）。
pub const SIM_FMMU_USAGE: [u8; 4] = [0x03, 0x01, 0x02, 0xFF];

pub const SIM_SYNC_MANAGERS: [SiiSyncManager; 4] = [
    SiiSyncManager {
        physical_start_address: SIM_MAILBOX_OFFSET_RX,
        length: SIM_MAILBOX_SIZE,
        control: 0x26,
        status: 0,
        enable: 1,
        pdi_control: 0,
    },
    SiiSyncManager {
        physical_start_address: SIM_MAILBOX_OFFSET_TX,
        length: SIM_MAILBOX_SIZE,
        control: 0x22,
        status: 0,
        enable: 1,
        pdi_control: 0,
    },
    SiiSyncManager {
        physical_start_address: SIM_OUTPUT_SM_START,
        length: 1,
        control: 0x04,
        status: 0,
        enable: 1,
        pdi_control: 0,
    },
    SiiSyncManager {
        physical_start_address: SIM_INPUT_SM_START,
        length: 1,
        control: 0x00,
        status: 0,
        enable: 1,
        pdi_control: 0,
    },
];

/// TxPDO/RxPDO 类别中的一条映射项，固定 8 字节。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SiiPdoEntry {
    /// 对象索引（如 0x6000）。
    pub index: u16,
    /// 子索引。
    pub subindex: u8,
    /// 位长度（如 8）。
    pub bit_length: u8,
}

impl SiiPdoEntry {
    /// 条目字节数。
    pub const BYTES: usize = 8;

    /// 编码为 8 字节：index u16、subindex u8、name_index（0）、保留、bit_length（+5）、保留。
    #[must_use]
    pub fn to_bytes(self) -> [u8; Self::BYTES] {
        let mut bytes = [0u8; Self::BYTES];
        bytes[0..2].copy_from_slice(&self.index.to_le_bytes());
        bytes[2] = self.subindex;
        bytes[5] = self.bit_length;
        bytes
    }
}

/// 一个 PDO 的默认映射（SII PDO 类别条目：8 字节头 + 映射项）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SiiPdo {
    /// PDO 索引（如 0x1A00）。
    pub index: u16,
    /// 绑定的 Sync Manager 通道。
    pub sync_manager: u8,
    /// 默认映射项。
    pub entries: &'static [SiiPdoEntry],
}

impl SiiPdo {
    /// 编码为 SII PDO 类别数据（不包含类别头）。
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(8 + self.entries.len() * SiiPdoEntry::BYTES);
        bytes.extend_from_slice(&self.index.to_le_bytes());
        bytes.push(self.entries.len() as u8); // entry_count @+2
        bytes.push(self.sync_manager); // sync_index @+3
        bytes.push(0); // @+4 保留
        bytes.push(0); // name_index @+5（0 = 无名称）
        bytes.extend_from_slice(&[0, 0]); // @+6..+8 保留
        for entry in self.entries {
            bytes.extend_from_slice(&entry.to_bytes());
        }
        bytes
    }
}

/// 输入方向默认映射（TxPDO 0x1A00 → SM3，8 位输入）。
pub const SIM_TX_PDO_ENTRIES: [SiiPdoEntry; 1] = [SiiPdoEntry {
    index: SIM_INPUT_OBJECT,
    subindex: 0,
    bit_length: SIM_INPUT_BITS,
}];
/// 输出方向默认映射（RxPDO 0x1600 → SM2，8 位输出）。
pub const SIM_RX_PDO_ENTRIES: [SiiPdoEntry; 1] = [SiiPdoEntry {
    index: SIM_OUTPUT_OBJECT,
    subindex: 0,
    bit_length: SIM_OUTPUT_BITS,
}];
/// 本镜像的 TxPDO 列表（输入方向）。
pub const SIM_TX_PDOS: [SiiPdo; 1] = [SiiPdo {
    index: SIM_TX_PDO_INDEX,
    sync_manager: SIM_INPUT_SM_INDEX,
    entries: &SIM_TX_PDO_ENTRIES,
}];
/// 本镜像的 RxPDO 列表（输出方向）。
pub const SIM_RX_PDOS: [SiiPdo; 1] = [SiiPdo {
    index: SIM_RX_PDO_INDEX,
    sync_manager: SIM_OUTPUT_SM_INDEX,
    entries: &SIM_RX_PDO_ENTRIES,
}];

/// SII 通用类别（0x001E）里的 CoE 能力位（IgH `slave.c: ec_slave_fetch_sii_general()`
/// 的 `coe_details`：bit0 enable SDO、bit1 enable SDO Info、bit2 enable PDO assign、
/// bit3 enable PDO configuration）。
///
/// 本模拟器支持 SDO 上传/下载、SDO Info（`0x8` 服务）与 PDO 分配/映射对象读写 → `0x0F`。
/// 内置镜像保持 bit5（Complete Access）为 0：CA 上传/下载虽已实现（2026-11-13），但
/// 内置镜像的 SOEM `ecx_readPDOmapCA()` 路径尚未验证；声明 CA 的画像（manifest
/// `coe.complete_access: true`）已验证 SOEM/IgH 扫描与 OP 无回归。
pub const SIM_COE_DETAILS: u8 = 0x0F;

/// 标准/引导邮箱参数（SII 固定区 word 0x0014–0x001C）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SiiMailbox {
    /// 接收（主站 → 从站，SM0）邮箱偏移。
    pub rx_offset: u16,
    /// 接收邮箱容量（字节）。
    pub rx_size: u16,
    /// 发送（从站 → 主站，SM1）邮箱偏移。
    pub tx_offset: u16,
    /// 发送邮箱容量（字节）。
    pub tx_size: u16,
    /// 邮箱协议位（bit2 = CoE，见 [`SIM_MAILBOX_PROTOCOLS`]）。
    pub protocols: u16,
}

/// 运行时 PDO 描述（SII 类别 0x0032 / 0x0033 的一个 PDO）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiiPdoSpec {
    /// PDO 索引（如 0x1600 / 0x1A00）。
    pub index: u16,
    /// 绑定的 Sync Manager 通道。
    pub sync_manager: u8,
    /// 默认映射项。
    pub entries: Vec<SiiPdoEntry>,
}

/// SII 镜像的运行时描述：声明式插件 manifest / 后续 ESI 转换的生成输入。
///
/// [`SiiImage::from_spec`] 把它编码为完整 SII 镜像（固定区 + 类别链 + 终止字），
/// 并自动计算固定区 CRC-8 与 EEPROM 大小（word 0x003E）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiiSpec {
    /// Vendor ID（必须是我方自有，R2）。
    pub vendor_id: u32,
    /// Product Code（自有）。
    pub product_code: u32,
    /// Revision Number。
    pub revision: u32,
    /// Serial Number。
    pub serial: u32,
    /// 字符串类别：索引 0 = 设备名（必须带 `SIMULATED`），其后为厂商名等。
    pub names: Vec<String>,
    /// 邮箱参数；`None` = 不声明邮箱（固定区 0x0014–0x001B 填 0xFFFF、协议字 0）。
    pub mailbox: Option<SiiMailbox>,
    /// Sync Manager 类别（0x0029）条目。
    pub sync_managers: Vec<SiiSyncManager>,
    /// 输入方向 PDO（TxPDO，类别 0x0032，从站 → 主站）。
    pub tx_pdos: Vec<SiiPdoSpec>,
    /// 输出方向 PDO（RxPDO，类别 0x0033，主站 → 从站）。
    pub rx_pdos: Vec<SiiPdoSpec>,
    /// 通用类别 CoE details 字节（bit0 SDO、bit1 SDO Info、bit2 PDO assign、
    /// bit3 PDO configuration、bit5 Complete Access）。
    pub coe_details: u8,
    /// 通用类别 general flags（`data[11]`，bit0 enable SAFEOP、bit1 enable not-LRW）。
    pub general_flags: u8,
    /// Ebus 电流（mA，通用类别 `data[12]`）。
    pub ebus_current: i16,
    /// 分布式时钟类别（0x003C）；`None` = 不声明 DC（模拟器未实现 DC 同步时保持 None）。
    pub dc: Option<SiiDc>,
}

/// SII 分布式时钟类别（0x003C）内容（24 字节，12 words）。
///
/// 字节布局（公开资料 CSDN《EtherCAT EEPROM 详解》DC 表，访问 2026-11-13；只读参考）：
/// `cycleTime0` u32 @0、`shiftTime0` u32 @4、`shiftTime1` u32 @8、
/// `sync1CycleFactor` i16 @0x0C、`assignActivate` u16 @0x0E、`sync0CycleFactor` i16 @0x10、
/// `nameIndex` u8 @0x12、`descIndex` u8 @0x13、`reserved[4]` @0x14。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SiiDc {
    /// DC 周期时间（ns）。
    pub cycle_time0: u32,
    /// SYNC0 相对参考时钟的偏移（ns）。
    pub shift_time0: u32,
    /// SYNC1 相对 SYNC0 的偏移（ns）。
    pub shift_time1: u32,
    /// SYNC1 周期因子（i16，-1 = 禁用）。
    pub sync1_cycle_factor: i16,
    /// SYNC 激活位（ETG.1000.6 assign activate）。
    pub assign_activate: u16,
    /// SYNC0 周期因子（i16）。
    pub sync0_cycle_factor: i16,
    /// 字符串类别中的名称索引（0 = 无名称）。
    pub name_index: u8,
    /// 字符串类别中的描述索引（0 = 无描述）。
    pub desc_index: u8,
}

impl SiiDc {
    /// 编码为 SII DC 类别数据（不含类别头；固定 24 字节）。
    #[must_use]
    pub fn to_bytes(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(24);
        bytes.extend_from_slice(&self.cycle_time0.to_le_bytes());
        bytes.extend_from_slice(&self.shift_time0.to_le_bytes());
        bytes.extend_from_slice(&self.shift_time1.to_le_bytes());
        bytes.extend_from_slice(&self.sync1_cycle_factor.to_le_bytes());
        bytes.extend_from_slice(&self.assign_activate.to_le_bytes());
        bytes.extend_from_slice(&self.sync0_cycle_factor.to_le_bytes());
        bytes.push(self.name_index);
        bytes.push(self.desc_index);
        bytes.extend_from_slice(&[0u8; 4]); // reserved
        bytes
    }
}

/// 最小 SII 镜像（word 数组，1 word = 2 字节小端）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiiImage {
    words: Vec<u16>,
}

impl SiiImage {
    /// 构造"1 个仿真从站"的最小 SII：固定区（含 mailbox 参数）+ 字符串/通用/SM/TxPDO/RxPDO
    /// 类别 + 终止字（等价于 [`SiiSpec`] 的默认画像）。
    #[must_use]
    pub fn simulated_slave() -> Self {
        Self::from_spec(&simulated_spec())
    }

    /// 按运行时 [`SiiSpec`] 生成完整 SII 镜像。
    ///
    /// 生成顺序：固定区（识别/邮箱/SII 版本）→ 字符串 / 通用 / SM / TxPDO / RxPDO 类别
    /// → `0xFFFF` 终止字；随后按实际长度回填 **word 0x003E（EEPROM 大小，单位 128 字节，
    /// 值 = 单元数 − 1）** 与固定区 **CRC-8（word 0x0007，覆盖 word 0x0000–0x0006）**。
    #[must_use]
    pub fn from_spec(spec: &SiiSpec) -> Self {
        let mut words = vec![0u16; usize::from(SII_FIRST_CATEGORY_WORD)];
        put_u32(&mut words, 0x0008, spec.vendor_id);
        put_u32(&mut words, 0x000A, spec.product_code);
        put_u32(&mut words, 0x000C, spec.revision);
        put_u32(&mut words, 0x000E, spec.serial);
        match spec.mailbox {
            Some(mailbox) => {
                // boot 与 std 两套都声明（偏移/容量），协议字最后写。
                for base in [0x0014usize, 0x0018] {
                    words[base] = mailbox.rx_offset;
                    words[base + 1] = mailbox.rx_size;
                    words[base + 2] = mailbox.tx_offset;
                    words[base + 3] = mailbox.tx_size;
                }
                words[0x001C] = mailbox.protocols;
            }
            None => {
                for word in &mut words[0x0014..=0x001B] {
                    *word = 0xFFFF;
                }
                words[0x001C] = 0;
            }
        }
        words[usize::from(SII_VERSION_WORD)] = SIM_SII_VERSION;

        push_category(
            &mut words,
            category::STRINGS,
            &strings_category(&spec.names),
        );
        push_category(
            &mut words,
            category::GENERAL,
            &general_category(spec.coe_details, spec.general_flags, spec.ebus_current),
        );
        push_category(
            &mut words,
            category::SYNC_MANAGER,
            &sync_manager_category(&spec.sync_managers),
        );
        push_category(&mut words, category::FMMU, &SIM_FMMU_USAGE);
        if let Some(dc) = &spec.dc {
            push_category(&mut words, category::DC, &dc.to_bytes());
        }
        if !spec.tx_pdos.is_empty() {
            push_category(&mut words, category::TX_PDO, &pdo_category(&spec.tx_pdos));
        }
        if !spec.rx_pdos.is_empty() {
            push_category(&mut words, category::RX_PDO, &pdo_category(&spec.rx_pdos));
        }
        words.push(category::TERMINATOR);

        // EEPROM 大小：word 0x003E 的值 = ceil(字节数 / 128) − 1（0 = 128 字节）。
        let bytes = words.len() * 2;
        let units = bytes.div_ceil(128).saturating_sub(1);
        words[usize::from(SII_EEPROM_SIZE_WORD)] = u16::try_from(units).unwrap_or(u16::MAX);
        // 固定区 CRC-8 必须在 0x0000–0x0006 全部确定后计算，写入 word 0x0007。
        words[usize::from(SII_CHECKSUM_WORD)] = u16::from(Self::fixed_area_crc8(&words));
        Self { words }
    }

    /// 完整 word 数组。
    #[must_use]
    pub fn words(&self) -> &[u16] {
        &self.words
    }

    /// 读取一个 word；越界返回 [`SII_WORD_OUT_OF_RANGE`]。
    #[must_use]
    pub fn read_word(&self, offset: u16) -> u16 {
        self.words
            .get(usize::from(offset))
            .copied()
            .unwrap_or(SII_WORD_OUT_OF_RANGE)
    }

    /// 镜像长度（word 数）。
    #[must_use]
    pub fn len_words(&self) -> usize {
        self.words.len()
    }

    /// 查找类别并返回其数据字节（不含类别头，也不含末尾补齐字节）。
    ///
    /// 遍历方式与 IgH `ec_slave_fetch_sii_*()` / SOEM `ecx_siifind()` 一致：
    /// 类别头 = `type` + `size`（word 数），数据紧随其后；遇到 `0xFFFF` 终止字返回 `None`。
    #[must_use]
    pub fn category_bytes(&self, category_type: u16) -> Option<Vec<u8>> {
        let mut word = usize::from(SII_FIRST_CATEGORY_WORD);
        loop {
            let cat_type = (*self.words.get(word)?) & 0x7FFF;
            if cat_type == category::TERMINATOR {
                return None;
            }
            let size_words = usize::from(*self.words.get(word + 1)?);
            let start = word + 2;
            let end = start.checked_add(size_words)?;
            let data_words = self.words.get(start..end)?;
            if cat_type == category_type {
                let mut bytes = Vec::with_capacity(size_words * 2);
                for w in data_words {
                    bytes.extend_from_slice(&w.to_le_bytes());
                }
                return Some(bytes);
            }
            word = end;
        }
    }

    /// 固定区声明的邮箱协议位（word 0x001C）；`0xFFFF`（未声明）归一化为 0。
    #[must_use]
    pub fn mailbox_protocols(&self) -> u16 {
        match self.read_word(0x001C) {
            SII_WORD_OUT_OF_RANGE => 0,
            protocols => protocols,
        }
    }

    /// SM 类别（0x0029）声明的 Sync Manager 数量；无类别时返回 0。
    #[must_use]
    pub fn sync_manager_count(&self) -> usize {
        self.category_bytes(category::SYNC_MANAGER)
            .map_or(0, |bytes| bytes.len() / SiiSyncManager::BYTES)
    }

    /// SII 是否声明过程数据 SM（SM2/SM3 至少其一，即 SM 数量 ≥ 3）。
    ///
    /// 用于 ESM 前置条件：声明了过程数据 SM 的从站在进入 SAFEOP 前必须先配置 SM2/SM3。
    #[must_use]
    pub fn declares_process_data_sync_managers(&self) -> bool {
        self.sync_manager_count() >= 3
    }

    /// word 0x0007 中当前存储的 CRC-8（低字节）。
    #[must_use]
    pub fn checksum(&self) -> u8 {
        (self.read_word(SII_CHECKSUM_WORD) & 0x00FF) as u8
    }

    /// 按当前 word 0x0000–0x0006 重新计算固定区 CRC-8。
    #[must_use]
    pub fn calculate_checksum(&self) -> u8 {
        Self::fixed_area_crc8(&self.words)
    }

    /// word 0x0007 是否与重新计算值一致。
    #[must_use]
    pub fn verify_checksum(&self) -> bool {
        self.checksum() == self.calculate_checksum()
    }

    /// 设置站别名（word 0x0004）并重算 CRC-8。
    ///
    /// 对应 SOEM `eepromtool -walias` 的行为：改别名后必须重写 word 0x0007，
    /// 否则校验不一致（真实 EEPROM 的写入工具即如此处理）。
    pub fn set_alias(&mut self, alias: u16) {
        let Some(word) = self.words.get_mut(0x0004) else {
            return;
        };
        *word = alias;
        self.recompute_checksum();
    }

    /// 由 word 0x0000–0x0006 的 14 字节计算固定区 CRC-8（word 不足时按 0 补齐）。
    fn fixed_area_crc8(words: &[u16]) -> u8 {
        let mut bytes = [0u8; SII_CHECKSUM_BYTES];
        for (i, slot) in bytes.chunks_mut(2).enumerate() {
            if let Some(word) = words.get(i) {
                slot.copy_from_slice(&word.to_le_bytes());
            }
        }
        sii_crc8(&bytes)
    }

    fn recompute_checksum(&mut self) {
        let crc = Self::fixed_area_crc8(&self.words);
        if let Some(word) = self.words.get_mut(usize::from(SII_CHECKSUM_WORD)) {
            *word = u16::from(crc);
        }
    }
}

fn put_u32(words: &mut [u16], word_offset: usize, value: u32) {
    words[word_offset] = (value & 0xFFFF) as u16;
    words[word_offset + 1] = (value >> 16) as u16;
}

/// SM 类别（0x0029）数据：每条 8 字节，无额外计数/长度 word
/// （类别头已声明 size，两家主站都按 size 推导条数）。
fn sync_manager_category(entries: &[SiiSyncManager]) -> Vec<u8> {
    let mut data = Vec::with_capacity(entries.len() * SiiSyncManager::BYTES);
    for entry in entries {
        data.extend_from_slice(&entry.to_bytes());
    }
    data
}

/// PDO 类别（0x0032/0x0033）数据：逐个 PDO 的 8 字节头 + 映射项，无额外计数 word。
fn pdo_category(pdos: &[SiiPdoSpec]) -> Vec<u8> {
    let mut data = Vec::new();
    for pdo in pdos {
        data.extend_from_slice(&pdo.index.to_le_bytes());
        data.push(u8::try_from(pdo.entries.len()).unwrap_or(u8::MAX));
        data.push(pdo.sync_manager);
        data.push(0); // @+4 保留
        data.push(0); // @+5 name_index（0 = 无名称）
        data.extend_from_slice(&[0, 0]); // @+6..+8 保留
        for entry in &pdo.entries {
            data.extend_from_slice(&entry.to_bytes());
        }
    }
    data
}

/// 按 ETG.1000.6 字符串类别格式编码：u8 条数 + 每条 (u8 长度 + 字节)，补齐偶数字节。
fn strings_category<S: AsRef<str>>(strings: &[S]) -> Vec<u8> {
    let mut data = Vec::new();
    data.push(u8::try_from(strings.len()).unwrap_or(u8::MAX));
    for s in strings {
        let bytes = s.as_ref().as_bytes();
        data.push(u8::try_from(bytes.len()).unwrap_or(u8::MAX));
        data.extend_from_slice(bytes);
    }
    if data.len() % 2 == 1 {
        data.push(0);
    }
    data
}

/// 通用类别（0x001E）的 32 字节数据：CoE details（data[5]）、general flags（`data[11]`）、
/// Ebus 电流（data[0x0C..0x0E]）。布局依据 IgH `slave.c: ec_slave_fetch_sii_general()`。
fn general_category(coe_details: u8, general_flags: u8, ebus_current: i16) -> Vec<u8> {
    let mut data = vec![0u8; 32];
    data[5] = coe_details;
    data[11] = general_flags;
    data[0x0C..0x0E].copy_from_slice(&ebus_current.to_le_bytes());
    data
}

/// 内置仿真从站的 [`SiiSpec`]（由 B6/B7 的常量画像构造）。
#[must_use]
pub fn simulated_spec() -> SiiSpec {
    SiiSpec {
        vendor_id: SIM_VENDOR_ID,
        product_code: SIM_PRODUCT_CODE,
        revision: SIM_REVISION,
        serial: SIM_SERIAL,
        names: SIM_NAMES.iter().map(|name| (*name).to_string()).collect(),
        mailbox: Some(SiiMailbox {
            rx_offset: SIM_MAILBOX_OFFSET_RX,
            rx_size: SIM_MAILBOX_SIZE,
            tx_offset: SIM_MAILBOX_OFFSET_TX,
            tx_size: SIM_MAILBOX_SIZE,
            protocols: SIM_MAILBOX_PROTOCOLS,
        }),
        sync_managers: SIM_SYNC_MANAGERS.to_vec(),
        tx_pdos: SIM_TX_PDOS
            .iter()
            .map(|pdo| SiiPdoSpec {
                index: pdo.index,
                sync_manager: pdo.sync_manager,
                entries: pdo.entries.to_vec(),
            })
            .collect(),
        rx_pdos: SIM_RX_PDOS
            .iter()
            .map(|pdo| SiiPdoSpec {
                index: pdo.index,
                sync_manager: pdo.sync_manager,
                entries: pdo.entries.to_vec(),
            })
            .collect(),
        coe_details: SIM_COE_DETAILS,
        general_flags: 0,
        ebus_current: 0,
        dc: None,
    }
}

fn push_category(words: &mut Vec<u16>, cat_type: u16, data: &[u8]) {
    let mut data = data.to_vec();
    if data.len() % 2 == 1 {
        data.push(0);
    }
    words.push(cat_type);
    words.push((data.len() / 2) as u16);
    for chunk in data.chunks(2) {
        words.push(u16::from_le_bytes([chunk[0], chunk[1]]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_area_has_identity_fields() {
        let sii = SiiImage::simulated_slave();
        assert_eq!(sii.read_word(0x0008), (SIM_VENDOR_ID & 0xFFFF) as u16);
        assert_eq!(sii.read_word(0x0009), (SIM_VENDOR_ID >> 16) as u16);
        assert_eq!(sii.read_word(0x000A), (SIM_PRODUCT_CODE & 0xFFFF) as u16);
        assert_eq!(sii.read_word(0x000C), (SIM_REVISION & 0xFFFF) as u16);
        assert_eq!(sii.read_word(0x000E), (SIM_SERIAL & 0xFFFF) as u16);
        // mailbox 声明（B7 子项 1）：boot 与 std 两套 rx/tx 偏移 + 容量 + CoE 协议位。
        // IgH 读 std 在 word 0x0018/0x001A，SOEM 读 mbx_wo/mbx_ro/mbx_l/mbx_rl。
        assert_eq!(sii.read_word(0x0014), SIM_MAILBOX_OFFSET_RX);
        assert_eq!(sii.read_word(0x0015), SIM_MAILBOX_SIZE);
        assert_eq!(sii.read_word(0x0016), SIM_MAILBOX_OFFSET_TX);
        assert_eq!(sii.read_word(0x0017), SIM_MAILBOX_SIZE);
        assert_eq!(sii.read_word(0x0018), SIM_MAILBOX_OFFSET_RX);
        assert_eq!(sii.read_word(0x0019), SIM_MAILBOX_SIZE);
        assert_eq!(sii.read_word(0x001A), SIM_MAILBOX_OFFSET_TX);
        assert_eq!(sii.read_word(0x001B), SIM_MAILBOX_SIZE);
        assert_eq!(sii.read_word(0x001C), SIM_MAILBOX_PROTOCOLS);
        assert_eq!(
            sii.read_word(0x001C) & 0x0004,
            0x0004,
            "bit2 = CoE（IgH EC_MBOX_COE = 0x04 / SOEM ECT_MBXPROT_COE = 0x0004）"
        );
        // alias 为 0（word 0x0004，IgH 读取位置）
        assert_eq!(sii.read_word(0x0004), 0);
    }

    #[test]
    fn category_chain_is_walkable_and_terminated() {
        let sii = SiiImage::simulated_slave();
        let mut word = usize::from(SII_FIRST_CATEGORY_WORD);
        let mut types = Vec::new();
        loop {
            let cat_type = sii.words()[word];
            if cat_type == 0xFFFF {
                break;
            }
            let cat_size = usize::from(sii.words()[word + 1]);
            types.push(cat_type);
            word += 2 + cat_size;
            assert!(word < sii.len_words(), "category chain must stay in range");
        }
        assert_eq!(
            types,
            vec![
                category::STRINGS,
                category::GENERAL,
                category::SYNC_MANAGER,
                category::FMMU,
                category::TX_PDO,
                category::RX_PDO,
            ]
        );
        // 终止字存在，且之后无多余 word
        assert_eq!(word + 1, sii.len_words());
    }

    #[test]
    fn fmmu_category_has_usage_bytes() {
        let sii = SiiImage::simulated_slave();
        assert_eq!(
            sii.category_bytes(category::FMMU).as_deref(),
            Some(&SIM_FMMU_USAGE[..])
        );
        // 类别头长度字 = 2（4 字节）。
        let mut word = usize::from(SII_FIRST_CATEGORY_WORD);
        loop {
            let cat_type = sii.words()[word];
            if cat_type == category::FMMU {
                assert_eq!(sii.words()[word + 1], 2);
                break;
            }
            assert_ne!(cat_type, 0xFFFF, "FMMU 类别应存在于链中");
            word += 2 + usize::from(sii.words()[word + 1]);
        }
    }

    #[test]
    fn dc_category_encodes_documented_layout() {
        let dc = SiiDc {
            cycle_time0: 0x000F_4240, // 1 ms
            shift_time0: 0x0000_03E8,
            shift_time1: 0x0000_07D0,
            sync1_cycle_factor: -1,
            assign_activate: 0x0300,
            sync0_cycle_factor: 1,
            name_index: 0,
            desc_index: 0,
        };
        let bytes = dc.to_bytes();
        assert_eq!(bytes.len(), 24);
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().expect("u32")),
            0x000F_4240
        );
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().expect("u32")),
            0x03E8
        );
        assert_eq!(
            u32::from_le_bytes(bytes[8..12].try_into().expect("u32")),
            0x07D0
        );
        assert_eq!(
            i16::from_le_bytes(bytes[12..14].try_into().expect("i16")),
            -1
        );
        assert_eq!(
            u16::from_le_bytes(bytes[14..16].try_into().expect("u16")),
            0x0300
        );
        assert_eq!(
            i16::from_le_bytes(bytes[16..18].try_into().expect("i16")),
            1
        );
        assert_eq!(&bytes[18..24], &[0u8; 6]); // name/desc 索引 + reserved
    }

    #[test]
    fn from_spec_with_dc_adds_dc_category() {
        let dc = SiiDc {
            cycle_time0: 1_000_000,
            shift_time0: 0,
            shift_time1: 0,
            sync1_cycle_factor: -1,
            assign_activate: 0x0300,
            sync0_cycle_factor: 1,
            name_index: 0,
            desc_index: 0,
        };
        let spec = SiiSpec {
            dc: Some(dc),
            ..simulated_spec()
        };
        let sii = SiiImage::from_spec(&spec);
        assert_eq!(sii.category_bytes(category::DC), Some(dc.to_bytes()));
        // DC 类别头长度字 = 12（24 字节 / 2）。
        let mut word = usize::from(SII_FIRST_CATEGORY_WORD);
        loop {
            let cat_type = sii.words()[word];
            if cat_type == category::DC {
                assert_eq!(sii.words()[word + 1], 12);
                break;
            }
            assert_ne!(cat_type, 0xFFFF, "DC 类别应存在于链中");
            word += 2 + usize::from(sii.words()[word + 1]);
        }
    }

    #[test]
    fn strings_category_parses_back() {
        let sii = SiiImage::simulated_slave();
        let start = usize::from(SII_FIRST_CATEGORY_WORD) + 2;
        let data_word_len = usize::from(sii.words()[usize::from(SII_FIRST_CATEGORY_WORD) + 1]);
        let mut bytes = Vec::new();
        for w in &sii.words()[start..start + data_word_len] {
            bytes.extend_from_slice(&w.to_le_bytes());
        }
        assert_eq!(usize::from(bytes[0]), SIM_NAMES.len());
        let mut offset = 1;
        for name in SIM_NAMES {
            let len = usize::from(bytes[offset]);
            offset += 1;
            assert_eq!(&bytes[offset..offset + len], name.as_bytes());
            offset += len;
        }
    }

    #[test]
    fn out_of_range_reads_yield_sentinel() {
        let sii = SiiImage::simulated_slave();
        assert_eq!(sii.read_word(sii.len_words() as u16), SII_WORD_OUT_OF_RANGE);
    }

    #[test]
    fn crc8_matches_cross_checked_vectors() {
        // 向量由两个独立实现交叉校验：SOEM eepromtool 的逐位形式与
        // IgH tool/sii_crc.cpp 的 pycrc bit-feeding 形式（poly 0x07、init 0xFF、MSB-first）。
        assert_eq!(sii_crc8(&[]), 0xFF);
        assert_eq!(sii_crc8(&[0x00]), 0xF3);
        assert_eq!(sii_crc8(&[0, 1, 2, 3, 4, 5, 6]), 0x6B);
        assert_eq!(sii_crc8(&[0u8; SII_CHECKSUM_BYTES]), 0x30);
        let ascending: Vec<u8> = (0u8..SII_CHECKSUM_BYTES as u8).collect();
        assert_eq!(sii_crc8(&ascending), 0xE0);
    }

    #[test]
    fn checksum_word_matches_first_14_bytes() {
        let sii = SiiImage::simulated_slave();
        let mut bytes = Vec::new();
        for word in &sii.words()[..SII_CHECKSUM_BYTES / 2] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        assert_eq!(bytes.len(), SII_CHECKSUM_BYTES);
        assert_eq!(sii.checksum(), sii_crc8(&bytes));
        assert!(sii.verify_checksum());
        // CRC-8 只占低字节，高字节必须为 0。
        assert_eq!(sii.read_word(SII_CHECKSUM_WORD) & 0xFF00, 0);
    }

    #[test]
    fn fixed_area_declares_eeprom_size_and_sii_version() {
        let sii = SiiImage::simulated_slave();
        assert_eq!(sii.read_word(SII_EEPROM_SIZE_WORD), SIM_EEPROM_SIZE);
        assert_eq!(sii.read_word(SII_VERSION_WORD), SIM_SII_VERSION);
        // 声明容量（(size + 1) * 128 字节）必须覆盖镜像本身。
        assert!(usize::from(SIM_EEPROM_SIZE + 1) * 128 >= sii.len_words() * 2);
    }

    #[test]
    fn alias_change_recomputes_checksum() {
        let mut sii = SiiImage::simulated_slave();
        let before = sii.checksum();
        sii.set_alias(0x1234);
        assert_eq!(sii.read_word(0x0004), 0x1234);
        assert_ne!(sii.checksum(), before);
        assert!(sii.verify_checksum());
    }

    #[test]
    fn checksum_detects_fixed_area_corruption() {
        let mut sii = SiiImage::simulated_slave();
        sii.words[0x0004] ^= 0x0001; // 破坏站别名（CRC 覆盖 word 0x0000–0x0006）
        assert!(!sii.verify_checksum());
        // 覆盖范围之外（Vendor ID 在 word 0x0008）的改变不影响校验。
        let mut sii = SiiImage::simulated_slave();
        sii.words[0x0008] ^= 0x0001;
        assert!(sii.verify_checksum());
    }

    /// 按类别类型提取类别数据字节（不含类别头），并返回类别大小（word 数）。
    /// 走法与 IgH `fsm_slave_scan.c` / SOEM `ecx_siifind` 一致：类别头 = type + size(word)。
    fn category_data(sii: &SiiImage, cat: u16) -> Option<(Vec<u8>, u16)> {
        let mut word = usize::from(SII_FIRST_CATEGORY_WORD);
        loop {
            let cat_type = sii.words()[word] & 0x7FFF;
            if cat_type == category::TERMINATOR {
                return None;
            }
            let size_words = sii.words()[word + 1];
            let start = word + 2;
            if cat_type == cat {
                let mut bytes = Vec::with_capacity(usize::from(size_words) * 2);
                for w in &sii.words()[start..start + usize::from(size_words)] {
                    bytes.extend_from_slice(&w.to_le_bytes());
                }
                return Some((bytes, size_words));
            }
            word = start + usize::from(size_words);
        }
    }

    #[test]
    fn sync_manager_category_matches_both_master_parsers() {
        let sii = SiiImage::simulated_slave();
        let (data, size_words) = category_data(&sii, category::SYNC_MANAGER).expect("SM category");
        // IgH `ec_slave_fetch_sii_syncs()`：data_size 必须是 8 的倍数，count = size/8。
        assert_eq!(data.len() % 8, 0);
        // SOEM `ecx_siiSM()`：nSM = size_words / 4（每条 4 word）。
        assert_eq!(usize::from(size_words) / 4, SIM_SYNC_MANAGERS.len());
        assert_eq!(data.len() / 8, SIM_SYNC_MANAGERS.len());

        let mut parsed = Vec::new();
        for chunk in data.chunks(8) {
            let bytes: [u8; 8] = chunk.try_into().expect("8 bytes");
            parsed.push(SiiSyncManager::from_bytes(&bytes));
        }
        assert_eq!(parsed, SIM_SYNC_MANAGERS.to_vec());

        // SM0/SM1 = mailbox：物理 0x1000/0x1080、128 字节、control 0x26/0x22（IgH/SOEM 默认值）。
        assert_eq!(parsed[0].physical_start_address, SIM_MAILBOX_OFFSET_RX);
        assert_eq!(parsed[0].length, SIM_MAILBOX_SIZE);
        assert_eq!(
            parsed[0].control, 0x26,
            "mailbox out: 模式 10 + 写方向 + 事件位"
        );
        assert_eq!(parsed[0].enable & 0x01, 0x01);
        assert_eq!(parsed[1].physical_start_address, SIM_MAILBOX_OFFSET_TX);
        assert_eq!(parsed[1].length, SIM_MAILBOX_SIZE);
        assert_eq!(
            parsed[1].control, 0x22,
            "mailbox in: 模式 10 + 读方向 + 事件位"
        );
        assert_eq!(parsed[1].enable & 0x01, 0x01);
        // SM2 = 8 位输出：物理 0x1100、写方向（control bit2）、使能。
        assert_eq!(parsed[2].physical_start_address, SIM_OUTPUT_SM_START);
        assert_eq!(parsed[2].length, 1);
        assert_eq!(parsed[2].control & 0x0C, 0x04, "direction = write");
        assert_eq!(parsed[2].enable & 0x01, 0x01);
        // SM3 = 8 位输入：物理 0x1110、读方向、使能。
        assert_eq!(parsed[3].physical_start_address, SIM_INPUT_SM_START);
        assert_eq!(parsed[3].length, 1);
        assert_eq!(parsed[3].control & 0x0C, 0x00, "direction = read");
        assert_eq!(parsed[3].enable & 0x01, 0x01);
    }

    #[test]
    fn tx_and_rx_pdo_categories_match_both_master_parsers() {
        let sii = SiiImage::simulated_slave();

        // IgH `ec_slave_fetch_sii_pdos()`：PDO 头 8 字节 + 每条映射 8 字节。
        let (tx, tx_words) = category_data(&sii, category::TX_PDO).expect("TxPDO category");
        assert_eq!(tx.len(), 16);
        assert_eq!(
            u16::from_le_bytes([tx[0], tx[1]]),
            SIM_TX_PDO_INDEX,
            "PDO index"
        );
        assert_eq!(tx[2], 1, "entry_count");
        assert_eq!(tx[3], SIM_INPUT_SM_INDEX, "sync_index");
        assert_eq!(tx[5], 0, "name_index");
        assert_eq!(u16::from_le_bytes([tx[8], tx[9]]), SIM_INPUT_OBJECT);
        assert_eq!(tx[10], 0, "subindex");
        assert_eq!(tx[13], SIM_INPUT_BITS, "bit_length");

        // SOEM `ecx_siiPDO()`：Length = size_words；遍历 8 字节头 + 映射项，累加 bit_length。
        let tx_bits = soem_pdo_bit_sizes(&tx, tx_words);
        assert_eq!(tx_bits[SIM_INPUT_SM_INDEX as usize], 8, "输入 8 位 → SM3");

        let (rx, rx_words) = category_data(&sii, category::RX_PDO).expect("RxPDO category");
        assert_eq!(u16::from_le_bytes([rx[0], rx[1]]), SIM_RX_PDO_INDEX);
        assert_eq!(rx[3], SIM_OUTPUT_SM_INDEX);
        assert_eq!(u16::from_le_bytes([rx[8], rx[9]]), SIM_OUTPUT_OBJECT);
        assert_eq!(rx[13], SIM_OUTPUT_BITS);
        let rx_bits = soem_pdo_bit_sizes(&rx, rx_words);
        assert_eq!(rx_bits[SIM_OUTPUT_SM_INDEX as usize], 8, "输出 8 位 → SM2");
    }

    /// 复刻 SOEM `ecx_siiPDO()` 的遍历（类别大小 word 数 + 8 字节头 + 8 字节映射项），
    /// 返回每个 Sync Manager 的位总数（下标 = SM 通道）。
    fn soem_pdo_bit_sizes(data: &[u8], size_words: u16) -> [u32; 8] {
        let mut bits = [0u32; 8];
        let length = usize::from(size_words);
        let mut a = 0usize;
        let mut c = 1usize; // SOEM 把类别大小 word 也计入遍历计数
        while c < length {
            // PDO 头：index u16、entry_count u8、sync_index u8、4 字节保留。
            let entry_count = data[a + 2];
            let sync_index = data[a + 3];
            a += 8;
            c += 3;
            let mut pdo_bits = 0u32;
            for _ in 0..entry_count {
                pdo_bits += u32::from(data[a + 5]);
                a += 8;
                c += 4;
            }
            if usize::from(sync_index) < bits.len() {
                bits[usize::from(sync_index)] += pdo_bits;
            }
            c += 1;
        }
        bits
    }

    #[test]
    fn eeprom_size_declaration_covers_all_categories() {
        let sii = SiiImage::simulated_slave();
        // 固定区 word 0x003E 声明容量（单位 128 字节）：必须覆盖整个镜像。
        let declared_bytes = usize::from(SIM_EEPROM_SIZE + 1) * 128;
        assert!(
            declared_bytes >= sii.len_words() * 2,
            "声明 {declared_bytes} 字节 < 镜像 {} 字节",
            sii.len_words() * 2
        );
    }

    #[test]
    fn category_lookup_and_esm_helpers() {
        let sii = SiiImage::simulated_slave();
        assert_eq!(sii.mailbox_protocols(), SIM_MAILBOX_PROTOCOLS);
        assert_eq!(sii.sync_manager_count(), SIM_SYNC_MANAGERS.len());
        assert!(
            sii.declares_process_data_sync_managers(),
            "本镜像声明 SM2/SM3"
        );
        assert_eq!(
            sii.category_bytes(category::SYNC_MANAGER)
                .expect("SM category")
                .len(),
            SIM_SYNC_MANAGERS.len() * SiiSyncManager::BYTES
        );
        // 未在类别链中出现的类别 → None（遍历到 0xFFFF 终止字结束）。
        assert!(sii.category_bytes(0x0040).is_none());
    }

    #[test]
    fn empty_sii_declares_no_mailbox_or_process_data() {
        // 没有类别链的最小镜像：不应 panic，且各项声明均为"无"。
        let sii = SiiImage::simulated_slave();
        let empty = SiiImage {
            words: sii.words()[..usize::from(SII_FIRST_CATEGORY_WORD)].to_vec(),
        };
        assert_eq!(empty.sync_manager_count(), 0);
        assert!(!empty.declares_process_data_sync_managers());
        assert!(empty.category_bytes(category::SYNC_MANAGER).is_none());
    }

    #[test]
    fn general_category_declares_coe_capabilities_including_sdo_info() {
        let sii = SiiImage::simulated_slave();
        let (data, size_words) = category_data(&sii, category::GENERAL).expect("general category");
        assert_eq!(size_words * 2, 32, "IgH 要求通用类别正好 32 字节");
        // IgH `slave.c: ec_slave_fetch_sii_general()`：data[5] = CoE details。
        assert_eq!(data[5], SIM_COE_DETAILS);
        assert_eq!(data[5] & 0x01, 0x01, "enable SDO");
        assert_eq!(
            data[5] & 0x02,
            0x02,
            "enable SDO Info（ethercat sdos 需要）"
        );
        assert_eq!(data[5] & 0x20, 0x00, "不声明 Complete Access（尚未实现）");
    }

    #[test]
    fn from_spec_matches_builtin_profile_word_for_word() {
        // `simulated_slave()` 现在只是 `from_spec(simulated_spec())` 的薄封装；
        // 两者必须逐 word 相同（保证既有双主站验证过的镜像不变）。
        let builtin = SiiImage::simulated_slave();
        let from_spec = SiiImage::from_spec(&simulated_spec());
        assert_eq!(from_spec.words(), builtin.words());
    }

    #[test]
    fn from_spec_generates_manifest_style_image() {
        let spec = SiiSpec {
            vendor_id: 0x0000_0EC5,
            product_code: 0x0000_E5C2,
            revision: 0x0001_0000,
            serial: 0,
            names: vec!["EC-Sim 8DO (SIMULATED)".to_string(), "EC-Sim".to_string()],
            mailbox: Some(SiiMailbox {
                rx_offset: 0x1000,
                rx_size: 128,
                tx_offset: 0x1080,
                tx_size: 128,
                protocols: SIM_MAILBOX_PROTOCOLS,
            }),
            sync_managers: SIM_SYNC_MANAGERS.to_vec(),
            tx_pdos: vec![SiiPdoSpec {
                index: 0x1A00,
                sync_manager: 3,
                entries: vec![SiiPdoEntry {
                    index: 0x6000,
                    subindex: 0,
                    bit_length: 8,
                }],
            }],
            rx_pdos: vec![SiiPdoSpec {
                index: 0x1600,
                sync_manager: 2,
                entries: vec![SiiPdoEntry {
                    index: 0x7000,
                    subindex: 0,
                    bit_length: 8,
                }],
            }],
            coe_details: SIM_COE_DETAILS,
            general_flags: 0,
            ebus_current: 0,
            dc: None,
        };
        let sii = SiiImage::from_spec(&spec);
        assert_eq!(sii.read_word(0x000A), 0xE5C2);
        assert_eq!(sii.read_word(SII_VERSION_WORD), SIM_SII_VERSION);
        assert!(sii.verify_checksum());
        // EEPROM 大小声明必须覆盖镜像自身。
        let declared = (usize::from(sii.read_word(SII_EEPROM_SIZE_WORD)) + 1) * 128;
        assert!(declared >= sii.len_words() * 2);
        assert_eq!(sii.sync_manager_count(), SIM_SYNC_MANAGERS.len());
        let tx = sii
            .category_bytes(category::TX_PDO)
            .expect("TxPDO category");
        assert_eq!(u16::from_le_bytes([tx[0], tx[1]]), 0x1A00);
        let strings = sii
            .category_bytes(category::STRINGS)
            .expect("strings category");
        assert!(
            strings.windows(9).any(|window| window == b"SIMULATED"),
            "设备名必须带 SIMULATED 标识（R2）"
        );
    }

    #[test]
    fn from_spec_without_mailbox_marks_fixed_area_as_absent() {
        let mut spec = simulated_spec();
        spec.mailbox = None;
        spec.tx_pdos.clear();
        spec.rx_pdos.clear();
        let sii = SiiImage::from_spec(&spec);
        for offset in 0x0014..=0x001B {
            assert_eq!(sii.read_word(offset), 0xFFFF, "word {offset:#06x}");
        }
        assert_eq!(sii.read_word(0x001C), 0);
        assert_eq!(sii.mailbox_protocols(), 0);
        assert!(sii.verify_checksum());
        assert!(sii.category_bytes(category::TX_PDO).is_none());
    }
}
