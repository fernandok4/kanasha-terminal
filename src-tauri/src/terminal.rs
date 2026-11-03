use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::{
    io::{Read, Write},
    path::Path,
    thread,
};

// A PTY read is an arbitrary byte boundary, including the middle of a UTF-8
// character. Keep incomplete characters for the next read so they occupy the
// same number of terminal cells as the application intended.
#[derive(Default)]
struct OutputDecoder {
    pending: Vec<u8>,
}

impl OutputDecoder {
    fn decode(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut output = String::new();
        let mut consumed = 0;
        while consumed < self.pending.len() {
            match std::str::from_utf8(&self.pending[consumed..]) {
                Ok(text) => {
                    output.push_str(text);
                    consumed = self.pending.len();
                }
                Err(error) => {
                    let valid_end = consumed + error.valid_up_to();
                    output
                        .push_str(std::str::from_utf8(&self.pending[consumed..valid_end]).unwrap());
                    consumed = valid_end;
                    match error.error_len() {
                        Some(length) => {
                            output.push('\u{fffd}');
                            consumed += length;
                        }
                        None => break,
                    }
                }
            }
        }
        self.pending.drain(..consumed);
        output
    }

    fn finish(self) -> String {
        String::from_utf8_lossy(&self.pending).into_owned()
    }
}

pub struct PtySession {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
}

impl PtySession {
    pub fn write(&mut self, input: &str) -> Result<(), String> {
        self.writer
            .write_all(input.as_bytes())
            .and_then(|_| self.writer.flush())
            .map_err(|error| format!("Não foi possível enviar dados ao terminal: {error}"))
    }

    pub fn resize(&self, columns: u16, rows: u16) -> Result<(), String> {
        self.master
            .resize(PtySize {
                rows,
                cols: columns,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("Não foi possível redimensionar o terminal: {error}"))
    }

    pub fn stop(&mut self) -> Result<(), String> {
        self.child
            .kill()
            .map_err(|error| format!("Não foi possível encerrar o terminal: {error}"))?;
        self.child.wait().map(|_| ()).map_err(|error| {
            format!("Não foi possível aguardar o encerramento do terminal: {error}")
        })
    }
}

pub fn start_pty<F, E>(
    executable: &str,
    arguments: &[String],
    environment: &[(String, String)],
    root_path: &Path,
    on_output: F,
    on_exit: E,
) -> Result<PtySession, String>
where
    F: Fn(String) + Send + 'static,
    E: FnOnce() + Send + 'static,
{
    if !root_path.is_dir() {
        return Err("A pasta-raiz da área não está disponível.".to_string());
    }

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 36,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| format!("Não foi possível criar o PTY: {error}"))?;

    let mut command = CommandBuilder::new(executable);
    command.args(arguments);
    for (key, value) in environment {
        command.env(key, value);
    }
    command.cwd(root_path);

    let child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| format!("Não foi possível iniciar o perfil local: {error}"))?;
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("Não foi possível ler a saída do PTY: {error}"))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|error| format!("Não foi possível preparar a entrada do PTY: {error}"))?;

    thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        let mut decoder = OutputDecoder::default();
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(size) => {
                    let output = decoder.decode(&buffer[..size]);
                    if !output.is_empty() {
                        on_output(output);
                    }
                }
            }
        }
        let remaining = decoder.finish();
        if !remaining.is_empty() {
            on_output(remaining);
        }
        on_exit();
    });

    Ok(PtySession {
        writer,
        master: pair.master,
        child,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };

    #[test]
    fn output_preserves_utf8_and_cursor_commands_at_every_read_boundary() {
        let expected = "\x1b[2J\x1b[3;5HCorreção: ação, 日本語, 🦀\r\n\x1b[4Cfim";
        for boundary in 0..=expected.len() {
            let mut decoder = OutputDecoder::default();
            let mut actual = decoder.decode(&expected.as_bytes()[..boundary]);
            actual.push_str(&decoder.decode(&expected.as_bytes()[boundary..]));
            actual.push_str(&decoder.finish());
            assert_eq!(actual, expected, "boundary {boundary}");
        }
        let mut decoder = OutputDecoder::default();
        let actual: String = expected
            .as_bytes()
            .iter()
            .map(|byte| decoder.decode(&[*byte]))
            .collect();
        assert_eq!(actual + &decoder.finish(), expected);
    }

    #[test]
    fn invalid_utf8_and_incomplete_eof_do_not_discard_following_output() {
        let mut decoder = OutputDecoder::default();
        assert_eq!(decoder.decode(b"a\xff\x1b[2J\xf0\x9f"), "a\u{fffd}\x1b[2J");
        assert_eq!(decoder.decode(b"\xa6"), "");
        assert_eq!(decoder.finish(), "\u{fffd}");
        let mut decoder = OutputDecoder::default();
        assert_eq!(decoder.decode(b"\xc3"), "");
        assert_eq!(decoder.decode(b"\r\nnext"), "\u{fffd}\r\nnext");
    }

    #[cfg(unix)]
    #[test]
    fn pty_round_trip_preserves_input_and_output() {
        let output = Arc::new(Mutex::new(String::new()));
        let output_for_callback = Arc::clone(&output);
        let environment = vec![(
            "KANASHA_TEST_WORKSPACE".to_string(),
            "workspace-42".to_string(),
        )];
        let mut session = start_pty(
            "/bin/sh",
            &[],
            &environment,
            Path::new("/tmp"),
            move |chunk| {
                output_for_callback
                    .lock()
                    .expect("output lock")
                    .push_str(&chunk)
            },
            || {},
        )
        .expect("PTY should start");

        session
            .write("printf 'kanasha-pty-ok:%s\\n' \"$KANASHA_TEST_WORKSPACE\"\n")
            .expect("PTY should accept input");

        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if output
                .lock()
                .expect("output lock")
                .contains("kanasha-pty-ok:workspace-42")
            {
                session.stop().expect("PTY should stop");
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }

        let captured = output.lock().expect("output lock").clone();
        let _ = session.stop();
        panic!("PTY did not return the written command. Output: {captured:?}");
    }

    #[cfg(unix)]
    #[test]
    fn independent_ptys_do_not_mix_their_io() {
        let first_output = Arc::new(Mutex::new(String::new()));
        let second_output = Arc::new(Mutex::new(String::new()));
        let first_callback = Arc::clone(&first_output);
        let second_callback = Arc::clone(&second_output);
        let mut first = start_pty(
            "/bin/sh",
            &[],
            &[],
            Path::new("/tmp"),
            move |chunk| {
                first_callback
                    .lock()
                    .expect("first output lock")
                    .push_str(&chunk)
            },
            || {},
        )
        .expect("first PTY should start");
        let mut second = start_pty(
            "/bin/sh",
            &[],
            &[],
            Path::new("/tmp"),
            move |chunk| {
                second_callback
                    .lock()
                    .expect("second output lock")
                    .push_str(&chunk)
            },
            || {},
        )
        .expect("second PTY should start");

        first
            .write("printf 'kanasha-first-pty\\n'\\n")
            .expect("first PTY input");
        second
            .write("printf 'kanasha-second-pty\\n'\\n")
            .expect("second PTY input");

        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            let captured_first = first_output.lock().expect("first output lock").clone();
            let captured_second = second_output.lock().expect("second output lock").clone();
            if captured_first.contains("kanasha-first-pty")
                && captured_second.contains("kanasha-second-pty")
            {
                assert!(!captured_first.contains("kanasha-second-pty"));
                assert!(!captured_second.contains("kanasha-first-pty"));
                first.stop().expect("first PTY should stop");
                second.stop().expect("second PTY should stop");
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }

        let _ = first.stop();
        let _ = second.stop();
        panic!("Os PTYs não produziram suas saídas independentes a tempo.");
    }
}
