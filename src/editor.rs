// editor — the buffer, cursor, rendering, and editing operations.

use std::fs;

use crate::input::Key;
use crate::tty::{fd_write, get_screen_size};

pub struct Editor {
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
    pub fn new() -> Editor {
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

    pub fn open(&mut self, path: &str) {
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

    pub fn save(&mut self) {
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
    pub fn refresh(&mut self) {
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

    pub fn process_key(&mut self, key: Key) -> bool {
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