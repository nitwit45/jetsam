//! Binary entry point. `jetsam` opens the app; `jetsam --headless` runs one scan-and-clean
//! with no window and prints a summary (what the weekly launchd job calls).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use jetsam_lib::types::Mode;

const USAGE: &str = "\
jetsam - macOS disk and memory cleaner

USAGE
    jetsam                       open the app
    jetsam --headless [OPTIONS]  scan and clean with no window, print a summary

OPTIONS
    --mode <MODE>   light (default) | standard | deep | full; --mode=<MODE> also works
    --dry-run       report what would be removed; removes nothing
    -h, --help      print this help

MODES
    light      app caches and logs, Trash, old installers, Homebrew and Docker leftovers
    standard   light + package-manager caches, Xcode/Android artefacts
    deep       standard + build artefacts in repos idle for 14 days, stale worktrees
               idle for 24 hours, and AI session logs listed for review
    full       deep with no idle window; still never git-tracked files, never user data

Headless runs clean in light and standard only. deep and full need --dry-run.
Any other argument, including a misspelled option, is refused with exit code 2
and nothing runs.
";

/// What the command line asks for.
#[derive(Debug, PartialEq)]
enum Command {
    Help,
    App,
    Headless { mode: Mode, dry_run: bool },
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(&args) {
        Ok(Command::Help) => print!("{USAGE}"),
        Ok(Command::App) => jetsam_lib::run(),
        Ok(Command::Headless { mode, dry_run }) => println!("{}", jetsam_lib::run_headless(mode, dry_run)),
        Err(message) => {
            eprintln!("jetsam: {message}");
            std::process::exit(2);
        }
    }
}

/// Every argument must be one this binary knows, spelled exactly. A typo is refused
/// rather than ignored, so a misspelled `--dry-run` can never become a real clean.
fn parse_args(args: &[String]) -> Result<Command, String> {
    let mut help = false;
    let mut headless = false;
    let mut dry_run = false;
    let mut mode: Option<&str> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let value = match arg.as_str() {
            "-h" | "--help" => {
                help = true;
                continue;
            }
            "--headless" => {
                headless = true;
                continue;
            }
            "--dry-run" => {
                dry_run = true;
                continue;
            }
            "--mode" => it.next().map(String::as_str).filter(|v| !v.starts_with('-')),
            // Older macOS adds a process serial number when Finder launches an app.
            a if a.starts_with("-psn_") => continue,
            a => match a.strip_prefix("--mode=") {
                Some(v) => Some(v),
                None => return Err(format!("unknown argument {a}; see jetsam --help")),
            },
        };
        let value = value.filter(|v| !v.is_empty()).ok_or("--mode needs a value; see jetsam --help")?;
        if mode.replace(value).is_some() {
            return Err("--mode given more than once".into());
        }
    }
    if help {
        return Ok(Command::Help);
    }
    if !headless {
        if mode.is_some() || dry_run {
            return Err("--mode and --dry-run only apply with --headless; see jetsam --help".into());
        }
        return Ok(Command::App);
    }
    let name = mode.unwrap_or("light");
    let mode = Mode::parse(name)
        .ok_or_else(|| format!("unknown mode {name} (expected light, standard, deep or full)"))?;
    if mode > Mode::Standard && !dry_run {
        return Err(format!(
            "{name} mode only runs headless with --dry-run; headless cleaning is light or standard"
        ));
    }
    Ok(Command::Headless { mode, dry_run })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, String> {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        parse_args(&args)
    }

    fn headless(mode: Mode, dry_run: bool) -> Result<Command, String> {
        Ok(Command::Headless { mode, dry_run })
    }

    #[test]
    fn headless_mode_messages() {
        assert_eq!(parse(&[]), Ok(Command::App));
        assert_eq!(parse(&["--help"]), Ok(Command::Help));
        assert_eq!(parse(&["--headless", "-h"]), Ok(Command::Help));
        assert_eq!(parse(&["--headless"]), headless(Mode::Light, false));
        assert_eq!(parse(&["--headless", "--mode", "standard"]), headless(Mode::Standard, false));
        assert_eq!(parse(&["--headless", "--mode", "full", "--dry-run"]), headless(Mode::Full, true));
        assert_eq!(
            parse(&["--headless", "--mode", "bogus", "--dry-run"]),
            Err("unknown mode bogus (expected light, standard, deep or full)".into())
        );
        assert_eq!(
            parse(&["--headless", "--mode", "deep"]),
            Err("deep mode only runs headless with --dry-run; headless cleaning is light or standard".into())
        );
        assert!(parse(&["--headless", "--mode"]).unwrap_err().contains("needs a value"));
        assert!(parse(&["--headless", "--mode", "--dry-run"]).unwrap_err().contains("needs a value"));
        assert!(parse(&["--headless", "--mode="]).unwrap_err().contains("needs a value"));
        assert!(parse(&["--headless", "--mode", "light", "--mode=standard"])
            .unwrap_err()
            .contains("more than once"));
    }

    #[test]
    fn unknown_argument_is_refused() {
        assert_eq!(
            parse(&["--headless", "--force"]),
            Err("unknown argument --force; see jetsam --help".into())
        );
        assert_eq!(parse(&["--bogus"]), Err("unknown argument --bogus; see jetsam --help".into()));
        assert_eq!(
            parse(&["--headless", "standard"]),
            Err("unknown argument standard; see jetsam --help".into())
        );
        assert!(parse(&["--mode", "light"]).unwrap_err().contains("only apply with --headless"));
        assert_eq!(parse(&["-psn_0_12345"]), Ok(Command::App), "Finder's process serial number");
    }

    #[test]
    fn mode_equals_form_is_parsed() {
        assert_eq!(parse(&["--headless", "--mode=deep", "--dry-run"]), headless(Mode::Deep, true));
        assert_eq!(parse(&["--headless", "--mode=standard"]), headless(Mode::Standard, false));
        assert!(parse(&["--headless", "--mode=deep"])
            .unwrap_err()
            .contains("only runs headless with --dry-run"));
    }

    #[test]
    fn misspelled_dry_run_is_refused() {
        for typo in ["--dryrun", "--dry_run", "--dry-runn", "-dry-run", "--Dry-Run", "--dry-run=true"] {
            assert_eq!(
                parse(&["--headless", "--mode", "standard", typo]),
                Err(format!("unknown argument {typo}; see jetsam --help")),
                "{typo} must never run a real clean"
            );
        }
    }
}
