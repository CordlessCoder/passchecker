use clap::Parser;
use const_format::{str_replace, str_split};
use owo_colors::{
    OwoColorize,
    Stream::{Stderr, Stdout},
    Style,
};
use std::path::PathBuf;
use std::{borrow::Cow, fs::read_to_string};
use std::{cmp::min, io::stdin};

#[derive(Debug, Clone)]
enum WordlistType {
    Internal([&'static str; str_split!(include_str!("../wordlist"), '\n').len()]),
    External(String),
}

static WORDLIST: WordlistType = WordlistType::Internal(str_split!(
    str_replace!(include_str!("../wordlist"), '\r', ""),
    '\n'
));

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// The password to check
    password: Option<String>,

    /// Sets what wordlist to check against, if not specified defaults to the internal wordlist
    #[arg(short, long, value_name = "FILE")]
    wordlist: Option<PathBuf>,

    /// Overrides the minimum length of the password
    #[arg(short, long, value_name = "MINIMUM LENGTH")]
    min_length: Option<u8>,

    /// Whether to perform the wordlist check, defaults to true
    #[arg(short, long)]
    enable_wordlist: Option<bool>,
}

const DEFAULT_MIN_LENGTH: u8 = 8;

struct Test<'a> {
    name: String,
    test: fn(&'a Cli, &str) -> (Option<bool>, Cow<'a, str>),
}

impl<'a> Test<'a> {
    fn new(name: String, test: fn(&'a Cli, &str) -> (Option<bool>, Cow<'a, str>)) -> Self {
        Self { name, test }
    }
}

// #[derive(Subcommand)]
// enum Commands {
//     /// does testing things
//     Test {
//         /// lists test values
//         #[arg(short, long)]
//         list: bool,
//     },
// }

fn password_compare(checkpass: &str, pass: &str) -> bool {
    checkpass == pass
}

fn main() {
    let success_style: Style = Style::new().black().bold().on_bright_green();
    let failure_style: Style = Style::new().black().bold().on_bright_red();
    let ignored_style: Style = Style::new().black().bold().on_white();
    let cli = Cli::parse();
    let mut buf = String::with_capacity(8);

    let password = if let Some(ref password) = cli.password {
        password
    } else {
        let stdin = stdin();
        // If no password was provided as an argument
        let Ok(_) = stdin.read_line(&mut buf) else {
            eprintln!("{}","No password provided as argument and failed to read password from STDIN. Aborting.".if_supports_color(Stderr, |x|x.style(failure_style)));
            return
        };
        match buf.pop() {
            Some('\n') => (),
            Some(ch) => buf.push(ch),
            None => unreachable!("Somehow managed to read a 0 bytes long string from STDIN"),
        }
        &buf
    };

    let tests = [
        Test::new(
            format!(
                "At least {} characters",
                cli.min_length
                    .unwrap_or(DEFAULT_MIN_LENGTH)
                    .if_supports_color(Stdout, |x| x.blue())
            ),
            |cli: &Cli, pass: &str| {
                let min_length = if let Some(override_length) = cli.min_length {
                    override_length
                } else {
                    DEFAULT_MIN_LENGTH
                };
                let len = pass.len();
                let outcome = len >= min_length.into();
                (
                    Some(outcome),
                    if outcome {
                        Cow::Borrowed("")
                    } else {
                        Cow::Owned(format!(
                            "Password too short: {}/{} characters",
                            len, min_length
                        ))
                    },
                )
            },
        ),
        Test::new("numbers".to_string(), |cli: &Cli, pass: &str| {
            // pass.
            let outcome = pass.chars().any(|c| c.is_digit(10));
            (
                Some(outcome),
                Cow::Borrowed(if outcome {
                    ""
                } else {
                    "No numeric chacacters in password"
                }),
            )
        }),
        Test::new(
            "collisions in wordlist".to_string(),
            |cli: &Cli, pass: &str| {
                // Only execute the wordlist logic if enable_wordlist is true or was not provided
                if cli.enable_wordlist == Some(false) {
                    return (None, Cow::Borrowed("Disabled with -e false"));
                }
                let mut info = String::new();
                // Read wordlist from file if provided, default to internal otherwise
                let wordlist = if let Some(wordlist_path) = cli.wordlist.as_deref() {
                    let Ok(wordlist) = read_to_string(&wordlist_path) else {
            // If the given file doesn't exist
            info = format!("Failed to read file '{}'. Aborting.", wordlist_path.display().if_supports_color(Stderr, |x|x.red()));
            return (Some(false), Cow::Owned(info))
        };
                    WordlistType::External(wordlist)
                } else {
                    info = format!(
            "{}",
            "No wordlist provided, defaulting to internal wordlist(10k most common passwords)."
                .if_supports_color(Stderr, |x| x.blue())
        );
                    WORDLIST.to_owned()
                };
                // At this point we have the wordlist set correctly and  ensured that the test
                // should not be ignored
                let outcome = match wordlist {
                    WordlistType::Internal(lines) => {
                        let outcome = lines
                            .into_iter()
                            .find(|&checkpass| password_compare(checkpass, pass));
                        if let Some(checkpass) = outcome {
                            info = format!("Found a match in wordlist: {}", checkpass)
                        }
                        outcome.map(|x| x.to_owned())
                    }
                    WordlistType::External(string) => {
                        let outcome = string
                            .lines()
                            .find(|&checkpass| password_compare(checkpass, pass))
                            .map(|x| x.to_owned());
                        if let Some(checkpass) = &outcome {
                            info = format!("Found a match in wordlist: {}", checkpass)
                        }
                        outcome
                    }
                };
                (Some(outcome.is_none()), Cow::Owned(info))
            },
        ),
    ];
    let longest_name = tests.iter().fold(0, |acc, Test { name, .. }| {
        (name.chars().count() - name.chars().filter(|x| x == &'\u{1b}').count() * 5).max(acc)
    }) + 4;
    println!(
        "Password:{}{}",
        " ".repeat(longest_name.checked_sub(8).unwrap_or(0)),
        password.bold().blue()
    );
    let mut enabled_count = 0u32;
    let successes = tests
        .iter()
        .filter(|Test { name: expl, test }| {
            let difference = longest_name
                - (expl.chars().count() - expl.chars().filter(|x| x == &'\u{1b}').count() * 5);
            print!("{expl}:{}", " ".repeat(difference));
            let (outcome, info) = test(&cli, &password);
            match outcome {
                Some(true) => {
                    println!(
                        "{}",
                        "success".if_supports_color(Stdout, |x| x.style(success_style))
                    );
                    if info != "" {
                        println!("Additional info: {}", info)
                    }
                }
                Some(false) => {
                    println!(
                        "{}",
                        "failure".if_supports_color(Stdout, |x| x.style(failure_style))
                    );
                    println!(
                        "Additional info: {}",
                        info.if_supports_color(Stdout, |x| x.style(failure_style))
                    )
                }
                None => {
                    println!(
                        "{}",
                        "ignored".if_supports_color(Stdout, |x| x.style(ignored_style))
                    );
                    println!(
                        "Additional info: {}",
                        info.if_supports_color(Stdout, |x| x.style(ignored_style))
                    )
                }
            }
            if outcome.is_some() {
                enabled_count += 1
            }
            outcome.unwrap_or(false)
        })
        .count();
    println!(
        "Passed {} out of {} tests ({}%), {} ignored",
        successes.if_supports_color(Stdout, |x| x.blue()),
        tests.len().if_supports_color(Stdout, |x| x.blue()),
        (successes as f32 / enabled_count as f32 * 100.0).if_supports_color(Stdout, |x| x.yellow()),
        (tests.len() - enabled_count as usize)
            .if_supports_color(Stdout, |x| x.style(ignored_style))
    );
}
