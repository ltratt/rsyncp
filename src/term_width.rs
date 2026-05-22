use crate::plat::{TIOCGWINSZ, ioctl};
use std::{
    io::{self, IsTerminal},
    os::fd::AsRawFd,
};

const DEFAULT_TERM_WIDTH: u16 = 80;

#[repr(C)]
struct Winsize {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
}

pub(crate) fn term_width() -> u16 {
    if !io::stdout().is_terminal() {
        return DEFAULT_TERM_WIDTH;
    }

    let mut ws = Winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    let rc = unsafe { ioctl(io::stdout().as_raw_fd(), TIOCGWINSZ, &mut ws) };
    if rc == 0 && ws.ws_col != 0 {
        ws.ws_col
    } else {
        DEFAULT_TERM_WIDTH
    }
}
