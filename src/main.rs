// rsedit — a tiny text editor for the terminal, written in pure Rust.
//
// No external crates: everything is std + a handful of direct libc FFI
// calls (termios for raw mode, ioctl for the window size, read/write for
// byte-level I/O). Linux only.
//
// The code lives in three modules:
//   tty      — raw terminal mode, screen size, byte-level I/O
//   input    — turning keypresses (incl. escape sequences) into keys
//   editor   — the buffer, cursor, rendering and editing operations

mod editor;
mod input;
mod tty;

use std::process;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 2 {
        eprintln!("usage: rsedit [file]");
        process::exit(1);
    }

    let mut ed = editor::Editor::new();
    if args.len() == 2 {
        ed.open(&args[1]);
    }

    tty::enable_raw_mode();
    loop {
        ed.refresh();
        match input::read_key() {
            None => break, // stdin closed (e.g. piped input drained)
            Some(key) => {
                if ed.process_key(key) {
                    break; // user quit
                }
            }
        }
    }
    tty::disable_raw_mode();

    // farewell: clear the screen and let the terminal have the cursor back
    tty::fd_write(b"\x1b[2J\x1b[H");
}