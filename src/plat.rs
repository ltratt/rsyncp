use std::{
    error::Error,
    ffi::{c_int, c_short},
    io,
    os::fd::RawFd,
};

mod inner {
    include!(concat!(env!("OUT_DIR"), "/plat.rs"));
}

pub use inner::{F_GETFL, F_SETFL, O_NONBLOCK, POLLHUP, POLLIN, SIGINT, TIOCGWINSZ};

#[repr(C)]
pub struct PollFd {
    pub fd: c_int,
    pub events: c_short,
    pub revents: c_short,
}

type Sighandler = extern "C" fn(c_int);

unsafe extern "C" {
    pub fn fcntl(fd: c_int, cmd: c_int, ...) -> c_int;
    pub fn ioctl(fd: i32, request: u64, ...) -> i32;
    pub fn poll(fds: *mut PollFd, nfds: usize, timeout: c_int) -> c_int;
    pub fn signal(signum: c_int, handler: Sighandler) -> Sighandler;
}

/// Set `fd` as non-blocking.
pub fn set_fd_nonblocking(fd: RawFd) -> Result<(), Box<dyn Error>> {
    let flags = unsafe { fcntl(fd, F_GETFL) };
    if flags == -1 {
        return Err(io::Error::last_os_error().into());
    }
    if unsafe { fcntl(fd, F_SETFL, flags | O_NONBLOCK) } == -1 {
        return Err(io::Error::last_os_error().into());
    }

    Ok(())
}
