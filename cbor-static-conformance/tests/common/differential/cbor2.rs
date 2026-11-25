//! Python's cbor2, through one `harness.cbor2_oracle` child per test process, which every test
//! shares behind a lock: a request is one item, and its reply cbor2's re-encoding of what it
//! decodes, or why it refuses the item.

use std::process::{ChildStdin, ChildStdout};
use std::sync::mpsc::Receiver;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::hex;

const COMMAND: &str = "uv run --locked --quiet python -m harness.cbor2_oracle";
const REPLY_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reply {
    Reencoded {
        #[serde(deserialize_with = "hex::deserialize")]
        cbor: Vec<u8>,
    },
    Refused {
        reason: String,
    },
}

#[derive(Serialize)]
struct Request<'a> {
    #[serde(serialize_with = "hex::serialize")]
    cbor: &'a [u8],
}

struct Child {
    process: std::process::Child,
    requests: ChildStdin,
    replies: Receiver<String>,
}

static CHILD: LazyLock<Mutex<Child>> = LazyLock::new(|| Mutex::new(spawned()));

/// cbor2's reply to `item`. A child that does not reply in time is replaced before the panic, so
/// its late reply never answers the next request.
///
/// # Panics
///
/// If the child cannot start, has exited, or does not reply within `REPLY_TIMEOUT`.
pub fn reply(item: &[u8]) -> Reply {
    use std::io::Write;
    use std::sync::PoisonError;
    use std::sync::mpsc::RecvTimeoutError;

    let mut child = CHILD.lock().unwrap_or_else(PoisonError::into_inner);
    writeln!(
        child.requests,
        "{request}",
        request = serde_json::to_string(&Request { cbor: item }).expect("a request serializes")
    )
    .and_then(|()| child.requests.flush())
    .unwrap_or_else(|error| panic!("cbor2 oracle: `{COMMAND}` has exited: {error}"));
    match child.replies.recv_timeout(REPLY_TIMEOUT) {
        Ok(line) => serde_json::from_str(&line)
            .unwrap_or_else(|error| panic!("cbor2 oracle: unreadable reply {line:?}: {error}")),
        Err(RecvTimeoutError::Timeout) => {
            child.process.kill().ok();
            *child = spawned();
            panic!("cbor2 oracle: `{COMMAND}` did not reply within {REPLY_TIMEOUT:?}")
        }
        Err(RecvTimeoutError::Disconnected) => {
            panic!("cbor2 oracle: `{COMMAND}` exited; its stderr is above")
        }
    }
}

fn spawned() -> Child {
    use std::process::{Command, Stdio};

    let (program, arguments) = COMMAND
        .split_once(' ')
        .expect("a program and its arguments");
    let mut process = Command::new(program)
        .args(arguments.split(' '))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap_or_else(|error| panic!("cbor2 oracle: cannot start `{COMMAND}`: {error}"));
    Child {
        requests: process.stdin.take().expect("stdin is piped"),
        replies: lines_of(process.stdout.take().expect("stdout is piped")),
        process,
    }
}

/// The child's lines as they arrive, read on a thread of their own so a reply can time out.
fn lines_of(stdout: ChildStdout) -> Receiver<String> {
    use std::io::{BufRead, BufReader};

    let (sender, lines) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
            .try_for_each(|line| sender.send(line))
    });
    lines
}
