// rsedit — a tiny text editor for the terminal, written in pure Rust.
//
// No external crates: everything is std + a handful of direct libc FFI
// calls (termios for raw mode, ioctl for the window size, read/write for
// byte-level I/O). Linux only.

use std::ffi::c_int;
use std::ffi::c_ulong;
use std::fs;
use std::process;

// ---------------------------------------------------------------------------
//  minimal libc bindings
// ---------------------------------------------------------------------------

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
const STDIN: c_int = 0;
const STDOUT: c_int = 1;

#[repr(C)]
#[derive(Clone, Copy)]
struct PollFd {
    fd: c_int,
    events: i16,
    revents: i16,
}

const POLLIN: i16 = 0x001;

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
    fn poll(fds: *mut PollFd, nfds: u64, timeout: c_int) -> c_int;
    fn read(fd: c_int, buf: *mut u8, count: usize) -> isize;
    fn write(fd: c_int, buf: *const u8, count: usize) -> isize;
}

fn fd_write(mut bytes: &[u8]) {
    while !bytes.is_empty() {
        let n = unsafe { write(STDOUT, bytes.as_ptr(), bytes.len()) };
        if n <= 0 {
            return; // give up on a wedged terminal
        }
        bytes = &bytes[n as usize..];
    }
}

// ---------------------------------------------------------------------------
//  raw terminal mode
// ---------------------------------------------------------------------------

static mut ORIG_TERMIOS: Option<Termios> = None;
static mut IS_TTY: bool = false;

fn enable_raw_mode() {
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

fn disable_raw_mode() {
    if let Some(orig) = unsafe { ORIG_TERMIOS } {
        unsafe {
            tcsetattr(STDIN, TCSANOW, &orig);
        }
    }
}

fn get_screen_size() -> (usize, usize) {
    let ws: WinSize = unsafe { std::mem::zeroed() };
    let rc = unsafe { ioctl(STDIN, TIOCGWINSZ, &ws as *const WinSize) };
    if rc == -1 || ws.ws_col == 0 {
        return (24, 80);
    }
    (ws.ws_row as usize, ws.ws_col as usize)
}

// ---------------------------------------------------------------------------
//  key input
// ---------------------------------------------------------------------------

#[derive(PartialEq, Clone, Copy)]
enum Key {
    Char(char),
    Enter,
    Tab,
    Backspace,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowRight,
    ArrowLeft,
    Home,
    End,
    PageUp,
    PageDown,
    WordLeft,
    WordRight,
    CtrlA,
    CtrlB,
    CtrlE,
    CtrlF,
    CtrlG,
    CtrlK,
    CtrlQ,
    CtrlS,
    CtrlU,
    CtrlW,
    Esc,
}

fn read_byte() -> Option<u8> {
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

fn read_key() -> Option<Key> {
    let c = read_byte()?;
    match c {
        27 => {
            // a real escape sequence sends its bytes back-to-back; poll briefly
            // so a lone Esc doesn't block waiting for bytes that never come
            let mut pfd = PollFd { fd: STDIN, events: POLLIN, revents: 0 };
            if unsafe { poll(&mut pfd, 1, 25) } <= 0 {
                return Some(Key::Esc);
            }
            if read_byte() != Some(b'[') {
                return Some(Key::Esc);
            }
            // CSI sequences carry parameter bytes (digits, ';') before a final
            // byte >= 0x40 — parse them so ctrl/alt+arrow are distinguishable
            let mut params: Vec<u8> = Vec::new();
            let mut b = read_byte();
            while let Some(p) = b {
                if p < 0x40 {
                    params.push(p);
                    b = read_byte();
                } else {
                    break;
                }
            }
            match b {
                Some(b'A') => Some(Key::ArrowUp),
                Some(b'B') => Some(Key::ArrowDown),
                Some(b'C') => {
                    if params.contains(&b'3') || params.contains(&b'5') {
                        Some(Key::WordRight)
                    } else {
                        Some(Key::ArrowRight)
                    }
                }
                Some(b'D') => {
                    if params.contains(&b'3') || params.contains(&b'5') {
                        Some(Key::WordLeft)
                    } else {
                        Some(Key::ArrowLeft)
                    }
                }
                Some(b'H') => Some(Key::Home),
                Some(b'F') => Some(Key::End),
                Some(b'~') => match params.as_slice() {
                    [b'3'] => Some(Key::Delete),
                    [b'5'] => Some(Key::PageUp),
                    [b'6'] => Some(Key::PageDown),
                    [b'1'] | [b'7'] => Some(Key::Home),
                    [b'4'] | [b'8'] => Some(Key::End),
                    _ => Some(Key::Esc),
                },
                _ => Some(Key::Esc),
            }
        }
        8 | 127 => Some(Key::Backspace),
        13 => Some(Key::Enter),
        9 => Some(Key::Tab),
        v if v < 32 => match v {
            1 => Some(Key::CtrlA),
            2 => Some(Key::CtrlB),
            5 => Some(Key::CtrlE),
            6 => Some(Key::CtrlF),
            7 => Some(Key::CtrlG),
            11 => Some(Key::CtrlK),
            17 => Some(Key::CtrlQ),
            19 => Some(Key::CtrlS),
            21 => Some(Key::CtrlU),
            23 => Some(Key::CtrlW),
            _ => Some(Key::Esc),    // Ctrl-C and friends are no-ops
        },
        v => Some(Key::Char(v as char)),
    }
}

// ---------------------------------------------------------------------------
//  editor state + operations
// ---------------------------------------------------------------------------

struct Editor {
    rows: Vec<Vec<char>>,          // one Vec per line of the buffer
    filename: Option<String>,
    dirty: bool,

    cx: usize,                     // cursor column (chars into the row)
    cy: usize,                     // cursor row (into rows)
    row_off: usize,                // first visible row
    col_off: usize,                // first visible display column

    screen_rows: usize,
    screen_cols: usize,

    status_msg: String,
    quit_times: u8,
}

impl Editor {
    fn new() -> Editor {
        let (h, w) = get_screen_size();
        Editor {
            rows: vec![Vec::new()],
            filename: None,
            dirty: false,
            cx: 0,
            cy: 0,
            row_off: 0,
            col_off: 0,
            screen_rows: h,
            screen_cols: w,
            status_msg: String::from("Ctrl-S save  |  Ctrl-Q quit  |  Ctrl-G info"),
            quit_times: 0,
        }
    }

    fn open(&mut self, path: &str) {
        self.filename = Some(path.to_string());
        self.rows.clear();
        match fs::read_to_string(path) {
            Ok(content) => {
                self.rows.clear();
                let mut lines: Vec<&str> = content.split('\n').collect();
                if lines.last() == Some(&"") {
                    lines.pop(); // drop the empty line after a trailing \n
                }
                for line in lines {
                    let line = line.strip_suffix('\r').unwrap_or(line);
                    self.rows.push(line.chars().collect());
                }
                if self.rows.is_empty() {
                    self.rows.push(Vec::new());
                }
                self.set_status(format!("Opened \"{}\" ({} lines)", path, self.rows.len()));
            }
            Err(_) => {
                self.rows.push(Vec::new());
                self.set_status(format!("New file: \"{}\"", path));
            }
        }
    }

    fn save(&mut self) {
        let mut content = String::new();
        let last = self.rows.len() - 1;
        for (i, row) in self.rows.iter().enumerate() {
            content.extend(row.iter());
            if i != last {
                content.push('\n');
            }
        }
        let path = match &self.filename {
            Some(p) => p.clone(),
            None => {
                let p = String::from("untitled.txt");
                self.filename = Some(p.clone());
                p
            }
        };
        match fs::write(&path, &content) {
            Ok(()) => {
                self.dirty = false;
                self.set_status(format!("Saved \"{}\" ({} bytes)", path, content.len()));
            }
            Err(e) => self.set_status(format!("Save failed: {}", e)),
        }
    }

    fn set_status(&mut self, msg: String) {
        self.status_msg = msg;
    }

    fn show_info(&mut self) {
        let name = self.filename.as_deref().unwrap_or("[No Name]");
        let dirty = if self.dirty { " (modified)" } else { "" };
        self.set_status(format!(
            "\"{}\" — {} lines, column {}{}",
            name,
            self.rows.len(),
            self.cx + 1,
            dirty
        ));
    }

    fn insert_char(&mut self, c: char) {
        self.rows.get_mut(self.cy).unwrap().insert(self.cx, c);
        self.cx += 1;
        self.dirty = true;
    }

    fn insert_newline(&mut self) {
        let right: Vec<char> = self.rows[self.cy].split_off(self.cx);
        self.rows.insert(self.cy + 1, right);
        self.cx = 0;
        self.cy += 1;
        self.dirty = true;
    }

    fn backspace(&mut self) {
        if self.cx > 0 {
            self.rows[self.cy].remove(self.cx - 1);
            self.cx -= 1;
            self.dirty = true;
        } else if self.cy > 0 {
            let prev_len = self.rows[self.cy - 1].len();
            let tail = self.rows.remove(self.cy);
            self.rows[self.cy - 1].extend(tail);
            self.cy -= 1;
            self.cx = prev_len;
            self.dirty = true;
        }
    }

    fn delete(&mut self) {
        let row_len = self.rows[self.cy].len();
        if self.cx < row_len {
            self.rows[self.cy].remove(self.cx);
            self.dirty = true;
        } else if self.cy + 1 < self.rows.len() {
            let next = self.rows.remove(self.cy + 1);
            self.rows[self.cy].extend(next);
            self.dirty = true;
        }
    }

    fn move_up(&mut self) {
        if self.cy > 0 {
            self.cy -= 1;
        }
        self.clamp_cx();
    }

    fn move_down(&mut self) {
        if self.cy + 1 < self.rows.len() {
            self.cy += 1;
        }
        self.clamp_cx();
    }

    fn move_left(&mut self) {
        if self.cx > 0 {
            self.cx -= 1;
        } else if self.cy > 0 {
            self.cy -= 1;
            self.cx = self.rows[self.cy].len();
        }
    }

    fn move_right(&mut self) {
        let row_len = self.rows[self.cy].len();
        if self.cx < row_len {
            self.cx += 1;
        } else if self.cy + 1 < self.rows.len() {
            self.cy += 1;
            self.cx = 0;
        }
    }

    fn home(&mut self) {
        self.cx = 0;
    }

    fn end(&mut self) {
        self.cx = self.rows[self.cy].len();
    }

    fn word_next(&mut self) {
        let row_len = self.rows[self.cy].len();
        if self.cx >= row_len {
            if self.cy + 1 < self.rows.len() {
                self.cy += 1;
                self.cx = 0;
            }
            return;
        }
        let chars = self.rows[self.cy].clone();
        let mut i = self.cx;
        while i < row_len && chars[i].is_whitespace() {
            i += 1;
        }
        while i < row_len && !chars[i].is_whitespace() {
            i += 1;
        }
        while i < row_len && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= row_len && self.cy + 1 < self.rows.len() {
            self.cy += 1;
            self.cx = 0;
        } else {
            self.cx = i; // lands on the next word start, like vim 'w'
        }
    }

    fn word_prev(&mut self) {
        if self.cx == 0 {
            if self.cy > 0 {
                self.cy -= 1;
                self.cx = self.rows[self.cy].len();
            }
            return;
        }
        let chars = self.rows[self.cy].clone();
        let mut i = self.cx;
        while i > 0 && chars[i - 1].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !chars[i - 1].is_whitespace() {
            i -= 1;
        }
        self.cx = i; // start of the previous word, like vim 'b'
    }

    fn delete_to_line_start(&mut self) {
        if self.cx > 0 {
            self.rows[self.cy].drain(..self.cx);
            self.cx = 0;
            self.dirty = true;
        }
    }

    fn delete_to_line_end(&mut self) {
        let row = &mut self.rows[self.cy];
        if self.cx < row.len() {
            row.truncate(self.cx);
            self.dirty = true;
        }
    }

    fn delete_word_back(&mut self) {
        let line = &self.rows[self.cy];
        let mut i = self.cx;
        while i > 0 && line[i - 1].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !line[i - 1].is_whitespace() {
            i -= 1;
        }
        if i < self.cx {
            self.rows[self.cy].drain(i..self.cx);
            self.cx = i;
            self.dirty = true;
        }
    }

    fn page_up(&mut self) {
        self.cy = self.cy.saturating_sub(self.screen_rows);
        self.clamp_cx();
    }

    fn page_down(&mut self) {
        self.cy = (self.cy + self.screen_rows).min(self.rows.len() - 1);
        self.clamp_cx();
    }

    fn clamp_cx(&mut self) {
        let len = self.rows[self.cy].len();
        if self.cx > len {
            self.cx = len;
        }
    }

    // Turn a char index into a screen column, expanding tabs to 4 spaces.
    fn cursor_display_col(&self) -> usize {
        let row = &self.rows[self.cy];
        let mut col = 0;
        for &c in row.iter().take(self.cx) {
            col += if c == '\t' { 4 } else { 1 };
        }
        col
    }

    fn scroll(&mut self) {
        if self.cy < self.row_off {
            self.row_off = self.cy;
        }
        if self.cy >= self.row_off + self.screen_rows - 1 {
            self.row_off = self.cy - self.screen_rows + 2;
        }
        let dcol = self.cursor_display_col();
        if dcol < self.col_off {
            self.col_off = dcol;
        }
        if dcol >= self.col_off + self.screen_cols {
            self.col_off = dcol - self.screen_cols + 1;
        }
    }

    // Build one full frame and push it to the terminal in a single write.
    fn refresh(&mut self) {
        let (h, w) = get_screen_size();
        self.screen_rows = h.max(1);
        self.screen_cols = w.max(1);

        if self.screen_rows < 3 {
            fd_write(b"\x1b[H\x1b[2JWindow too small\r\n");
            return;
        }

        self.scroll();

        let mut buf: Vec<u8> = Vec::with_capacity(self.screen_rows * self.screen_cols * 2);
        buf.extend_from_slice(b"\x1b[?25l\x1b[H"); // hide cursor, go home

        // text area
        let body_h = self.screen_rows - 2;
        let nrows = self.rows.len();
        for i in 0..body_h {
            let idx = self.row_off + i;
            if idx < nrows {
                let display = row_display(&self.rows[idx]);
                let shown: String = display.chars().skip(self.col_off).take(self.screen_cols).collect();
                buf.extend_from_slice(shown.as_bytes());
            } else {
                buf.push(b'~');
            }
            buf.extend_from_slice(b"\x1b[K\r\n");
        }

        // status bar (inverted)
        let name = self.filename.as_deref().unwrap_or("[No Name]");
        let dirty = if self.dirty { " (modified)" } else { "" };
        let status = format!(" {} - {} lines{}", name, nrows, dirty);
        let mut status_line: String = status.chars().take(self.screen_cols).collect();
        while status_line.chars().count() < self.screen_cols {
            status_line.push(' ');
        }
        buf.extend_from_slice(b"\x1b[7m");
        buf.extend_from_slice(status_line.as_bytes());
        buf.extend_from_slice(b"\x1b[m\r\n");

        // message bar
        let msg: String = self.status_msg.chars().take(self.screen_cols).collect();
        buf.extend_from_slice(msg.as_bytes());
        buf.extend_from_slice(b"\x1b[K\r\n");

        // place the cursor
        let cx = self.cursor_display_col().saturating_sub(self.col_off) + 1;
        let cy = self.cy.saturating_sub(self.row_off) + 1;
        buf.extend_from_slice(format!("\x1b[{};{}H", cy, cx).as_bytes());
        buf.extend_from_slice(b"\x1b[?25h"); // show cursor

        fd_write(&buf);
    }

    fn process_key(&mut self, key: Key) -> bool {
        match key {
            Key::Char(c) => self.insert_char(c),
            Key::Enter => self.insert_newline(),
            Key::Tab => {
                for _ in 0..4 {
                    self.insert_char(' ');
                }
            }
            Key::Backspace => self.backspace(),
            Key::Delete => self.delete(),
            Key::ArrowUp => self.move_up(),
            Key::ArrowDown => self.move_down(),
            Key::ArrowLeft => self.move_left(),
            Key::ArrowRight => self.move_right(),
            Key::Home => self.home(),
            Key::End => self.end(),
            Key::PageUp => self.page_up(),
            Key::PageDown => self.page_down(),
            Key::WordLeft => self.word_prev(),
            Key::WordRight => self.word_next(),
            Key::CtrlA => self.home(),
            Key::CtrlE => self.end(),
            Key::CtrlB => self.page_up(),
            Key::CtrlF => self.page_down(),
            Key::CtrlK => self.delete_to_line_end(),
            Key::CtrlU => self.delete_to_line_start(),
            Key::CtrlW => self.delete_word_back(),
            Key::CtrlG => self.show_info(),
            Key::CtrlS => self.save(),
            Key::CtrlQ => {
                if self.dirty && self.quit_times < 1 {
                    self.quit_times = 1;
                    self.set_status(String::from(
                        "Unsaved changes! Press Ctrl-Q again to quit anyway",
                    ));
                } else {
                    return true;
                }
            }
            Key::Esc => {}
        }
        self.quit_times = 0;
        false
    }
}

fn row_display(row: &[char]) -> String {
    let mut out = String::with_capacity(row.len());
    for &c in row {
        if c == '\t' {
            out.push_str("    ");
        } else {
            out.push(c);
        }
    }
    out
}

// ---------------------------------------------------------------------------
//  entry point
// ---------------------------------------------------------------------------

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 2 {
        eprintln!("usage: rsedit [file]");
        process::exit(1);
    }

    let mut ed = Editor::new();
    if args.len() == 2 {
        ed.open(&args[1]);
    }

    enable_raw_mode();
    loop {
        ed.refresh();
        match read_key() {
            None => break, // stdin closed (e.g. piped input drained)
            Some(key) => {
                if ed.process_key(key) {
                    break; // user quit
                }
            }
        }
    }
    disable_raw_mode();

    // farewell: clear the screen and let the terminal have the cursor back
    fd_write(b"\x1b[2J\x1b[H");
}