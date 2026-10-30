//! Platform bus with RAM, a timer, UART, interrupt controller, and exit device.

use std::collections::VecDeque;
use std::fmt;

use crate::bus::{Bus, InterruptLines, MemoryType, check_write_bytes};
use crate::memory::{DramLayout, DramLayoutError, Memory, MemoryError};
use crate::trap::Trap;

/// Base address of the platform test/exit device.
pub const SYSCON_BASE: u64 = 0x0010_0000;
/// Size of the system-control device address window.
pub const SYSCON_SIZE: u64 = 0x1000;
/// Base address of the core-local interruptor (CLINT).
pub const CLINT_BASE: u64 = 0x0200_0000;
/// Size of the CLINT address window.
pub const CLINT_SIZE: u64 = 0x1_0000;
/// Base address of the platform-level interrupt controller (PLIC).
pub const PLIC_BASE: u64 = 0x0c00_0000;
/// Size of the PLIC address window.
pub const PLIC_SIZE: u64 = 0x40_0000;
/// Base address of the NS16550-compatible UART.
pub const UART_BASE: u64 = 0x1000_0000;
/// Size of the UART address window.
pub const UART_SIZE: u64 = 0x100;

const SYSCON_WORD_BYTES: u64 = 4;
const SYSCON_PASS: u16 = 0x5555;
const SYSCON_FAIL: u16 = 0x3333;
const FAILURE_EXIT_CODE: u8 = 1;

/// Offset from [`CLINT_BASE`] to the machine timer compare register.
pub const CLINT_MTIMECMP_OFFSET: u64 = 0x4000;
/// Offset from [`CLINT_BASE`] to the machine time register.
pub const CLINT_MTIME_OFFSET: u64 = 0xbff8;
const CLINT_REGISTER_BYTES: u64 = 8;
const CLINT_MSIP_BYTES: u64 = 4;
const CLINT_MSIP_PENDING: u32 = 1;

const UART_RBR_OFFSET: u64 = 0;
const UART_IER_OFFSET: u64 = 1;
const UART_IIR_OFFSET: u64 = 2;
const UART_LSR_OFFSET: u64 = 5;
const UART_INTERRUPT_ENABLE_RECEIVE: u8 = 1;
const UART_INTERRUPT_ID_RECEIVE: u8 = 0x04;
const UART_INTERRUPT_ID_NONE: u8 = 0x01;
const UART_LINE_STATUS_DATA_READY: u8 = 0x01;
const UART_LINE_STATUS_TRANSMIT_HOLDING_EMPTY: u8 = 0x20;
const UART_LINE_STATUS_TRANSMITTER_EMPTY: u8 = 0x40;

const PLIC_PRIORITY_OFFSET: u64 = 0;
const PLIC_PENDING_OFFSET: u64 = 0x1000;
const PLIC_ENABLE_OFFSET: u64 = 0x2000;
const PLIC_CONTEXT_STRIDE: u64 = 0x1000;
const PLIC_CONTEXT_ENABLE_STRIDE: u64 = 0x80;
const PLIC_CONTEXT_ENABLE_BYTES: u64 = PLIC_CONTEXT_ENABLE_STRIDE * PLIC_CONTEXT_COUNT as u64;
const PLIC_CONTEXT_BASE: u64 = 0x20_0000;
const PLIC_REGISTER_BYTES: u64 = 4;
const PLIC_CONTEXT_REGISTERS_BYTES: u64 = PLIC_REGISTER_BYTES * 2;
/// Offset of the first PLIC context's claim/complete register.
pub const PLIC_CONTEXT_CLAIM_OFFSET: u64 = PLIC_CONTEXT_BASE + PLIC_REGISTER_BYTES;
const PLIC_MACHINE_CONTEXT: usize = 0;
const PLIC_SUPERVISOR_CONTEXT: usize = 1;
const PLIC_CONTEXT_COUNT: usize = 2;
const PLIC_SOURCE_COUNT: usize = 2;
const PLIC_FIRST_SOURCE: usize = 1;
const PLIC_SOURCE_BIT_ONE: u32 = 1;
const PLIC_UART_SOURCE: u32 = 1;
const PLIC_NO_SOURCE: u32 = 0;
const PLIC_MAX_PRIORITY: u32 = 7;

const MMIO_RANGES: [(&str, u64, u64); 4] = [
    ("Syscon", SYSCON_BASE, SYSCON_SIZE),
    ("CLINT", CLINT_BASE, CLINT_SIZE),
    ("PLIC", PLIC_BASE, PLIC_SIZE),
    ("UART", UART_BASE, UART_SIZE),
];

/// Why a platform could not be constructed for a DRAM layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformInitError {
    /// The backing memory does not describe a valid architectural range.
    InvalidLayout(DramLayoutError),
    /// The DRAM range overlaps fixed platform MMIO.
    OverlapsMmio(&'static str),
    /// The requested host allocation failed.
    Memory(MemoryError),
}

impl fmt::Display for PlatformInitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLayout(error) => error.fmt(formatter),
            Self::OverlapsMmio(device) => {
                write!(formatter, "DRAM address range overlaps {device} MMIO")
            }
            Self::Memory(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for PlatformInitError {}

/// A physical platform bus with configurable DRAM and fixed memory-mapped devices.
pub struct Platform {
    memory: Memory,
    clock: u64,
    timer_compare: u64,
    machine_software_pending: u32,
    uart: Uart,
    plic: Plic,
    syscon: [u8; SYSCON_WORD_BYTES as usize],
    exit_code: Option<u8>,
}

impl Platform {
    /// Creates a platform with the supplied RAM.
    ///
    /// # Panics
    ///
    /// Panics if the supplied memory does not describe a valid, non-overlapping
    /// DRAM range.
    pub fn new(memory: Memory) -> Self {
        Self::try_new(memory).expect("invalid platform DRAM layout")
    }

    /// Creates a platform with validated DRAM for every access and image load.
    pub fn try_new(memory: Memory) -> Result<Self, PlatformInitError> {
        let layout = DramLayout::new(memory.base(), memory.len())
            .map_err(PlatformInitError::InvalidLayout)?;
        Self::validate_mmio_nonoverlap(layout)?;
        Ok(Self::from_memory(memory))
    }

    /// Validates and allocates a platform DRAM range.
    pub fn with_layout(layout: DramLayout) -> Result<Self, PlatformInitError> {
        Self::validate_mmio_nonoverlap(layout)?;
        let memory = Memory::try_with_layout(layout).map_err(PlatformInitError::Memory)?;
        Ok(Self::from_memory(memory))
    }

    fn from_memory(memory: Memory) -> Self {
        Self {
            memory,
            clock: 0,
            timer_compare: u64::MAX,
            machine_software_pending: 0,
            uart: Uart::default(),
            plic: Plic::default(),
            syscon: [0; SYSCON_WORD_BYTES as usize],
            exit_code: None,
        }
    }

    fn validate_mmio_nonoverlap(layout: DramLayout) -> Result<(), PlatformInitError> {
        for (device, base, size) in MMIO_RANGES {
            let mmio_end = base + size;
            if layout.base() < mmio_end && base < layout.end() {
                return Err(PlatformInitError::OverlapsMmio(device));
            }
        }
        Ok(())
    }

    /// Queues one byte for the UART to receive.
    pub fn push_uart_input(&mut self, byte: u8) {
        self.uart.receive.push_back(byte);
    }

    /// Removes and returns all console output produced so far.
    pub fn take_console_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.uart.output)
    }

    fn dram_offset(&self, address: u64, width: u64) -> Result<u64, Trap> {
        let offset = address
            .checked_sub(self.memory.base())
            .ok_or(Trap::AccessFault)?;
        let end = offset.checked_add(width).ok_or(Trap::AccessFault)?;
        if end > self.memory.len() {
            return Err(Trap::AccessFault);
        }
        Ok(offset)
    }

    fn read_clint_u8(&self, offset: u64) -> Result<u8, Trap> {
        if offset < CLINT_MSIP_BYTES {
            return byte_of(self.machine_software_pending, offset, CLINT_MSIP_BYTES);
        }
        if address_in_register(offset, CLINT_MTIMECMP_OFFSET, CLINT_REGISTER_BYTES) {
            return byte_of(
                self.timer_compare,
                offset - CLINT_MTIMECMP_OFFSET,
                CLINT_REGISTER_BYTES,
            );
        }
        if address_in_register(offset, CLINT_MTIME_OFFSET, CLINT_REGISTER_BYTES) {
            return byte_of(
                self.clock,
                offset - CLINT_MTIME_OFFSET,
                CLINT_REGISTER_BYTES,
            );
        }
        Err(Trap::AccessFault)
    }

    fn write_clint_u8(&mut self, offset: u64, value: u8) -> Result<(), Trap> {
        if offset < CLINT_MSIP_BYTES {
            write_byte_of(
                &mut self.machine_software_pending,
                offset,
                value,
                CLINT_MSIP_BYTES,
            );
            self.machine_software_pending &= CLINT_MSIP_PENDING;
            return Ok(());
        }
        if address_in_register(offset, CLINT_MTIMECMP_OFFSET, CLINT_REGISTER_BYTES) {
            write_byte_of(
                &mut self.timer_compare,
                offset - CLINT_MTIMECMP_OFFSET,
                value,
                CLINT_REGISTER_BYTES,
            );
            return Ok(());
        }
        if address_in_register(offset, CLINT_MTIME_OFFSET, CLINT_REGISTER_BYTES) {
            write_byte_of(
                &mut self.clock,
                offset - CLINT_MTIME_OFFSET,
                value,
                CLINT_REGISTER_BYTES,
            );
            return Ok(());
        }
        Err(Trap::AccessFault)
    }

    fn read_plic_u8(&mut self, offset: u64) -> Result<u8, Trap> {
        self.plic.read_byte(offset, self.uart.interrupt_pending())
    }

    fn write_plic_u8(&mut self, offset: u64, value: u8) -> Result<(), Trap> {
        self.plic.write_byte(offset, value)
    }

    fn write_syscon_u8(&mut self, offset: u64, value: u8) -> Result<(), Trap> {
        if offset >= SYSCON_WORD_BYTES {
            return Err(Trap::AccessFault);
        }
        self.syscon[offset as usize] = value;
        if offset + 1 == SYSCON_WORD_BYTES {
            let word = u32::from_le_bytes(self.syscon);
            let status = word as u16;
            if status == SYSCON_PASS {
                self.exit_code = Some(0);
            } else if status == SYSCON_FAIL {
                let requested = (word >> u16::BITS) as u8;
                self.exit_code = Some(if requested == 0 {
                    FAILURE_EXIT_CODE
                } else {
                    requested
                });
            }
        }
        Ok(())
    }
}

impl Default for Platform {
    fn default() -> Self {
        Self::with_layout(DramLayout::default()).expect("default platform layout must be valid")
    }
}

impl Bus for Platform {
    fn read_u8(&mut self, address: u64, memory_type: MemoryType) -> Result<u8, Trap> {
        if offset_in(address, SYSCON_BASE, SYSCON_SIZE).is_some() {
            return Ok(0);
        }
        if let Some(offset) = offset_in(address, CLINT_BASE, CLINT_SIZE) {
            return self.read_clint_u8(offset);
        }
        if self.dram_offset(address, 1).is_ok() {
            return self.memory.read_u8(address, memory_type);
        }
        if let Some(offset) = offset_in(address, PLIC_BASE, PLIC_SIZE) {
            return self.read_plic_u8(offset);
        }
        if let Some(offset) = offset_in(address, UART_BASE, UART_SIZE) {
            return self.uart.read_byte(offset);
        }
        Err(Trap::AccessFault)
    }

    fn write_u8(&mut self, address: u64, value: u8, memory_type: MemoryType) -> Result<(), Trap> {
        if let Some(offset) = offset_in(address, SYSCON_BASE, SYSCON_SIZE) {
            return self.write_syscon_u8(offset, value);
        }
        if let Some(offset) = offset_in(address, CLINT_BASE, CLINT_SIZE) {
            return self.write_clint_u8(offset, value);
        }
        if self.dram_offset(address, 1).is_ok() {
            return self.memory.write_u8(address, value, memory_type);
        }
        if let Some(offset) = offset_in(address, PLIC_BASE, PLIC_SIZE) {
            return self.write_plic_u8(offset, value);
        }
        if let Some(offset) = offset_in(address, UART_BASE, UART_SIZE) {
            return self.uart.write_byte(offset, value);
        }
        Err(Trap::AccessFault)
    }

    fn load_bytes(
        &mut self,
        address: u64,
        bytes: &[u8],
        memory_type: MemoryType,
    ) -> Result<(), Trap> {
        let length = bytes.len() as u64;
        self.dram_offset(address, length)?;
        self.memory.load_bytes(address, bytes, memory_type)
    }

    fn check_write(
        &mut self,
        address: u64,
        width: u64,
        memory_type: MemoryType,
    ) -> Result<(), Trap> {
        if self.dram_offset(address, width).is_ok() {
            Ok(())
        } else {
            check_write_bytes(self, address, width, memory_type)
        }
    }

    fn tick(&mut self) {
        self.clock = self.clock.wrapping_add(1);
    }

    fn interrupt_lines(&self) -> InterruptLines {
        let uart_pending = self.uart.interrupt_pending();
        InterruptLines {
            machine_software: self.machine_software_pending & CLINT_MSIP_PENDING != 0,
            machine_timer: self.clock >= self.timer_compare,
            machine_external: self
                .plic
                .context_requested(PLIC_MACHINE_CONTEXT, uart_pending),
            supervisor_software: false,
            supervisor_timer: false,
            supervisor_external: self
                .plic
                .context_requested(PLIC_SUPERVISOR_CONTEXT, uart_pending),
        }
    }

    fn exit_code(&self) -> Option<u8> {
        self.exit_code
    }
}

#[derive(Default)]
struct Uart {
    interrupt_enable: u8,
    receive: VecDeque<u8>,
    output: Vec<u8>,
}

impl Uart {
    fn interrupt_pending(&self) -> bool {
        self.interrupt_enable & UART_INTERRUPT_ENABLE_RECEIVE != 0 && !self.receive.is_empty()
    }

    fn read_byte(&mut self, offset: u64) -> Result<u8, Trap> {
        match offset {
            UART_RBR_OFFSET => Ok(self.receive.pop_front().unwrap_or(0)),
            UART_IER_OFFSET => Ok(self.interrupt_enable),
            UART_IIR_OFFSET => Ok(if self.interrupt_pending() {
                UART_INTERRUPT_ID_RECEIVE
            } else {
                UART_INTERRUPT_ID_NONE
            }),
            UART_LSR_OFFSET => Ok(UART_LINE_STATUS_TRANSMIT_HOLDING_EMPTY
                | UART_LINE_STATUS_TRANSMITTER_EMPTY
                | if self.receive.is_empty() {
                    0
                } else {
                    UART_LINE_STATUS_DATA_READY
                }),
            offset if offset < UART_SIZE => Ok(0),
            _ => Err(Trap::AccessFault),
        }
    }

    fn write_byte(&mut self, offset: u64, value: u8) -> Result<(), Trap> {
        match offset {
            UART_RBR_OFFSET => self.output.push(value),
            UART_IER_OFFSET => {
                self.interrupt_enable = value & UART_INTERRUPT_ENABLE_RECEIVE;
            }
            offset if offset < UART_SIZE => {}
            _ => return Err(Trap::AccessFault),
        }
        Ok(())
    }
}

struct Plic {
    priorities: [u32; PLIC_SOURCE_COUNT],
    enables: [u32; PLIC_CONTEXT_COUNT],
    thresholds: [u32; PLIC_CONTEXT_COUNT],
    claimed: [u32; PLIC_CONTEXT_COUNT],
}

impl Default for Plic {
    fn default() -> Self {
        Self {
            priorities: [0; PLIC_SOURCE_COUNT],
            enables: [0; PLIC_CONTEXT_COUNT],
            thresholds: [0; PLIC_CONTEXT_COUNT],
            claimed: [PLIC_NO_SOURCE; PLIC_CONTEXT_COUNT],
        }
    }
}

impl Plic {
    fn pending_sources(&self, uart_pending: bool) -> u32 {
        u32::from(uart_pending) << PLIC_UART_SOURCE
    }

    fn source_priority(&self, source: u32, pending: u32) -> u32 {
        if pending & (PLIC_SOURCE_BIT_ONE << source) == 0 {
            return 0;
        }
        self.priorities[source as usize]
    }

    fn context_requested(&self, context: usize, uart_pending: bool) -> bool {
        if self
            .claimed
            .iter()
            .any(|claimed| *claimed != PLIC_NO_SOURCE)
        {
            return false;
        }
        let pending = self.pending_sources(uart_pending);
        (PLIC_FIRST_SOURCE..PLIC_SOURCE_COUNT).any(|source| {
            let bit = PLIC_SOURCE_BIT_ONE << source;
            self.enables[context] & bit != 0
                && self.source_priority(source as u32, pending) > self.thresholds[context]
        })
    }

    fn read_byte(&mut self, offset: u64, uart_pending: bool) -> Result<u8, Trap> {
        if address_in_register(
            offset,
            PLIC_PRIORITY_OFFSET,
            PLIC_SOURCE_COUNT as u64 * PLIC_REGISTER_BYTES,
        ) {
            let source = (offset / PLIC_REGISTER_BYTES) as usize;
            return byte_of(
                self.priorities[source],
                offset % PLIC_REGISTER_BYTES,
                PLIC_REGISTER_BYTES,
            );
        }
        if address_in_register(offset, PLIC_PENDING_OFFSET, PLIC_REGISTER_BYTES) {
            return byte_of(
                self.pending_sources(uart_pending),
                offset - PLIC_PENDING_OFFSET,
                PLIC_REGISTER_BYTES,
            );
        }
        if let Some(context) = plic_context_enable(offset) {
            let within = offset - PLIC_ENABLE_OFFSET;
            let register = (within % PLIC_CONTEXT_ENABLE_STRIDE) / PLIC_REGISTER_BYTES;
            let byte = within % PLIC_REGISTER_BYTES;
            if register == 0 {
                return byte_of(self.enables[context], byte, PLIC_REGISTER_BYTES);
            }
            return Ok(0);
        }
        if let Some((context, register)) = plic_context_register(offset) {
            if register == 0 {
                return byte_of(
                    self.thresholds[context],
                    offset % PLIC_REGISTER_BYTES,
                    PLIC_REGISTER_BYTES,
                );
            }
            if register == 1 {
                if offset.is_multiple_of(PLIC_REGISTER_BYTES) {
                    let claimed = self.claim(context, uart_pending);
                    return byte_of(claimed, 0, PLIC_REGISTER_BYTES);
                }
                return Ok(0);
            }
        }
        Err(Trap::AccessFault)
    }

    fn write_byte(&mut self, offset: u64, value: u8) -> Result<(), Trap> {
        if address_in_register(
            offset,
            PLIC_PRIORITY_OFFSET,
            PLIC_SOURCE_COUNT as u64 * PLIC_REGISTER_BYTES,
        ) {
            let source = (offset / PLIC_REGISTER_BYTES) as usize;
            write_byte_of(
                &mut self.priorities[source],
                offset % PLIC_REGISTER_BYTES,
                value,
                PLIC_REGISTER_BYTES,
            );
            self.priorities[source] &= PLIC_MAX_PRIORITY;
            return Ok(());
        }
        if let Some(context) = plic_context_enable(offset) {
            let within = offset - PLIC_ENABLE_OFFSET;
            let register = (within % PLIC_CONTEXT_ENABLE_STRIDE) / PLIC_REGISTER_BYTES;
            let byte = within % PLIC_REGISTER_BYTES;
            if register == 0 {
                write_byte_of(&mut self.enables[context], byte, value, PLIC_REGISTER_BYTES);
            }
            return Ok(());
        }
        if let Some((context, register)) = plic_context_register(offset) {
            if register == 0 {
                write_byte_of(
                    &mut self.thresholds[context],
                    offset % PLIC_REGISTER_BYTES,
                    value,
                    PLIC_REGISTER_BYTES,
                );
                return Ok(());
            }
            if register == 1 {
                if offset.is_multiple_of(PLIC_REGISTER_BYTES) {
                    let source = u32::from(value);
                    if self.claimed[context] == source {
                        self.claimed[context] = PLIC_NO_SOURCE;
                    }
                }
                return Ok(());
            }
        }
        Err(Trap::AccessFault)
    }

    fn claim(&mut self, context: usize, uart_pending: bool) -> u32 {
        if self.claimed[context] != PLIC_NO_SOURCE {
            return PLIC_NO_SOURCE;
        }
        if !self.context_requested(context, uart_pending) {
            return PLIC_NO_SOURCE;
        }
        self.claimed[context] = PLIC_UART_SOURCE;
        PLIC_UART_SOURCE
    }
}

fn plic_context_enable(offset: u64) -> Option<usize> {
    let relative = offset.checked_sub(PLIC_ENABLE_OFFSET)?;
    if relative >= PLIC_CONTEXT_ENABLE_BYTES {
        return None;
    }
    Some((relative / PLIC_CONTEXT_ENABLE_STRIDE) as usize)
}

fn plic_context_register(offset: u64) -> Option<(usize, u64)> {
    for context in 0..PLIC_CONTEXT_COUNT {
        let base = PLIC_CONTEXT_BASE + context as u64 * PLIC_CONTEXT_STRIDE;
        if let Some(register_offset) = offset.checked_sub(base)
            && register_offset < PLIC_CONTEXT_REGISTERS_BYTES
        {
            return Some((context, register_offset / PLIC_REGISTER_BYTES));
        }
    }
    None
}

fn address_in_register(offset: u64, base: u64, width: u64) -> bool {
    offset >= base && offset - base < width
}

fn offset_in(address: u64, base: u64, size: u64) -> Option<u64> {
    address.checked_sub(base).filter(|offset| *offset < size)
}

fn byte_of<T>(value: T, offset: u64, width: u64) -> Result<u8, Trap>
where
    T: Into<u64> + Copy,
{
    if offset >= width {
        return Err(Trap::AccessFault);
    }
    Ok(((value.into() >> (offset * u8::BITS as u64)) & u8::MAX as u64) as u8)
}

fn write_byte_of<T>(value: &mut T, offset: u64, byte: u8, width: u64)
where
    T: Into<u64> + Copy + TryFrom<u64>,
{
    if offset >= width {
        return;
    }
    let shift = offset * u8::BITS as u64;
    let mask = u64::from(u8::MAX) << shift;
    let updated = ((*value).into() & !mask) | (u64::from(byte) << shift);
    if let Ok(converted) = T::try_from(updated) {
        *value = converted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{DRAM_BASE, DRAM_PAGE_BYTES};

    const SMALL_MEMORY_BYTES: usize = DRAM_PAGE_BYTES as usize;
    const TEST_DRAM_BYTES: u64 = 0x2000;
    const TEST_CUSTOM_DRAM_BASE: u64 = 0x9000_0000;
    const TIMER_DELAY: u64 = 5;
    const UART_RECEIVE_ENABLE: u8 = 1;
    const PLIC_SOURCE: u64 = 1;
    const PLIC_CONTEXT_ENABLE_BYTES: u64 = 0x80;
    const PLIC_CONTEXT_REGISTER_BYTES: u64 = 0x1000;
    const PLIC_PRIORITY: u32 = 1;
    const PLIC_ENABLED_SOURCE: u32 = 1 << PLIC_SOURCE;

    fn platform() -> Platform {
        Platform::new(Memory::with_size(SMALL_MEMORY_BYTES))
    }

    #[test]
    fn ram_and_unknown_addresses_use_the_platform_map() {
        let mut platform = platform();

        platform
            .write_u64(DRAM_BASE, u64::MAX, MemoryType::Cacheable)
            .unwrap();
        assert_eq!(
            platform.read_u64(DRAM_BASE, MemoryType::Cacheable).unwrap(),
            u64::MAX
        );
        assert_eq!(
            platform.read_u8(0, MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
    }

    #[test]
    fn clint_time_reaches_compare_and_asserts_machine_timer() {
        let mut platform = platform();
        platform
            .write_u64(
                CLINT_BASE + CLINT_MTIMECMP_OFFSET,
                TIMER_DELAY,
                MemoryType::Io,
            )
            .unwrap();

        for _ in 0..TIMER_DELAY {
            assert!(!platform.interrupt_lines().machine_timer);
            platform.tick();
        }

        assert!(platform.interrupt_lines().machine_timer);
        assert_eq!(
            platform
                .read_u64(CLINT_BASE + CLINT_MTIME_OFFSET, MemoryType::Io)
                .unwrap(),
            TIMER_DELAY
        );
    }

    #[test]
    fn clint_software_interrupt_tracks_msip_bit_zero() {
        let mut platform = platform();

        platform
            .write_u32(CLINT_BASE, u32::MAX, MemoryType::Io)
            .unwrap();

        assert!(platform.interrupt_lines().machine_software);
        assert_eq!(
            platform.read_u32(CLINT_BASE, MemoryType::Io).unwrap(),
            CLINT_MSIP_PENDING
        );
    }

    #[test]
    fn uart_buffers_output_and_exposes_receive_interrupts() {
        let mut platform = platform();
        platform.write_u8(UART_BASE, b'A', MemoryType::Io).unwrap();
        platform
            .write_u8(
                UART_BASE + UART_IER_OFFSET,
                UART_RECEIVE_ENABLE,
                MemoryType::Io,
            )
            .unwrap();
        platform.push_uart_input(b'B');

        assert!(platform.uart.interrupt_pending());
        assert_eq!(platform.read_u8(UART_BASE, MemoryType::Io).unwrap(), b'B');
        assert_eq!(platform.take_console_output(), b"A");
    }

    #[test]
    fn plic_routes_uart_interrupts_to_enabled_contexts() {
        let mut platform = platform();
        configure_uart_irq(&mut platform, PLIC_MACHINE_CONTEXT);
        configure_uart_irq(&mut platform, PLIC_SUPERVISOR_CONTEXT);
        platform.push_uart_input(b'x');

        assert!(platform.interrupt_lines().machine_external);
        assert!(platform.interrupt_lines().supervisor_external);

        let claim_address = PLIC_BASE
            + PLIC_CONTEXT_CLAIM_OFFSET
            + PLIC_MACHINE_CONTEXT as u64 * PLIC_CONTEXT_REGISTER_BYTES;
        assert_eq!(
            platform.read_u32(claim_address, MemoryType::Io).unwrap(),
            PLIC_UART_SOURCE
        );
        assert_eq!(platform.read_u32(claim_address, MemoryType::Io).unwrap(), 0);
        assert!(!platform.interrupt_lines().machine_external);
        assert!(!platform.interrupt_lines().supervisor_external);

        let supervisor_claim_address = PLIC_BASE
            + PLIC_CONTEXT_CLAIM_OFFSET
            + PLIC_SUPERVISOR_CONTEXT as u64 * PLIC_CONTEXT_REGISTER_BYTES;
        assert_eq!(
            platform
                .read_u32(supervisor_claim_address, MemoryType::Io)
                .unwrap(),
            0
        );

        platform
            .write_u32(claim_address, PLIC_UART_SOURCE, MemoryType::Io)
            .unwrap();
        assert!(platform.interrupt_lines().machine_external);
        assert!(platform.interrupt_lines().supervisor_external);
        assert_eq!(
            platform
                .read_u32(supervisor_claim_address, MemoryType::Io)
                .unwrap(),
            PLIC_UART_SOURCE
        );
        platform
            .write_u32(supervisor_claim_address, PLIC_UART_SOURCE, MemoryType::Io)
            .unwrap();
    }

    #[test]
    fn custom_layout_uses_one_dram_window_for_loads_and_accesses() {
        let layout = DramLayout::new(TEST_CUSTOM_DRAM_BASE, TEST_DRAM_BYTES).unwrap();
        let mut platform = Platform::with_layout(layout).unwrap();
        let last_byte = layout.end() - 1;
        platform
            .load_bytes(last_byte, &[0xaa], MemoryType::Cacheable)
            .unwrap();

        assert_eq!(
            platform
                .read_u8(layout.base(), MemoryType::Cacheable)
                .unwrap(),
            0
        );
        let first_byte_outside = layout.end();
        assert_eq!(
            platform.load_bytes(first_byte_outside, &[0xbb], MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
        assert_eq!(
            platform.read_u8(first_byte_outside, MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
        assert_eq!(
            platform.write_u8(first_byte_outside, 0xbb, MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
        assert_eq!(
            platform.read_u8(last_byte, MemoryType::Cacheable).unwrap(),
            0xaa
        );
    }

    #[test]
    fn dram_write_preflight_checks_whole_ranges_and_preserves_boundaries() {
        let mut platform = platform();
        let last_byte = DRAM_BASE + SMALL_MEMORY_BYTES as u64 - 1;

        platform
            .check_write(DRAM_BASE, SMALL_MEMORY_BYTES as u64, MemoryType::Cacheable)
            .unwrap();
        assert_eq!(
            platform.check_write(last_byte, 2, MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
        assert_eq!(
            platform.check_write(DRAM_BASE - 1, 2, MemoryType::Cacheable),
            Err(Trap::AccessFault)
        );
    }

    #[test]
    fn custom_dram_layout_rejects_mmio_overlap() {
        for (base, size) in [
            (SYSCON_BASE, DRAM_PAGE_BYTES),
            (CLINT_BASE, DRAM_PAGE_BYTES),
            (PLIC_BASE, DRAM_PAGE_BYTES),
            (UART_BASE, DRAM_PAGE_BYTES),
        ] {
            let layout = DramLayout::new(base, size).unwrap();
            assert!(
                matches!(
                    Platform::with_layout(layout),
                    Err(PlatformInitError::OverlapsMmio(_))
                ),
                "{base:#x}"
            );
        }
    }

    #[test]
    fn syscon_reports_pass_and_failure_exit_status() {
        let mut pass = platform();
        assert_eq!(
            pass.read_u32(SYSCON_BASE, MemoryType::Io).unwrap(),
            0,
            "syscon must support the read-modify-write used by firmware reset"
        );
        pass.write_u32(SYSCON_BASE, u32::from(SYSCON_PASS), MemoryType::Io)
            .unwrap();
        assert_eq!(pass.exit_code(), Some(0));

        let mut failure = platform();
        failure
            .write_u32(
                SYSCON_BASE,
                u32::from(SYSCON_FAIL) | (u32::from(FAILURE_EXIT_CODE) << u16::BITS),
                MemoryType::Io,
            )
            .unwrap();
        assert_eq!(failure.exit_code(), Some(FAILURE_EXIT_CODE));
    }

    fn configure_uart_irq(platform: &mut Platform, context: usize) {
        platform
            .write_u32(
                PLIC_BASE + PLIC_SOURCE * PLIC_REGISTER_BYTES,
                PLIC_PRIORITY,
                MemoryType::Io,
            )
            .unwrap();
        platform
            .write_u8(
                UART_BASE + UART_IER_OFFSET,
                UART_RECEIVE_ENABLE,
                MemoryType::Io,
            )
            .unwrap();
        platform
            .write_u32(
                PLIC_BASE + PLIC_ENABLE_OFFSET + context as u64 * PLIC_CONTEXT_ENABLE_BYTES,
                PLIC_ENABLED_SOURCE,
                MemoryType::Io,
            )
            .unwrap();
    }
}
