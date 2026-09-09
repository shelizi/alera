use std::io;

use anyhow::{Context, Result};
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

use crate::cli::TerminalAttachArgs;
use crate::runtime_host_client::RuntimeHostRpcClient;

const INPUT_CHUNK_BYTES: usize = 8 * 1024;

pub(crate) async fn run(client: &mut RuntimeHostRpcClient, args: TerminalAttachArgs) -> i32 {
    let result = run_session(client, &args).await;
    match result {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("Failed to attach terminal {}: {error}", args.handle);
            1
        }
    }
}

async fn run_session(client: &mut RuntimeHostRpcClient, args: &TerminalAttachArgs) -> Result<()> {
    let raw_mode = RawTerminalMode::enable().context("failed to enable terminal input mode")?;
    let result = run_session_with_raw_mode(client, args).await;
    drop(raw_mode);
    result
}

async fn run_session_with_raw_mode(
    client: &mut RuntimeHostRpcClient,
    args: &TerminalAttachArgs,
) -> Result<()> {
    let (input_tx, mut input_rx) = mpsc::channel::<Vec<u8>>(8);
    let input_task = tokio::spawn(async move {
        let mut stdin = tokio::io::stdin();
        let mut buffer = vec![0_u8; INPUT_CHUNK_BYTES];
        loop {
            match stdin.read(&mut buffer).await {
                Ok(0) => break,
                Ok(length) => {
                    if input_tx.send(buffer[..length].to_vec()).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut stdout = tokio::io::stdout();
    let mut cursor = None;
    let mut last_size = None;
    let mut ready_file_written = false;
    let poll_delay = Duration::from_millis(args.poll_ms);
    let result = async {
        loop {
            tokio::select! {
                input = input_rx.recv() => {
                    let Some(input) = input else {
                        break;
                    };
                    if input.is_empty() {
                        continue;
                    }
                    client
                        .request_value(
                            "write",
                            &json!({
                                "sessionId": args.handle,
                                "dataBase64": STANDARD.encode(input),
                                "deferredEnter": false,
                                "bracketedPaste": false,
                            }),
                        )
                        .await
                        .context("runtime host rejected terminal input")?;
                }
                _ = sleep(poll_delay) => {
                    let size = TerminalSize::detect();
                    if size != last_size {
                        if let Some(size) = size {
                            client
                                .request_value(
                                    "resize",
                                    &json!({
                                        "sessionId": args.handle,
                                        "cols": size.cols,
                                        "rows": size.rows,
                                    }),
                                )
                                .await
                                .context("runtime host rejected terminal resize")?;
                        }
                        last_size = size;
                    }

                    let value = client
                        .request_value(
                            "terminal.read",
                            &json!({
                                "sessionId": args.handle,
                                "cursor": cursor,
                                "maxBytes": args.max_bytes,
                            }),
                        )
                        .await
                        .context("runtime host rejected terminal output read")?;
                    let data = read_data(&value)?;
                    if !data.is_empty() {
                        stdout.write_all(&data).await?;
                        stdout.flush().await?;
                    }
                    cursor = value.get("nextCursor").and_then(Value::as_u64);

                    if !ready_file_written {
                        if let Some(path) = args.ready_file.as_deref() {
                            tokio::fs::write(path, b"ready\n")
                                .await
                                .with_context(|| format!("failed to write ready file {path}"))?;
                        }
                        ready_file_written = true;
                    }
                    if value.get("running").and_then(Value::as_bool) == Some(false) {
                        break;
                    }
                }
            }
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;
    input_task.abort();
    result
}

fn read_data(value: &Value) -> Result<Vec<u8>> {
    let encoded = value
        .get("dataBase64")
        .and_then(Value::as_str)
        .unwrap_or_default();
    STANDARD
        .decode(encoded)
        .context("runtime host returned invalid terminal output")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TerminalSize {
    cols: u16,
    rows: u16,
}

impl TerminalSize {
    fn detect() -> Option<Self> {
        detect_terminal_size()
    }
}

struct RawTerminalMode {
    #[cfg(unix)]
    original: Option<libc::termios>,
    #[cfg(windows)]
    state: Option<WindowsConsoleState>,
}

impl RawTerminalMode {
    fn enable() -> io::Result<Self> {
        enable_raw_terminal_mode()
    }
}

impl Drop for RawTerminalMode {
    fn drop(&mut self) {
        restore_raw_terminal_mode(self);
    }
}

#[cfg(unix)]
fn enable_raw_terminal_mode() -> io::Result<RawTerminalMode> {
    let fd = libc::STDIN_FILENO;
    if unsafe { libc::isatty(fd) } != 1 {
        return Ok(RawTerminalMode { original: None });
    }
    let mut original = std::mem::MaybeUninit::<libc::termios>::uninit();
    if unsafe { libc::tcgetattr(fd, original.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let original = unsafe { original.assume_init() };
    let mut raw = original;
    unsafe { libc::cfmakeraw(&mut raw) };
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(RawTerminalMode {
        original: Some(original),
    })
}

#[cfg(unix)]
fn restore_raw_terminal_mode(mode: &mut RawTerminalMode) {
    if let Some(original) = mode.original.take() {
        unsafe {
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &original);
        }
    }
}

#[cfg(unix)]
fn detect_terminal_size() -> Option<TerminalSize> {
    let mut size = std::mem::MaybeUninit::<libc::winsize>::zeroed();
    if unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, size.as_mut_ptr()) } != 0 {
        return None;
    }
    let size = unsafe { size.assume_init() };
    (size.ws_col > 0 && size.ws_row > 0).then_some(TerminalSize {
        cols: size.ws_col,
        rows: size.ws_row,
    })
}

#[cfg(windows)]
use windows::Win32::Foundation::HANDLE;
#[cfg(windows)]
use windows::Win32::System::Console::{
    GetConsoleMode, GetConsoleScreenBufferInfo, GetStdHandle, SetConsoleMode, CONSOLE_MODE,
    CONSOLE_SCREEN_BUFFER_INFO, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT,
    ENABLE_PROCESSED_OUTPUT, ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
    STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};

#[cfg(windows)]
struct WindowsConsoleState {
    input: HANDLE,
    input_mode: CONSOLE_MODE,
    output: HANDLE,
    output_mode: CONSOLE_MODE,
}

#[cfg(windows)]
fn enable_raw_terminal_mode() -> io::Result<RawTerminalMode> {
    let (input, output) = unsafe {
        (
            GetStdHandle(STD_INPUT_HANDLE).map_err(windows_io_error)?,
            GetStdHandle(STD_OUTPUT_HANDLE).map_err(windows_io_error)?,
        )
    };
    let mut input_mode = CONSOLE_MODE(0);
    let mut output_mode = CONSOLE_MODE(0);
    unsafe {
        GetConsoleMode(input, &mut input_mode).map_err(windows_io_error)?;
        GetConsoleMode(output, &mut output_mode).map_err(windows_io_error)?;
    }
    let raw_input = (input_mode
        & !(ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT))
        | ENABLE_VIRTUAL_TERMINAL_INPUT;
    let raw_output = output_mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING | ENABLE_PROCESSED_OUTPUT;
    unsafe { SetConsoleMode(input, raw_input).map_err(windows_io_error)? };
    if let Err(error) = unsafe { SetConsoleMode(output, raw_output) } {
        unsafe {
            let _ = SetConsoleMode(input, input_mode);
        }
        return Err(windows_io_error(error));
    }
    Ok(RawTerminalMode {
        state: Some(WindowsConsoleState {
            input,
            input_mode,
            output,
            output_mode,
        }),
    })
}

#[cfg(windows)]
fn restore_raw_terminal_mode(mode: &mut RawTerminalMode) {
    if let Some(state) = mode.state.take() {
        unsafe {
            let _ = SetConsoleMode(state.input, state.input_mode);
            let _ = SetConsoleMode(state.output, state.output_mode);
        }
    }
}

#[cfg(windows)]
fn detect_terminal_size() -> Option<TerminalSize> {
    let output = unsafe { GetStdHandle(STD_OUTPUT_HANDLE).ok()? };
    let mut info = CONSOLE_SCREEN_BUFFER_INFO::default();
    unsafe { GetConsoleScreenBufferInfo(output, &mut info).ok()? };
    let cols = (info.srWindow.Right - info.srWindow.Left + 1).max(1) as u16;
    let rows = (info.srWindow.Bottom - info.srWindow.Top + 1).max(1) as u16;
    Some(TerminalSize { cols, rows })
}

#[cfg(windows)]
fn windows_io_error(error: windows::core::Error) -> io::Error {
    io::Error::other(error.to_string())
}

#[cfg(not(any(unix, windows)))]
fn enable_raw_terminal_mode() -> io::Result<RawTerminalMode> {
    Ok(RawTerminalMode {})
}

#[cfg(not(any(unix, windows)))]
fn restore_raw_terminal_mode(_mode: &mut RawTerminalMode) {}

#[cfg(not(any(unix, windows)))]
fn detect_terminal_size() -> Option<TerminalSize> {
    None
}
