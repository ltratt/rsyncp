use std::{
    env,
    io::{IsTerminal, stdout},
    process::exit,
};

const USAGE: &str = "usage: rsyncp [-h] [-i] [-x <str> [...-x <str>] | -c <cookie>] -- <rsync_arg_1> [... <rsync_arg_n>]";
const VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) struct Config {
    pub cookie: String,
    pub rsync_args: Vec<String>,
    pub show_progress: bool,
}

impl Config {
    /// Parse the arguments passed to rsyncp and return a [Config].
    pub(crate) fn new<I>(mut args: I) -> Self
    where
        I: IntoIterator<Item = String> + Iterator<Item = String>,
    {
        let mut cookie = None;
        let mut excluded = Vec::new();
        let mut rsync_args = Vec::new();
        let _ = args.next();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--" => {
                    rsync_args.extend(args);
                    break;
                }
                "-c" => {
                    if cookie.is_some() || !excluded.is_empty() {
                        usage(1);
                    }
                    let Some(v) = args.next() else { usage(1) };
                    cookie = Some(format!("u{v}"));
                }
                "-h" => usage(0),
                "-i" => {
                    println!("rsyncp version {VERSION}");
                    exit(0);
                }
                "-x" => {
                    if cookie.is_some() {
                        usage(1);
                    }
                    let Some(v) = args.next() else {
                        usage(1);
                    };
                    excluded.push(v);
                }
                _ => usage(1),
            }
        }
        // Check that `-- rsync_arg_1` was passed.
        if rsync_args.is_empty() {
            usage(1);
        }
        // Check that the user doesn't try and pass something to rsync that we're going to
        // interfere with.
        for x in &rsync_args {
            if x.starts_with("--human-readable")
                || (x.starts_with("-") && !x.starts_with("--") && x.contains("h"))
            {
                error("passing -h/--human-readable to rsync");
            }
            if x.starts_with("--info=") {
                error("passing --info to rsync");
            }
            if x.starts_with("--outbuf") {
                error("passing --outbuf to rsync");
            }
            if x.starts_with("--out-format") {
                error("passing --out-format to rsync");
            }
            if x.starts_with("--itemize-changes")
                || (x.starts_with("-") && !x.starts_with("--") && x.contains("i"))
            {
                error("passing -i/--itemize-changes to rsync");
            }
        }
        let cookie = match cookie {
            Some(x) => x,
            None => format!(
                "h{}",
                fnv1a(rsync_args.iter().filter(|x| !excluded.contains(x)))
            ),
        };
        Self {
            cookie,
            rsync_args,
            show_progress: stdout().is_terminal(),
        }
    }
}

// Return the FNV-1a hash of the iterator of [String]s `iter`.
fn fnv1a<'a, I>(iter: I) -> u64
where
    I: Iterator<Item = &'a String>,
{
    let mut hash: u64 = 0xCBF29CE484222325;
    for x in iter {
        for y in x.as_bytes().iter() {
            hash ^= u64::from(*y);
            hash = hash.wrapping_mul(0x100000001B3);
        }
    }
    hash
}

fn error(msg: &str) -> ! {
    #[cfg(not(test))]
    {
        eprintln!("error: {msg}");
        exit(1);
    }
    #[cfg(test)]
    {
        panic!("error: {msg}");
    }
}

fn usage(code: i32) -> ! {
    #[cfg(not(test))]
    {
        eprintln!("{USAGE}");
        exit(code);
    }
    #[cfg(test)]
    {
        let _ = code;
        panic!("{USAGE}");
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn args_basic() {
        let cfg = Config::new(["", "--", "a1", "a2"].map(|x| x.to_owned()).into_iter());
        assert_eq!(cfg.cookie, "h6150226604729619776");
        assert_eq!(cfg.rsync_args.as_slice(), ["a1", "a2"]);

        let cfg = Config::new(
            ["", "-x", "--f=g", "--", "a1", "--f=g", "a2"]
                .map(|x| x.to_owned())
                .into_iter(),
        );
        assert_eq!(cfg.cookie, "h6150226604729619776");
        assert_eq!(cfg.rsync_args.as_slice(), ["a1", "--f=g", "a2"]);

        let cfg = Config::new(
            ["", "-x", "--x=y", "--", "a1", "--f=g", "a2"]
                .map(|x| x.to_owned())
                .into_iter(),
        );
        assert_eq!(cfg.cookie, "h3437211982339263002");
        assert_eq!(cfg.rsync_args.as_slice(), ["a1", "--f=g", "a2"]);

        let cfg = Config::new(
            ["", "-c", "d", "--", "a1", "a2"]
                .map(|x| x.to_owned())
                .into_iter(),
        );
        assert_eq!(cfg.cookie, "ud");
    }

    #[test]
    #[should_panic(expected = "usage: rsyncp")]
    fn args_h() {
        Config::new(["", "-h"].map(|x| x.to_owned()).into_iter());
    }

    #[test]
    #[should_panic(expected = "usage: rsyncp")]
    fn args_unknown() {
        Config::new(["", "-unknown"].map(|x| x.to_owned()).into_iter());
    }

    #[test]
    #[should_panic(expected = "usage: rsyncp")]
    fn args_c_only_once() {
        Config::new(["", "-c", "a", "-c", "b"].map(|x| x.to_owned()).into_iter());
    }

    #[test]
    #[should_panic(expected = "usage: rsyncp")]
    fn args_c_x_mutually_exclusive1() {
        Config::new(["", "-c", "a", "-x", "b"].map(|x| x.to_owned()).into_iter());
    }

    #[test]
    #[should_panic(expected = "usage: rsyncp")]
    fn args_c_x_mutually_exclusive2() {
        Config::new(["", "-x", "a", "-c", "b"].map(|x| x.to_owned()).into_iter());
    }

    #[test]
    #[should_panic(expected = "usage: rsyncp")]
    fn args_x_missing() {
        Config::new(["", "-x"].map(|x| x.to_owned()).into_iter());
    }

    #[test]
    #[should_panic(expected = "error: passing -h/--human-readable")]
    fn args_no_human_readable1() {
        Config::new(
            ["", "--", "--human-readable"]
                .map(|x| x.to_owned())
                .into_iter(),
        );
    }

    #[test]
    #[should_panic(expected = "error: passing -h/--human-readable")]
    fn args_no_human_readable2() {
        Config::new(["", "--", "-h"].map(|x| x.to_owned()).into_iter());
    }

    #[test]
    #[should_panic(expected = "error: passing -h/--human-readable")]
    fn args_no_human_readable3() {
        Config::new(["", "--", "-ah"].map(|x| x.to_owned()).into_iter());
    }

    #[test]
    fn args_no_human_readable_allows_through_other_h() {
        Config::new(["", "--", "--ahb"].map(|x| x.to_owned()).into_iter());
    }

    #[test]
    #[should_panic(expected = "error: passing --info")]
    fn args_no_info() {
        Config::new(["", "--", "--info=x"].map(|x| x.to_owned()).into_iter());
    }

    #[test]
    #[should_panic(expected = "error: passing -i/--itemize-changes")]
    fn args_no_itemise_changes1() {
        Config::new(
            ["", "--", "--itemize-changes"]
                .map(|x| x.to_owned())
                .into_iter(),
        );
    }

    #[test]
    #[should_panic(expected = "error: passing -i/--itemize-changes")]
    fn args_no_itemise_changes2() {
        Config::new(["", "--", "-i"].map(|x| x.to_owned()).into_iter());
    }

    #[test]
    #[should_panic(expected = "error: passing -i/--itemize-changes")]
    fn args_no_itemise_changes3() {
        Config::new(["", "--", "-ai"].map(|x| x.to_owned()).into_iter());
    }

    #[test]
    fn args_no_itemise_changes_allows_through_other_i() {
        Config::new(["", "--", "--aib"].map(|x| x.to_owned()).into_iter());
    }

    #[test]
    #[should_panic(expected = "error: passing --outbuf")]
    fn args_no_outbuf() {
        Config::new(
            ["", "--", "--outbuf=none"]
                .map(|x| x.to_owned())
                .into_iter(),
        );
    }

    #[test]
    #[should_panic(expected = "error: passing --out-format")]
    fn args_no_out_format() {
        Config::new(
            ["", "--", "--out-format=x"]
                .map(|x| x.to_owned())
                .into_iter(),
        );
    }
}
