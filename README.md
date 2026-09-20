# rsedit

A tiny TUI text editor for the terminal, written in **pure Rust with zero dependencies**.

It puts the terminal into raw mode (plain `termios` FFI, no crates) and redraws the
screen with ANSI escape codes — a classic "kilo"-style editor you can read in one
sitting.

## Build & run

```bash
cargo build --release        # binary lands in target/release/rsedit
./target/release/rsedit                      # open a new buffer
./target/release/rsedit README.md            # open a file
```

## Controls

| Key | Action |
|---|---|
| `arrows` / `Home` / `End` | move cursor |
| `Page Up` / `Page Down` | jump by a screen |
| `Enter` | new line |
| `Tab` | insert 4 spaces |
| `Backspace` / `Delete` | delete backwards / forward |
| `Ctrl-S` | save |
| `Ctrl-Q` | quit (press twice if the buffer is unsaved) |

Everything else just types a character. Tabs are shown as 4 spaces.

## Vim-inspired shortcuts

`rsedit` stays modeless (you're always typing), but borrows a few motion
and editing keys from vim/nvim so you can keep your hands on the keyboard:

| Key | vim equivalent | Action |
|---|---|---|
| `Ctrl-←` / `Ctrl-→` | `b` / `w` | jump word backwards / forwards |
| `Ctrl-A` / `Ctrl-E` | `0` / `$` | jump to start / end of line |
| `Ctrl-B` / `Ctrl-F` | `^B` / `^F` | page up / page down |
| `Ctrl-W` | `db` in insert mode | delete the word before the cursor |
| `Ctrl-U` | insert-mode `^U` | delete back to start of line |
| `Ctrl-K` | `d$` | delete to end of line |
| `Ctrl-G` | `^G` | show file info (name, lines, column) |

## Notes

- Linux only (uses `termios` + `ioctl(TIOCGWINSZ)` directly).
- Missing files open as a new buffer and are created on first save.
- With no filename, the first save writes to `untitled.txt`.

## License

MIT