//! Command-line options for the simulator binary.

use std::path::PathBuf;

use crate::memory::DramLayout;

const KIB: u64 = 1 << 10;
const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;
const BINARY_SIZE_SUFFIXES: [(&str, u64); 3] = [("KiB", KIB), ("MiB", MIB), ("GiB", GIB)];
const HEX_PREFIX: &str = "0x";
const DECIMAL_RADIX: u32 = 10;
const HEXADECIMAL_RADIX: u32 = 16;

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Run the given program.
    Run(Options),
    /// Print usage and exit successfully.
    Help,
}

/// Options for a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Flat binary to load at the configured DRAM base.
    pub binary: PathBuf,
    /// Print each executed instruction.
    pub trace: bool,
    /// Steps to take before giving up.
    pub max_steps: u64,
    /// Configured DRAM address range.
    pub dram: DramLayout,
}

/// Steps a run takes before giving up, unless `--max-steps` says otherwise.
pub const DEFAULT_MAX_STEPS: u64 = 10_000;

/// Usage text, printed for `--help` and for bad arguments.
pub const USAGE: &str = "\
usage: riscv_simulator_in_rust --binary <FILE> [--trace] [--max-steps <N>]
       [--dram-base <ADDRESS>] [--dram-size <SIZE>]

  --binary <FILE>   flat binary to load at the configured DRAM base and run
  --trace           print each executed instruction
  --max-steps <N>   stop after N steps (default 10000)
  --dram-base <ADDRESS>  DRAM base (default 0x80000000)
  --dram-size <SIZE>     DRAM size (default 128MiB)
  --help            print this help
";

/// Parses arguments, excluding the program name.
pub fn parse<I>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut binary = None;
    let mut trace = false;
    let mut max_steps = DEFAULT_MAX_STEPS;
    let mut dram_base = DramLayout::default().base();
    let mut dram_size = DramLayout::default().size();

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--binary" => {
                let value = args.next().ok_or("--binary needs a file")?;
                binary = Some(PathBuf::from(value));
            }
            "--trace" => trace = true,
            "--max-steps" => {
                let value = args.next().ok_or("--max-steps needs a number")?;
                max_steps = value
                    .parse()
                    .map_err(|_| format!("--max-steps: {value:?} is not a number"))?;
            }
            "--dram-base" => {
                let value = args.next().ok_or("--dram-base needs an address")?;
                dram_base = parse_number("--dram-base", &value)?;
            }
            "--dram-size" => {
                let value = args.next().ok_or("--dram-size needs a size")?;
                dram_size = parse_size(&value)?;
            }
            "--help" => return Ok(Command::Help),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }

    let binary = binary.ok_or("--binary <FILE> is required")?;
    let dram = DramLayout::new(dram_base, dram_size)
        .map_err(|error| format!("invalid DRAM layout: {error}"))?;
    Ok(Command::Run(Options {
        binary,
        trace,
        max_steps,
        dram,
    }))
}

fn parse_size(value: &str) -> Result<u64, String> {
    for (suffix, multiplier) in BINARY_SIZE_SUFFIXES {
        if let Some(number) = value.strip_suffix(suffix) {
            let bytes = parse_number("--dram-size", number)?;
            return bytes
                .checked_mul(multiplier)
                .ok_or_else(|| format!("--dram-size: {value:?} is too large"));
        }
    }
    parse_number("--dram-size", value)
}

fn parse_number(option: &str, value: &str) -> Result<u64, String> {
    if let Some(hexadecimal) = value.strip_prefix(HEX_PREFIX) {
        u64::from_str_radix(hexadecimal, HEXADECIMAL_RADIX)
            .map_err(|_| format!("{option}: {value:?} is not a number"))
    } else {
        u64::from_str_radix(value, DECIMAL_RADIX)
            .map_err(|_| format!("{option}: {value:?} is not a number"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(|arg| arg.to_string()))
    }

    #[test]
    fn binary_only_uses_defaults() {
        let command = parse_args(&["--binary", "loop.bin"]).unwrap();

        assert_eq!(
            command,
            Command::Run(Options {
                binary: PathBuf::from("loop.bin"),
                trace: false,
                max_steps: DEFAULT_MAX_STEPS,
                dram: DramLayout::default(),
            })
        );
    }

    #[test]
    fn trace_and_step_limit_are_parsed() {
        let command = parse_args(&["--binary", "loop.bin", "--trace", "--max-steps", "7"]).unwrap();

        match command {
            Command::Run(options) => {
                assert!(options.trace);
                assert_eq!(options.max_steps, 7);
            }
            Command::Help => panic!("expected a run"),
        }
    }

    #[test]
    fn dram_layout_accepts_hexadecimal_and_binary_units() {
        for base in ["2415919104", "0x90000000"] {
            let command = parse_args(&[
                "--binary",
                "loop.bin",
                "--dram-base",
                base,
                "--dram-size",
                "16MiB",
            ])
            .unwrap();

            match command {
                Command::Run(options) => {
                    assert_eq!(options.dram.base(), 0x9000_0000);
                    assert_eq!(options.dram.size(), 16 * MIB);
                }
                Command::Help => panic!("expected a run"),
            }
        }
    }

    #[test]
    fn dram_size_accepts_all_binary_units_and_raw_bytes() {
        for (value, expected) in [
            ("4096", 4 * KIB),
            ("4KiB", 4 * KIB),
            ("2MiB", 2 * MIB),
            ("1GiB", GIB),
        ] {
            let command = parse_args(&["--binary", "loop.bin", "--dram-size", value]).unwrap();
            let Command::Run(options) = command else {
                panic!("expected a run");
            };
            assert_eq!(options.dram.size(), expected, "{value}");
        }
    }

    #[test]
    fn invalid_dram_options_are_rejected() {
        for arguments in [
            vec!["--binary", "loop.bin", "--dram-base", "nope"],
            vec!["--binary", "loop.bin", "--dram-base", "0x80000001"],
            vec!["--binary", "loop.bin", "--dram-size", "0"],
            vec!["--binary", "loop.bin", "--dram-size", "3KiB"],
            vec!["--binary", "loop.bin", "--dram-size", "2TiB"],
        ] {
            assert!(parse_args(&arguments).is_err(), "{arguments:?}");
        }
    }

    #[test]
    fn help_is_a_command() {
        assert_eq!(parse_args(&["--help"]), Ok(Command::Help));
    }

    #[test]
    fn binary_is_required() {
        assert!(parse_args(&[]).is_err());
        assert!(parse_args(&["--trace"]).is_err());
    }

    #[test]
    fn option_values_are_required() {
        assert!(parse_args(&["--binary"]).is_err());
        assert!(parse_args(&["--binary", "loop.bin", "--max-steps"]).is_err());
        assert!(parse_args(&["--binary", "loop.bin", "--dram-base"]).is_err());
        assert!(parse_args(&["--binary", "loop.bin", "--dram-size"]).is_err());
    }

    #[test]
    fn unknown_arguments_and_bad_numbers_are_rejected() {
        assert!(parse_args(&["--binary", "loop.bin", "--verbose"]).is_err());
        assert!(parse_args(&["--binary", "loop.bin", "--max-steps", "many"]).is_err());
    }
}
