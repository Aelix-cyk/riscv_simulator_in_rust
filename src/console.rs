//! Linux terminal integration for the platform UART console.

use std::io;
use std::os::fd::RawFd;

use crate::platform::Platform;

/// Host input bytes inspected by each nonblocking read.
const INPUT_BUFFER_BYTES: usize = 256;

/// Control byte that starts a host terminal command.
const HOST_ESCAPE: u8 = 0x01;

/// Host command that exits the simulator.
const EXIT_COMMAND: u8 = b'x';

/// Result of supplying host input to the UART.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputStatus {
    /// Continue the simulation.
    Continue,
    /// The user requested simulator termination.
    ExitRequested,
}

/// Splits host escape commands from bytes destined for the guest UART.
#[derive(Debug, Default)]
pub struct UartInputBridge {
    escape_pending: bool,
    exit_requested: bool,
}

impl UartInputBridge {
    /// Queues guest input and recognizes `Ctrl-A x` and `Ctrl-A Ctrl-A`.
    pub fn feed(&mut self, bytes: &[u8], platform: &mut Platform) -> InputStatus {
        let (guest_bytes, status) = self.filter(bytes);
        for byte in guest_bytes {
            platform.push_uart_input(byte);
        }
        status
    }

    /// Separates host commands from bytes that should be queued for the guest.
    fn filter(&mut self, bytes: &[u8]) -> (Vec<u8>, InputStatus) {
        if self.exit_requested {
            return (Vec::new(), InputStatus::ExitRequested);
        }
        let mut guest_bytes = Vec::with_capacity(bytes.len());
        for &byte in bytes {
            if self.escape_pending {
                self.escape_pending = false;
                match byte {
                    HOST_ESCAPE => guest_bytes.push(HOST_ESCAPE),
                    EXIT_COMMAND => {
                        self.exit_requested = true;
                        return (guest_bytes, InputStatus::ExitRequested);
                    }
                    other => {
                        guest_bytes.push(HOST_ESCAPE);
                        guest_bytes.push(other);
                    }
                }
            } else if byte == HOST_ESCAPE {
                self.escape_pending = true;
            } else {
                guest_bytes.push(byte);
            }
        }
        (guest_bytes, InputStatus::Continue)
    }
}

/// A Linux terminal connected to the guest UART.
///
/// The original stdin flags and terminal attributes are restored when this
/// value is dropped, including during normal Rust panic unwinding.
pub struct InteractiveConsole {
    input: RawFd,
    output: RawFd,
    original_flags: libc::c_int,
    original_termios: Option<libc::termios>,
    input_closed: bool,
    bridge: UartInputBridge,
}

impl InteractiveConsole {
    /// Connects the process's standard input and output to the UART.
    pub fn new() -> io::Result<Self> {
        let input = libc::STDIN_FILENO;
        let output = libc::STDOUT_FILENO;
        let original_flags = get_file_flags(input)?;
        let raw_terminal = is_terminal(input) && is_terminal(output);
        let original_termios = if raw_terminal {
            let original = get_termios(input)?;
            set_raw_termios(input, &original)?;
            Some(original)
        } else {
            None
        };
        if let Err(error) = set_file_flags(input, original_flags | libc::O_NONBLOCK) {
            if let Some(original) = &original_termios {
                let _ = set_termios(input, original);
            }
            return Err(error);
        }

        Ok(Self {
            input,
            output,
            original_flags,
            original_termios,
            input_closed: false,
            bridge: UartInputBridge::default(),
        })
    }

    /// Streams guest output, polls host input, and recognizes host exits.
    pub fn service(&mut self, platform: &mut Platform) -> io::Result<InputStatus> {
        self.flush_output(platform)?;
        self.read_input(platform)
    }

    /// Flushes output generated immediately before simulation termination.
    pub fn finish(&mut self, platform: &mut Platform) -> io::Result<()> {
        self.flush_output(platform)
    }

    fn flush_output(&mut self, platform: &mut Platform) -> io::Result<()> {
        let output = platform.take_console_output();
        write_all(self.output, &output)
    }

    fn read_input(&mut self, platform: &mut Platform) -> io::Result<InputStatus> {
        if self.input_closed {
            return Ok(InputStatus::Continue);
        }
        let mut bytes = [0; INPUT_BUFFER_BYTES];
        loop {
            let count = unsafe {
                libc::read(
                    self.input,
                    bytes.as_mut_ptr().cast(),
                    bytes.len() as libc::size_t,
                )
            };
            if count > 0 {
                let count = count as usize;
                if self.bridge.feed(&bytes[..count], platform) == InputStatus::ExitRequested {
                    return Ok(InputStatus::ExitRequested);
                }
                continue;
            }
            if count == 0 {
                self.input_closed = true;
                return Ok(InputStatus::Continue);
            }
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::Interrupted => continue,
                io::ErrorKind::WouldBlock => return Ok(InputStatus::Continue),
                _ => return Err(error),
            }
        }
    }
}

impl Drop for InteractiveConsole {
    fn drop(&mut self) {
        if let Some(original) = &self.original_termios {
            let _ = set_termios(self.input, original);
        }
        let _ = set_file_flags(self.input, self.original_flags);
    }
}

fn is_terminal(fd: RawFd) -> bool {
    unsafe { libc::isatty(fd) == 1 }
}

fn get_file_flags(fd: RawFd) -> io::Result<libc::c_int> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(flags)
    }
}

fn set_file_flags(fd: RawFd, flags: libc::c_int) -> io::Result<()> {
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags) } == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn get_termios(fd: RawFd) -> io::Result<libc::termios> {
    let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
    if unsafe { libc::tcgetattr(fd, termios.as_mut_ptr()) } == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { termios.assume_init() })
    }
}

fn set_raw_termios(fd: RawFd, original: &libc::termios) -> io::Result<()> {
    let mut raw = *original;
    unsafe { libc::cfmakeraw(&mut raw) };
    set_termios(fd, &raw)
}

fn set_termios(fd: RawFd, termios: &libc::termios) -> io::Result<()> {
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, termios) } == -1 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn write_all(fd: RawFd, mut bytes: &[u8]) -> io::Result<()> {
    while !bytes.is_empty() {
        let count = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len() as libc::size_t) };
        if count > 0 {
            bytes = &bytes[count as usize..];
            continue;
        }
        if count == 0 {
            return Err(io::Error::from(io::ErrorKind::WriteZero));
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::{Bus, MemoryType};

    #[test]
    fn ordinary_host_bytes_reach_the_guest_uart() {
        let mut platform = Platform::default();
        let mut bridge = UartInputBridge::default();

        assert_eq!(bridge.feed(b"help\r", &mut platform), InputStatus::Continue);

        for expected in b"help\r" {
            assert_eq!(
                platform
                    .read_u8(crate::platform::UART_BASE, MemoryType::Io)
                    .unwrap(),
                *expected
            );
        }
    }

    #[test]
    fn ctrl_a_x_requests_exit_without_reaching_the_guest() {
        let mut platform = Platform::default();
        let mut bridge = UartInputBridge::default();

        assert_eq!(
            bridge.feed(&[HOST_ESCAPE, EXIT_COMMAND, b'z'], &mut platform),
            InputStatus::ExitRequested
        );
        assert_eq!(
            platform
                .read_u8(crate::platform::UART_BASE, MemoryType::Io)
                .unwrap(),
            0
        );
    }

    #[test]
    fn doubled_ctrl_a_is_forwarded_to_the_guest() {
        let mut platform = Platform::default();
        let mut bridge = UartInputBridge::default();

        bridge.feed(&[HOST_ESCAPE, HOST_ESCAPE], &mut platform);

        assert_eq!(
            platform
                .read_u8(crate::platform::UART_BASE, MemoryType::Io)
                .unwrap(),
            HOST_ESCAPE
        );
    }

    #[test]
    fn unknown_escape_forwards_both_bytes() {
        let mut platform = Platform::default();
        let mut bridge = UartInputBridge::default();

        bridge.feed(&[HOST_ESCAPE, b'c'], &mut platform);

        for expected in [HOST_ESCAPE, b'c'] {
            assert_eq!(
                platform
                    .read_u8(crate::platform::UART_BASE, MemoryType::Io)
                    .unwrap(),
                expected
            );
        }
    }

    #[test]
    fn escape_state_survives_chunk_boundaries() {
        let mut platform = Platform::default();
        let mut bridge = UartInputBridge::default();

        bridge.feed(&[b'a', HOST_ESCAPE], &mut platform);
        assert_eq!(
            bridge.feed(&[EXIT_COMMAND], &mut platform),
            InputStatus::ExitRequested
        );

        assert_eq!(
            platform
                .read_u8(crate::platform::UART_BASE, MemoryType::Io)
                .unwrap(),
            b'a'
        );
        assert_eq!(
            platform
                .read_u8(crate::platform::UART_BASE, MemoryType::Io)
                .unwrap(),
            0
        );
    }
}
