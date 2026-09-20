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

## Notes

- Linux only (uses `termios` + `ioctl(TIOCGWINSZ)` directly).
- Missing files open as a new buffer and are created on first save.
- With no filename, the first save writes to `untitled.txt`.

## License

MIT