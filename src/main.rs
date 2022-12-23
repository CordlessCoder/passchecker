use clap::Parser;
use const_format::{str_replace, str_split};
use owo_colors::{
    OwoColorize,
    Stream::{Stderr, Stdout},
    Style,
};
use similar_string::find_best_similarity;
use std::io::{stdin, stdout, Write};
use std::path::PathBuf;
use std::{borrow::Cow, fs::read_to_string};

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
#[command(author = "CordlessCoder", version, about, long_about = None)]
struct Cli {
    /// The password to check
    password: Option<String>,

    /// Sets what wordlist to check against, if not specified defaults to the internal wordlist
    #[arg(short, long, value_name = "FILE")]
    wordlist: Option<PathBuf>,

    /// Overrides the minimum length of the password
    #[arg(short, long, value_name = "MINIMUM LENGTH")]
    min_length: Option<u8>,

    /// Which tests to ignore, optional
    #[arg(short, long, value_enum, value_name = "IGNORE")]
    ignore: Option<Vec<Ignore>>,

    /// The minimum percentage match required for a match to be considered a collision
    #[arg(short, long, value_name = "MINIMUM SIMILARITY")]
    similarity: Option<u8>,
}

#[derive(clap::ValueEnum, Clone, Debug, PartialEq, PartialOrd, Eq)]
enum Ignore {
    MinimumChars,
    Numbers,
    SpecialChars,
    WordlistCollisions,
}

const DEFAULT_MIN_LENGTH: u8 = 8;

struct Test<'a> {
    name: String,
    test: fn(&'a Cli, &str) -> (Option<bool>, Cow<'a, str>),
    ignore: Ignore,
}

impl<'a> Test<'a> {
    fn new(
        name: String,
        test: fn(&'a Cli, &str) -> (Option<bool>, Cow<'a, str>),
        ignore: Ignore,
    ) -> Self {
        Self { name, test, ignore }
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

fn main() {
    let success_style: Style = Style::new().black().bold().on_bright_green();
    let failure_style: Style = Style::new().black().bold().on_bright_red();
    let ignored_style: Style = Style::new().black().bold().on_white();
    let cli = Cli::parse();
    let mut buf = String::with_capacity(8);

    let password = if let Some(ref password) = cli.password {
        password
    } else {
        let mut lock = stdout().lock();
        write!(lock, "Please enter the password to check.\n> ").expect("Failed to write to stdout");
        stdout().flush().expect("Failed to flust stdout");
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
            Ignore::MinimumChars,
        ),
        Test::new(
            "numbers".to_string(),
            |_cli: &Cli, pass: &str| {
                // pass.
                let outcome = pass.chars().any(|c| c.is_ascii_digit());
                (
                    Some(outcome),
                    Cow::Borrowed(if outcome {
                        ""
                    } else {
                        "No numeric chacacters in password"
                    }),
                )
            },
            Ignore::Numbers,
        ),
        Test::new(
            "quirky characters".to_string(),
            |_cli: &Cli, pass: &str| {
                // pass.
                let outcome = pass.chars().any(|c| c.is_ascii_punctuation());
                (
                    Some(outcome),
                    Cow::Borrowed(if outcome {
                        ""
                    } else {
                        "No special chacacters in password"
                    }),
                )
            },
            Ignore::SpecialChars,
        ),
        Test::new(
            "collisions in wordlist".to_string(),
            |cli: &Cli, pass: &str| {
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
                        let outcome = find_best_similarity(pass, &lines);
                        if let Some((checkpass, similarity)) = &outcome {
                            info = format!(
                                "Best match in wordlist is {} with similarity {}%",
                                checkpass,
                                similarity * 100.0
                            )
                        }
                        outcome.map(|x| x.to_owned())
                    }
                    WordlistType::External(string) => {
                        let outcome =
                            find_best_similarity(pass, &string.lines().collect::<Vec<_>>());
                        if let Some((checkpass, similarity)) = &outcome {
                            info = format!(
                                "Best match in wordlist is {} with similarity {}%",
                                checkpass,
                                similarity * 100.0
                            )
                        }
                        outcome
                    }
                };
                (
                    Some(
                        outcome.is_some()
                            && outcome.unwrap().1
                                < (cli.similarity.unwrap_or(97).min(99) as f64 / 100.0),
                    ),
                    Cow::Owned(info),
                )
            },
            Ignore::WordlistCollisions,
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
        .filter(
            |Test {
                 name: expl,
                 test,
                 ignore,
             }| {
                let difference = longest_name
                    - (expl.chars().count() - expl.chars().filter(|x| x == &'\u{1b}').count() * 5);
                print!("{expl}:{}", " ".repeat(difference));
                // Only execute the logic if enable_wordlist is true or was not provided
                let (outcome, info) =
                    if cli.ignore.as_deref().map(|x| x.contains(&ignore)) == Some(true) {
                        (None, Cow::Owned(format!("disabled with -i {ignore:?}")))
                    } else {
                        test(&cli, &password)
                    };
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
            },
        )
        .count();
    println!(
        "Passed {} out of {} tests ({}%), {} ignored",
        successes.if_supports_color(Stdout, |x| x.blue()),
        enabled_count.if_supports_color(Stdout, |x| x.blue()),
        (successes as f32 / enabled_count as f32 * 100.0).if_supports_color(Stdout, |x| x.yellow()),
        (tests.len() - enabled_count as usize)
            .if_supports_color(Stdout, |x| x.style(ignored_style))
    );
}
