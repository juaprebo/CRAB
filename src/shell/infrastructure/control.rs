pub struct Reply {
    pub ok: bool,
    pub text: String,
}

#[cfg(unix)]
mod imp {
    use super::Reply;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::time::Duration;

    pub struct Request {
        pub line: String,
        pub reply: Sender<Reply>,
    }

    pub struct Server {
        pub requests: Receiver<Request>,
        path: PathBuf,
    }

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    pub fn socket_path() -> PathBuf {
        match std::env::var_os("XDG_RUNTIME_DIR") {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir).join("crab.sock"),
            _ => std::env::temp_dir().join(format!("crab-{}.sock", unsafe { libc::getuid() })),
        }
    }

    pub fn start() -> Option<Server> {
        start_at(socket_path())
    }

    pub fn start_at(path: PathBuf) -> Option<Server> {
        if UnixStream::connect(&path).is_ok() {
            return None;
        }
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).ok()?;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        let (tx, requests) = channel();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let tx = tx.clone();
                std::thread::spawn(move || serve(stream, tx));
            }
        });
        Some(Server { requests, path })
    }

    fn serve(stream: UnixStream, requests: Sender<Request>) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let mut line = String::new();
        if BufReader::new(&stream).read_line(&mut line).is_err() {
            return;
        }
        let line = line.trim().to_string();
        if line.is_empty() {
            return;
        }
        let (reply, answer) = channel();
        if requests.send(Request { line, reply }).is_err() {
            return;
        }
        let reply = answer
            .recv_timeout(Duration::from_secs(2))
            .unwrap_or(Reply {
                ok: false,
                text: "CRAB did not answer".into(),
            });
        let mut stream = stream;
        let _ = writeln!(
            stream,
            "{} {}",
            if reply.ok { "ok" } else { "error" },
            reply.text.replace('\n', " ")
        );
    }

    pub fn call_at(path: &std::path::Path, line: &str) -> Result<Reply, String> {
        let mut stream =
            UnixStream::connect(path).map_err(|_| "CRAB is not running".to_string())?;
        let timeout = Some(Duration::from_secs(4));
        let _ = stream.set_read_timeout(timeout);
        let _ = stream.set_write_timeout(timeout);
        writeln!(stream, "{line}").map_err(|e| e.to_string())?;
        let mut answer = String::new();
        BufReader::new(&stream)
            .read_line(&mut answer)
            .map_err(|e| e.to_string())?;
        let answer = answer.trim_end();
        let (status, text) = answer.split_once(' ').unwrap_or((answer, ""));
        match status {
            "ok" => Ok(Reply {
                ok: true,
                text: text.to_string(),
            }),
            "error" => Ok(Reply {
                ok: false,
                text: text.to_string(),
            }),
            _ => Err("CRAB sent an unexpected answer".to_string()),
        }
    }

    pub fn call(line: &str) -> Result<Reply, String> {
        call_at(&socket_path(), line)
    }
}

#[cfg(unix)]
pub use imp::{Server, call, start};

pub fn quote_arg(arg: &str) -> String {
    if arg.is_empty() || arg.contains(|c: char| c.is_whitespace() || c == '"' || c == '\\') {
        format!("\"{}\"", arg.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        arg.to_string()
    }
}

pub fn command_line(args: &[String]) -> String {
    let opens = args.first().is_some_and(|name| name == "open");
    let mut line = String::new();
    for (index, arg) in args.iter().enumerate() {
        if index > 0 {
            line.push(' ');
        }
        if index == 0 {
            line.push_str(arg);
        } else if opens {
            let absolute = std::fs::canonicalize(arg).unwrap_or_else(|_| arg.into());
            line.push_str(&quote_arg(&absolute.to_string_lossy()));
        } else {
            line.push_str(&quote_arg(arg));
        }
    }
    line
}

#[cfg(unix)]
pub fn open_in_running(files: &[std::path::PathBuf]) -> bool {
    let mut args = vec!["open".to_string()];
    args.extend(files.iter().map(|f| f.to_string_lossy().into_owned()));
    matches!(call(&command_line(&args)), Ok(reply) if reply.ok)
}

#[cfg(not(unix))]
pub fn open_in_running(_files: &[std::path::PathBuf]) -> bool {
    false
}

#[cfg(unix)]
pub fn run_client(args: &[String]) -> i32 {
    if args.is_empty() || matches!(args[0].as_str(), "-h" | "--help") {
        println!("Usage: crab ctl <command>");
        println!("Runs any : command in the CRAB that is open, for example:");
        println!("  crab ctl next        crab ctl volume 60        crab ctl seek +10");
        println!("  crab ctl status      prints the current state as JSON");
        return if args.is_empty() { 1 } else { 0 };
    }
    match call(&command_line(args)) {
        Ok(reply) if reply.ok => {
            if !reply.text.is_empty() {
                println!("{}", reply.text);
            }
            0
        }
        Ok(reply) => {
            eprintln!("{}", reply.text);
            1
        }
        Err(error) => {
            eprintln!("{error}");
            1
        }
    }
}

#[cfg(not(unix))]
pub struct Request {
    pub line: String,
    pub reply: std::sync::mpsc::Sender<Reply>,
}

#[cfg(not(unix))]
pub struct Server {
    pub requests: std::sync::mpsc::Receiver<Request>,
}

#[cfg(not(unix))]
pub fn start() -> Option<Server> {
    None
}

#[cfg(not(unix))]
pub fn run_client(_args: &[String]) -> i32 {
    eprintln!("crab ctl is only available on Unix systems");
    1
}

#[cfg(all(test, unix))]
mod tests {
    use super::imp::*;
    use super::*;

    #[test]
    fn quotes_arguments_with_spaces() {
        assert_eq!(quote_arg("plain"), "plain");
        assert_eq!(quote_arg("two words"), "\"two words\"");
        assert_eq!(quote_arg("a\"b"), "\"a\\\"b\"");
        assert_eq!(
            command_line(&["play".into(), "midnight run".into()]),
            "play \"midnight run\""
        );
    }

    #[test]
    fn serves_requests_and_reports_errors() {
        let dir = crate::testutil::temp_dir("ctl");
        let path = dir.join("crab.sock");
        let server = start_at(path.clone()).unwrap();
        assert!(start_at(path.clone()).is_none());
        let worker = std::thread::spawn({
            let path = path.clone();
            move || {
                let ok = call_at(&path, "volume 40").unwrap();
                let bad = call_at(&path, "nonsense").unwrap();
                (ok.ok, ok.text, bad.ok, bad.text)
            }
        });
        for _ in 0..2 {
            let request = server
                .requests
                .recv_timeout(std::time::Duration::from_secs(3))
                .unwrap();
            let reply = if request.line == "volume 40" {
                Reply {
                    ok: true,
                    text: "done".into(),
                }
            } else {
                Reply {
                    ok: false,
                    text: "Unknown command".into(),
                }
            };
            request.reply.send(reply).unwrap();
        }
        let (ok, ok_text, bad, bad_text) = worker.join().unwrap();
        assert!(ok && !bad);
        assert_eq!(ok_text, "done");
        assert_eq!(bad_text, "Unknown command");
        drop(server);
        assert!(!path.exists());
        assert!(call_at(&path, "next").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
