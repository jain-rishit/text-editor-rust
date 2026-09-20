// tty — raw terminal mode, window size, and byte-level I/O via direct libc FFI.

use std::ffi::c_int;
use std::ffi::c_ulong;

// Linux termios flag values (asm-generic termbits.h)
const ISIG: u32 = 0o000001;
const ICANON: u32 = 0o000002;
const ECHO: u32 = 0o000010;
const OPOST: u32 = 0o000002;
const ICRNL: u32 = 0o000400;
const IXON: u32 = 0o002000;
const IEXTEN: u32 = 0o100000;
const TCSANOW: c_int = 0;
const VMIN: usize = 6;
const VTIME: usize = 5;

const TIOCGWINSZ: c_ulong = 0x5413;
pub const STDIN: c_int = 0;
const STDOUT: c_int = 1;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PollFd {
    pub fd: c_int,
    pub events: i16,
    pub revents: i16,
}

pub const POLLIN: i16 = 0x001;

#[repr(C)]
#[derive(Clone, Copy)]
struct Termios {
    c_iflag: u32,
    c_oflag: u32,
    c_cflag: u32,
    c_lflag: u32,
    c_line: u8,
    c_cc: [u8; 32],
    c_ispeed: u32,
    c_ospeed: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct WinSize {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
}

extern "C" {
    fn tcgetattr(fd: c_int, tp: *mut Termios) -> c_int;
    fn tcsetattr(fd: c_int, actions: c_int, tp: *const Termios) -> c_int;
    fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
    pub fn poll(fds: *mut PollFd, nfds: u64, timeout: c_int) -> c_int;
    fn read(fd: c_int, buf: *mut u8, count: usize) -> isize;
    fn write(fd: c_int, buf: *const u8, count: usize) -> isize;
}

static mut ORIG_TERMIOS: Option<Termios> = None;
static mut IS_TTY: bool = false;

pub fn fd_write(mut bytes: &[u8]) {
    while !bytes.is_empty() {
        let n = unsafe { write(STDOUT, bytes.as_ptr(), bytes.len()) };
        if n <= 0 {
            return; // give up on a wedged terminal
        }
        bytes = &bytes[n as usize..];
    }
}

pub fn enable_raw_mode() {
    let mut raw: Termios = unsafe { std::mem::zeroed() };
    if unsafe { tcgetattr(STDIN, &mut raw) } != 0 {
        return; // stdin is not a terminal (piped input) — carry on regardless
    }
    unsafe {
        ORIG_TERMIOS = Some(raw);
        IS_TTY = true;
    }

    raw.c_iflag &= !(ICRNL | IXON);
    raw.c_oflag &= !(OPOST);
    raw.c_lflag &= !(ECHO | ICANON | ISIG | IEXTEN);
    raw.c_cc[VMIN] = 0;
    raw.c_cc[VTIME] = 1; // read() returns after ~100 ms even with no input
    unsafe {
        tcsetattr(STDIN, TCSANOW, &raw);
    }
}

pub fn disable_raw_mode() {
    if let Some(orig) = unsafe { ORIG_TERMIOS } {
        unsafe {
            tcsetattr(STDIN, TCSANOW, &orig);
        }
    }
}

pub fn get_screen_size() -> (usize, usize) {
    let ws: WinSize = unsafe { std::mem::zeroed() };
    let rc = unsafe { ioctl(STDIN, TIOCGWINSZ, &ws as *const WinSize) };
    if rc == -1 || ws.ws_col == 0 {
        return (24, 80);
    }
    (ws.ws_row as usize, ws.ws_col as usize)
}

pub fn read_byte() -> Option<u8> {
    let mut b = [0u8; 1];
    loop {
        let n = unsafe { read(STDIN, b.as_mut_ptr(), 1) };
        if n == 1 {
            return Some(b[0]);
        }
        if n == 0 && !unsafe { IS_TTY } {
            return None; // EOF: piped input is exhausted
        }
        // on a terminal a zero return just means "idle for 100 ms" — keep waiting,
        // quitting must be an explicit Ctrl-Q, never a timeout
    }
}