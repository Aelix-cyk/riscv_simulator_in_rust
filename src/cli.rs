//! Command-line options for the simulator binary.

use std::path::PathBuf;

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
    /// Flat binary to load at `DRAM_BASE`.
    pub binary: PathBuf,
    /// Print each executed instruction.
    pub trace: bool,
    /// Steps to take before giving up.
    pub max_steps: u64,
}

/// Steps a run takes before giving up, unless `--max-steps` says otherwise.
pub const DEFAULT_MAX_STEPS: u64 = 10_000;

/// Usage text, printed for `--help` and for bad arguments.
pub const USAGE: &str = "\
usage: riscv_simulator_in_rust --binary <FILE> [--trace] [--max-steps <N>]

  --binary <FILE>   flat binary to load at DRAM_BASE and run
  --trace           print each executed instruction
  --max-steps <N>   stop after N steps (default 10000)
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
            "--help" => return Ok(Command::Help),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }

    let binary = binary.ok_or("--binary <FILE> is required")?;
    Ok(Command::Run(Options {
        binary,
        trace,
        max_steps,
    }))
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
    }

    #[test]
    fn unknown_arguments_and_bad_numbers_are_rejected() {
        assert!(parse_args(&["--binary", "loop.bin", "--verbose"]).is_err());
        assert!(parse_args(&["--binary", "loop.bin", "--max-steps", "many"]).is_err());
    }
}
