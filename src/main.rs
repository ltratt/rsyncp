//! Terminology:
//!   * "Previous run": for a given cookie, the previous execution of `rsyncp`.
//!   * "Last X": the last X during this execution of `rsyncp`.

mod config;
mod eta;
mod plat;
mod term_width;

use crate::{
    config::Config,
    eta::Eta,
    plat::{POLLHUP, POLLIN, PollFd, SIGINT, poll, set_fd_nonblocking, signal},
};
use std::{
    env,
    error::Error,
    ffi::c_int,
    fs,
    io::{self, Read, Write},
    os::fd::AsRawFd,
    path::PathBuf,
    process::{self, Command, exit},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use term_width::term_width;
use unicode_width::UnicodeWidthStr;

const RSYNCP_DIR_NAME: &str = "rsyncp";

/// The minimum / maximum ETA window time, in seconds. We adjust the window somewhat based on the
/// previous run's elapsed time, but clamped to these two values.
const MIN_ETA_WINDOW: u64 = 8;
const MAX_ETA_WINDOW: u64 = 30;

/// The size of the buffer to capture rsync output, in bytes. Allocating a moderately big buffer is
/// about as cheap as allocating a small buffer
const PIPE_BUF_SIZE: usize = 1024 * 16;
const LINE_BUF_SIZE: usize = 1024;

/// We avoid updating the display more often than this value.
const MIN_DISPLAY_UPDATE: Duration = Duration::from_millis(250);
/// The minimum time to display a path for. If another path has been seen, it will then overwrite
/// the existing one.
const MIN_PATH_DISPLAY_FOR: Duration = Duration::from_millis(250);
/// The maximum time to display a path for. If no other path has been seen, we will display a path
/// for at least this long.
const MAX_PATH_DISPLAY_FOR: Duration = Duration::from_millis(750);

const PATH_TOO_LONG: &str = "...";
const OUTPUT_SEP: &str = " ";
const TERM_GREEN: &str = "\x1b[32m";
const TERM_RED: &str = "\x1b[31m";
const TERM_RESET: &str = "\x1b[0m";

static SIGINT_RECEIVED: AtomicBool = AtomicBool::new(false);

struct Runner {
    cfg: Config,
    termw: usize,
    /// How many paths were encountered during the previous run?
    prev_paths: Option<u64>,
}

impl Runner {
    fn new(cfg: Config) -> Self {
        Self {
            cfg,
            termw: usize::from(term_width()),
            prev_paths: None,
        }
    }

    fn run(mut self) -> Result<(), Box<dyn Error>> {
        let cookie_path = self.cookie_path();

        let mut eta = None;
        if let Some(cookie_path) = &cookie_path
            && let Ok(s) = fs::read(cookie_path)
            && let Ok(s) = str::from_utf8(&s)
            && let Some((prev_paths, prev_elapsed)) = self.parse_cookie(s)
        {
            self.prev_paths = Some(prev_paths);
            // The longer run is -- based on the previous time at least! -- the larger a smoothing
            // window tends to make sense. The `/100` is an arbitrary figure that will probably
            // need refining.
            let window =
                Duration::from_secs(MAX_ETA_WINDOW.min(prev_elapsed / 100).max(MIN_ETA_WINDOW));
            eta = Some(Eta::from_prev_run(
                window,
                prev_paths,
                Duration::from_secs(prev_elapsed),
            ));
        }

        let startt = Instant::now();
        let mut child = Command::new("rsync")
            .args([
                "--info=name,progress2",
                "--outbuf=line",
                "--out-format=%o %n %l",
            ])
            .args(&self.cfg.rsync_args)
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::inherit())
            .spawn()?;
        let mut stdout = child.stdout.take().unwrap();
        let stdout_fd = stdout.as_raw_fd();
        set_fd_nonblocking(stdout_fd)?;

        let mut buf = vec![0; PIPE_BUF_SIZE];
        // `last_path_seen` will be updated quite often, so to avoid endless allocations we create
        // a `String`-as-buffer, which we clear as necessary.
        let mut last_path_seen = (String::with_capacity(LINE_BUF_SIZE), false); // (path, is_delete)
        let mut last_disp_update = None;
        let mut last_path_disp = None;
        let mut paths_known = None; // The total number of paths seen in this execution.
        let mut paths_remainder = None;
        let mut sent = 0;
        let mut deleted = 0;
        'a: loop {
            if SIGINT_RECEIVED.load(Ordering::Relaxed) {
                child.kill().ok();
                child.wait().ok();
                return Err("SIGINT".into());
            }
            let timeout = c_int::try_from(
                MIN_DISPLAY_UPDATE
                    .saturating_sub(
                        last_disp_update
                            .map(|x| {
                                Instant::now()
                                    .checked_duration_since(x)
                                    .unwrap_or_else(|| Duration::from_secs(0))
                            })
                            .unwrap_or_else(|| Duration::from_secs(0)),
                    )
                    .as_millis(),
            )
            .unwrap_or(0);
            let mut pollfd = PollFd {
                fd: stdout_fd,
                events: POLLIN | POLLHUP,
                revents: 0,
            };
            let r = unsafe { poll(&mut pollfd, 1, timeout) };
            if r == -1 {
                // Error
                let err = io::Error::last_os_error();
                if let io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock = err.kind() {
                    continue;
                }
                child.kill().ok();
                child.wait().ok();
                return Err(format!("poll failed: {}", err).into());
            } else if r == 1 && pollfd.revents & POLLIN != 0 {
                match stdout.read(&mut buf) {
                    Ok(0) => {
                        // We adapt the suggestion from
                        // https://www.greenend.org.uk/rjk/tech/poll.html for "is this
                        // descriptor finished?"
                        if pollfd.revents & POLLHUP != 0 {
                            break 'a;
                        }
                    }
                    Ok(i) => {
                        let (seen_name, seen_progress) =
                            self.parse_rsync(&buf[..i], &mut sent, &mut deleted);

                        if let Some(line) = seen_name
                            && let Some(l) = line.find(' ')
                        {
                            last_path_seen.0.clear();
                            if line.starts_with("send")
                                && let Some(r) = line.rfind(' ')
                            {
                                last_path_seen.0.push_str(&line[l + 1..r]);
                                last_path_seen.1 = false;
                            } else if line.starts_with("del.") {
                                last_path_seen.0.push_str(&line[l + 1..]);
                                last_path_seen.1 = true;
                            }
                        }

                        if let Some(line) = seen_progress
                            && let Some((rem, tot)) = line
                                .find("ir-chk=")
                                .or_else(|| line.find("to-chk="))
                                .and_then(|x| slashed_digits(&line[x + 7..]))
                        {
                            paths_remainder = Some(rem);
                            paths_known = Some(tot);
                        }
                    }
                    Err(e)
                        if e.kind() == io::ErrorKind::WouldBlock
                            || e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) => return Err(e.into()),
                }
            }

            if self.cfg.show_progress
                && (last_disp_update.is_none()
                    || last_disp_update
                        .and_then(|x| x.checked_add(MIN_DISPLAY_UPDATE))
                        .unwrap_or_else(Instant::now)
                        <= Instant::now())
            {
                let elapsed = Instant::now().saturating_duration_since(startt);
                // The string being constructed for the RHS of terminal output.
                let mut rhs = None;
                if let Some(prev_paths) = self.prev_paths {
                    let done = paths_known
                        .unwrap_or(0)
                        .saturating_sub(paths_remainder.unwrap_or(0));
                    let paths = prev_paths.max(paths_known.unwrap_or(0));
                    if paths > 0
                        && let Some(ref mut eta) = eta
                        && let Some(x) = eta.update(done, paths, elapsed)
                    {
                        rhs = Some(eta::eta_string(x));
                    }
                }
                let rhs = rhs.unwrap_or_else(|| "??:??".to_owned());
                let sent = num_suffix_fmt(sent);
                let deleted = num_suffix_fmt(deleted);
                // Calculate `rhsw` before we add control codes!
                let rhsw = rhs.width()
                    + sent.width().max(4)
                    + 2 * OUTPUT_SEP.width()
                    + deleted.width().max(4)
                    + 2 * OUTPUT_SEP.width();
                let rhs = format!(
                    "{TERM_GREEN}{sent:>4}{TERM_RESET}{OUTPUT_SEP}{OUTPUT_SEP}{TERM_RED}{deleted:>4}{TERM_RESET}{OUTPUT_SEP}{OUTPUT_SEP}{rhs}"
                );
                if let Some((t, ref _p, _is_del)) = last_path_disp {
                    let elapsed = Instant::now().saturating_duration_since(t);
                    if elapsed > MIN_PATH_DISPLAY_FOR
                        && (elapsed > MAX_PATH_DISPLAY_FOR || !last_path_seen.0.is_empty())
                    {
                        last_path_disp = None;
                    }
                }

                if last_path_disp.is_none() && !last_path_seen.0.is_empty() {
                    last_path_disp = Some((
                        Instant::now(),
                        String::from(&last_path_seen.0),
                        last_path_seen.1,
                    ));
                    last_path_seen.0.clear();
                }

                if let Some((_, last_path, is_del)) = &last_path_disp {
                    let last_pathw = last_path.width();
                    let sepw = OUTPUT_SEP.width();
                    let (lhs, path) = if last_pathw + sepw + rhsw < self.termw {
                        ("", last_path.as_str())
                    } else {
                        let path_too_longw = PATH_TOO_LONG.width();
                        if path_too_longw + 1 + sepw + rhsw < self.termw {
                            (
                                PATH_TOO_LONG,
                                &last_path[last_path
                                    .char_indices()
                                    .rev()
                                    .nth(self.termw - rhsw - sepw - 1 - path_too_longw)
                                    .unwrap()
                                    .0..],
                            )
                        } else {
                            ("", rhs.as_str())
                        }
                    };
                    let clr = if *is_del { TERM_RED } else { TERM_GREEN };
                    // If `path` contains control codes, don't try and be clever: simply don't
                    // display it.
                    let path = if path.chars().all(|x| !x.is_control()) {
                        path
                    } else {
                        ""
                    };
                    let rhspad = self.termw - lhs.width() - path.width() - 1 - rhsw;
                    io::stdout()
                        .write_all(
                            format!(
                                "\r{clr}{lhs}{path}{TERM_RESET} {:rhspad$}{rhs}\x1b[K",
                                "",
                                rhspad = rhspad
                            )
                            .as_bytes(),
                        )
                        .ok();
                } else {
                    let rhspad = self.termw - rhsw;
                    io::stdout()
                        .write_all(
                            format!("\r{:rhspad$}{rhs}\x1b[K", "", rhspad = rhspad).as_bytes(),
                        )
                        .ok();
                }
                io::stdout().flush().ok();
                last_disp_update = Some(Instant::now());
            }
        }

        if let Some(cur_paths) = paths_known {
            let elapsed = Instant::now().saturating_duration_since(startt);
            let elapsed = elapsed.as_secs() + u64::from(elapsed.subsec_millis() != 0);
            if let Some(cookie_path) = cookie_path {
                fs::write(cookie_path, format!("RSYNCP01\n{cur_paths}\n{elapsed}")).ok();
            }
        }

        match child.wait() {
            Ok(x) => {
                if !x.success() {
                    return Err(format!(
                        "rsync exited with code {}",
                        x.code()
                            .map(|x| x.to_string())
                            .unwrap_or_else(|| "<unknown>".to_owned())
                    )
                    .into());
                }
            }
            Err(e) => return Err(e.into()),
        }

        Ok(())
    }

    fn cookie_path(&self) -> Option<PathBuf> {
        match env::var("XDG_STATE_HOME").map(PathBuf::from).or_else(|_| {
            env::var("HOME").map(|x| {
                let mut x = PathBuf::from(x);
                x.push(".local");
                x.push("state");
                x.push(RSYNCP_DIR_NAME);
                x
            })
        }) {
            Ok(mut x) => {
                if fs::create_dir_all(&x).is_ok() {
                    x.push(&self.cfg.cookie);
                    Some(x)
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    }

    fn parse_cookie(&self, s: &str) -> Option<(u64, u64)> {
        let mut lines = s.lines();
        if let Some("RSYNCP01") = lines.next()
            && let Some(Ok(prev_paths)) = lines.next().map(|x| x.parse::<u64>())
            && let Some(Ok(prev_elapsed)) = lines.next().map(|x| x.parse::<u64>())
            && lines.next().is_none()
        {
            Some((prev_paths, prev_elapsed))
        } else {
            None
        }
    }

    fn parse_rsync<'a>(
        &self,
        buf: &'a [u8],
        sent: &mut u64,
        deleted: &mut u64,
    ) -> (Option<&'a str>, Option<&'a str>) {
        let mut last_name = None;
        let mut last_progress = None;
        let mut i = buf.len();
        let mut last_end = i;
        loop {
            i -= 1;
            if i == 0 || buf[i] == b'\n' || buf[i] == b'\r' {
                let start = if i == 0 { 0 } else { i + 1 };
                if start < last_end
                    && let Ok(line) = str::from_utf8(&buf[start..last_end])
                {
                    if line.starts_with("send") || line.starts_with("del.") {
                        if line.starts_with("send") {
                            *sent += 1
                        } else {
                            *deleted += 1
                        };
                        if last_name.is_none() {
                            last_name = Some(line);
                        }
                    } else if last_progress.is_none() && line.contains("(xfr#") {
                        last_progress = Some(line);
                    }
                }
                // Chomp leading whitespace.
                while i > 0 && (buf[i] == b'\n' || buf[i] == b'\r' || buf[i] == b' ') {
                    i -= 1;
                }
                last_end = i;
            }
            if i == 0 {
                break;
            }
        }

        (last_name, last_progress)
    }
}

fn slashed_digits(s: &str) -> Option<(u64, u64)> {
    let x = s.find('/')?;
    if let Ok(before) = s[..x].parse::<u64>()
        && let Ok(after) = s[x + 1
            ..x + 1
                + s[x + 1..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .count()]
            .parse::<u64>()
    {
        return Some((before, after));
    }
    None
}

fn num_suffix_fmt(n: u64) -> String {
    let (unit, suffix): (u64, &str) = match n {
        0..=999 => return n.to_string(),
        1_000..=999_999 => (1_000, "K"),
        1_000_000..=999_999_999 => (1_000_000, "M"),
        _ => (1_000_000_000, "B"),
    };

    let whole = n / unit;
    if whole >= 10 {
        format!("{whole}{suffix}")
    } else {
        let decimal = (n % unit) / (unit / 10);

        if decimal == 0 {
            format!("{whole}{suffix}")
        } else {
            format!("{whole}.{decimal}{suffix}")
        }
    }
}

extern "C" fn handle_sigint(_: c_int) {
    SIGINT_RECEIVED.store(true, Ordering::Relaxed);
}

fn main() {
    let cfg = Config::new(env::args());
    if cfg.show_progress {
        print!("\x1b[?25l");
    }
    unsafe {
        signal(SIGINT, handle_sigint);
    }
    let show_progress = cfg.show_progress;
    let runner = Runner::new(cfg);
    if let Err(e) = runner.run() {
        eprintln!("rsyncp error: {e}");
        if show_progress {
            print!("\x1b[?25h");
        }
        exit(1);
    }
    if show_progress {
        print!("\x1b[?25h");
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_slashed_digits() {
        assert_eq!(slashed_digits(""), None);
        assert_eq!(slashed_digits("/"), None);
        assert_eq!(slashed_digits("a/"), None);
        assert_eq!(slashed_digits("/b"), None);
        assert_eq!(slashed_digits("a/b"), None);
        assert_eq!(slashed_digits("0/"), None);
        assert_eq!(slashed_digits("/0"), None);
        assert_eq!(slashed_digits("0/0"), Some((0, 0)));
        assert_eq!(slashed_digits("1/2"), Some((1, 2)));
        assert_eq!(slashed_digits("1234/5678"), Some((1234, 5678)));
        assert_eq!(slashed_digits("1234a/5678"), None);
        assert_eq!(slashed_digits("1234/5678a"), Some((1234, 5678)));
    }

    #[test]
    fn test_num_suffix_fmt() {
        let cases = [
            (0, "0"),
            (999, "999"),
            (1_000, "1K"),
            (1_099, "1K"),
            (1_100, "1.1K"),
            (1_199, "1.1K"),
            (1_999, "1.9K"),
            (2_000, "2K"),
            (9_999, "9.9K"),
            (10_000, "10K"),
            (10_999, "10K"),
            (99_999, "99K"),
            (100_000, "100K"),
            (999_999, "999K"),
            (1_000_000, "1M"),
            (1_000_001, "1M"),
            (1_099_999, "1M"),
            (1_100_000, "1.1M"),
            (1_999_999, "1.9M"),
            (9_999_999, "9.9M"),
            (10_000_000, "10M"),
            (99_999_999, "99M"),
            (100_000_000, "100M"),
            (999_999_999, "999M"),
            // B range; may exceed 4 chars
            (1_000_000_000, "1B"),
            (1_000_000_001, "1B"),
            (1_100_000_000, "1.1B"),
            (1_999_999_999, "1.9B"),
            (9_999_999_999, "9.9B"),
            (10_000_000_000, "10B"),
            (999_999_999_999, "999B"),
            (1_000_000_000_000, "1000B"),
            (u64::MAX, "18446744073B"),
        ];

        for &(input, expected) in &cases {
            assert_eq!(num_suffix_fmt(input), expected, "input: {input}");
        }
    }
}
