//! bsread over libzmq.
//!
//! The symbols are the ZeroMQ 4.3 C ABI (`zmq_socket`, `zmq_send`, `zmq_recv`).
//! A PUSH socket sends the three bsread parts with `ZMQ_SNDMORE`. A PULL socket
//! on the other side, including a pyzmq peer, reads the same frames.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::time::Duration;

use crate::bsread::{bsread_parts, decode_bsread_parts, BsMessage};
use crate::error::DsError;

const ZMQ_PUB: c_int = 1;
const ZMQ_SUB: c_int = 2;
const ZMQ_PULL: c_int = 7;
const ZMQ_PUSH: c_int = 8;
const ZMQ_SUBSCRIBE: c_int = 6;
const ZMQ_RCVMORE: c_int = 13;
const ZMQ_LINGER: c_int = 17;
const ZMQ_RCVTIMEO: c_int = 27;
const ZMQ_LAST_ENDPOINT: c_int = 32;
const ZMQ_SNDMORE: c_int = 2;

#[link(name = "zmq")]
extern "C" {
    fn zmq_ctx_new() -> *mut c_void;
    fn zmq_ctx_term(context: *mut c_void) -> c_int;
    fn zmq_socket(context: *mut c_void, kind: c_int) -> *mut c_void;
    fn zmq_close(socket: *mut c_void) -> c_int;
    fn zmq_bind(socket: *mut c_void, addr: *const c_char) -> c_int;
    fn zmq_connect(socket: *mut c_void, addr: *const c_char) -> c_int;
    fn zmq_send(socket: *mut c_void, buf: *const c_void, len: usize, flags: c_int) -> c_int;
    fn zmq_recv(socket: *mut c_void, buf: *mut c_void, len: usize, flags: c_int) -> c_int;
    fn zmq_setsockopt(
        socket: *mut c_void,
        option: c_int,
        value: *const c_void,
        len: usize,
    ) -> c_int;
    fn zmq_getsockopt(
        socket: *mut c_void,
        option: c_int,
        value: *mut c_void,
        len: *mut usize,
    ) -> c_int;
    fn zmq_strerror(errnum: c_int) -> *const c_char;
    fn zmq_errno() -> c_int;
}

pub struct Socket {
    ctx: *mut c_void,
    sock: *mut c_void,
}

impl Drop for Socket {
    fn drop(&mut self) {
        unsafe {
            if !self.sock.is_null() {
                let linger: c_int = 1_000;
                zmq_setsockopt(
                    self.sock,
                    ZMQ_LINGER,
                    &linger as *const c_int as *const c_void,
                    std::mem::size_of::<c_int>(),
                );
                zmq_close(self.sock);
            }
            if !self.ctx.is_null() {
                zmq_ctx_term(self.ctx);
            }
        }
    }
}

fn last_error(what: &str) -> DsError {
    let text = unsafe {
        let ptr = zmq_strerror(zmq_errno());
        if ptr.is_null() {
            "unknown".to_string()
        } else {
            CStr::from_ptr(ptr).to_string_lossy().into_owned()
        }
    };
    DsError::new(format!("{what}: {text}"))
}

fn endpoint_of(sock: *mut c_void) -> Result<String, DsError> {
    let mut buf = [0u8; 256];
    let mut len = buf.len();
    let rc = unsafe {
        zmq_getsockopt(
            sock,
            ZMQ_LAST_ENDPOINT,
            buf.as_mut_ptr() as *mut c_void,
            &mut len,
        )
    };
    if rc != 0 {
        return Err(last_error("zmq endpoint"));
    }
    let n = len.saturating_sub(1).min(buf.len());
    let text = String::from_utf8_lossy(&buf[..n])
        .trim_end_matches('\0')
        .to_string();
    Ok(text)
}

/// Bind a PUSH socket and return its `tcp://` endpoint.
pub fn bind_push() -> Result<(Socket, String), DsError> {
    bind_kind(ZMQ_PUSH)
}

/// Bind a PULL socket and return its `tcp://` endpoint.
pub fn bind_pull() -> Result<(Socket, String), DsError> {
    bind_kind(ZMQ_PULL)
}

/// Bind a PUB socket. Subscribers must set an empty subscription.
pub fn bind_pub() -> Result<(Socket, String), DsError> {
    bind_kind(ZMQ_PUB)
}

fn bind_kind(kind: c_int) -> Result<(Socket, String), DsError> {
    unsafe {
        let ctx = zmq_ctx_new();
        if ctx.is_null() {
            return Err(last_error("zmq context"));
        }
        let sock = zmq_socket(ctx, kind);
        if sock.is_null() {
            zmq_ctx_term(ctx);
            return Err(last_error("zmq socket"));
        }
        let addr = CString::new("tcp://127.0.0.1:*").map_err(|_| DsError::new("zmq address"))?;
        if zmq_bind(sock, addr.as_ptr()) != 0 {
            let err = last_error("zmq bind");
            zmq_close(sock);
            zmq_ctx_term(ctx);
            return Err(err);
        }
        if kind == ZMQ_PULL || kind == ZMQ_SUB {
            let millis: c_int = 4_000;
            zmq_setsockopt(
                sock,
                ZMQ_RCVTIMEO,
                &millis as *const c_int as *const c_void,
                std::mem::size_of::<c_int>(),
            );
        }
        let endpoint = endpoint_of(sock)?;
        Ok((Socket { ctx, sock }, endpoint))
    }
}

pub fn connect_pull(endpoint: &str, timeout: Duration) -> Result<Socket, DsError> {
    connect_kind(ZMQ_PULL, endpoint, timeout)
}

pub fn connect_sub(endpoint: &str, timeout: Duration) -> Result<Socket, DsError> {
    let socket = connect_kind(ZMQ_SUB, endpoint, timeout)?;
    let empty: [u8; 0] = [];
    let rc = unsafe {
        zmq_setsockopt(
            socket.sock,
            ZMQ_SUBSCRIBE,
            empty.as_ptr() as *const c_void,
            0,
        )
    };
    if rc != 0 {
        return Err(last_error("zmq subscribe"));
    }
    Ok(socket)
}

fn connect_kind(kind: c_int, endpoint: &str, timeout: Duration) -> Result<Socket, DsError> {
    unsafe {
        let ctx = zmq_ctx_new();
        if ctx.is_null() {
            return Err(last_error("zmq context"));
        }
        let sock = zmq_socket(ctx, kind);
        if sock.is_null() {
            zmq_ctx_term(ctx);
            return Err(last_error("zmq socket"));
        }
        let millis = timeout.as_millis().min(i32::MAX as u128) as c_int;
        zmq_setsockopt(
            sock,
            ZMQ_RCVTIMEO,
            &millis as *const c_int as *const c_void,
            std::mem::size_of::<c_int>(),
        );
        let addr = CString::new(endpoint).map_err(|_| DsError::new("zmq address"))?;
        if zmq_connect(sock, addr.as_ptr()) != 0 {
            let err = last_error("zmq connect");
            zmq_close(sock);
            zmq_ctx_term(ctx);
            return Err(err);
        }
        Ok(Socket { ctx, sock })
    }
}

pub fn send_parts(socket: &Socket, parts: &[Vec<u8>]) -> Result<(), DsError> {
    if parts.is_empty() {
        return Err(DsError::new("zmq send is empty"));
    }
    for (index, part) in parts.iter().enumerate() {
        let flags = if index + 1 == parts.len() {
            0
        } else {
            ZMQ_SNDMORE
        };
        let rc = unsafe {
            zmq_send(
                socket.sock,
                part.as_ptr() as *const c_void,
                part.len(),
                flags,
            )
        };
        if rc < 0 {
            return Err(last_error("zmq send"));
        }
    }
    Ok(())
}

pub fn recv_parts(socket: &Socket) -> Result<Vec<Vec<u8>>, DsError> {
    let mut parts = Vec::new();
    loop {
        let mut buf = vec![0u8; 65536];
        let rc = unsafe { zmq_recv(socket.sock, buf.as_mut_ptr() as *mut c_void, buf.len(), 0) };
        if rc < 0 {
            return Err(last_error("zmq recv"));
        }
        buf.truncate(rc as usize);
        parts.push(buf);
        let mut more: i32 = 0;
        let mut len = std::mem::size_of::<i32>();
        let got = unsafe {
            zmq_getsockopt(
                socket.sock,
                ZMQ_RCVMORE,
                &mut more as *mut i32 as *mut c_void,
                &mut len,
            )
        };
        if got != 0 || more == 0 {
            break;
        }
    }
    Ok(parts)
}

pub fn send_bsread_libzmq(endpoint: &str, message: &BsMessage) -> Result<(), DsError> {
    let socket = connect_kind(ZMQ_PUSH, endpoint, Duration::from_millis(800))?;
    send_parts(&socket, &bsread_parts(message))
}

pub fn recv_bsread_libzmq(endpoint: &str, timeout: Duration) -> Result<BsMessage, DsError> {
    let socket = connect_pull(endpoint, timeout)?;
    let parts = recv_parts(&socket)?;
    decode_bsread_parts(&parts)
}

/// Bind a PULL socket, send is the caller's job via the returned socket.
pub fn serve_bsread_pull() -> Result<(Socket, String), DsError> {
    bind_pull()
}

pub fn take_bsread(socket: &Socket) -> Result<BsMessage, DsError> {
    decode_bsread_parts(&recv_parts(socket)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn sample() -> BsMessage {
        BsMessage {
            pulse_id: 42,
            channel: "LIBZMQ:CH".into(),
            value: 3.5,
        }
    }

    #[test]
    fn rust_push_reaches_rust_pull() {
        let (pull, endpoint) = bind_pull().expect("pull");
        let endpoint_for_sender = endpoint.clone();
        let message = sample();
        let sender = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(40));
            send_bsread_libzmq(&endpoint_for_sender, &message).expect("push");
        });
        let got = take_bsread(&pull).expect("recv");
        sender.join().expect("sender");
        assert_eq!(got, sample());
    }

    #[test]
    fn rust_push_reaches_pyzmq_pull() {
        let (push, endpoint) = bind_push().expect("push");
        let dir = std::env::temp_dir().join(format!("bsread-pyzmq-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("pull.py");
        let out = dir.join("parts.bin");
        std::fs::write(
            &script,
            r#"
import sys, zmq
ctx = zmq.Context()
sock = ctx.socket(zmq.PULL)
sock.setsockopt(zmq.RCVTIMEO, 4000)
sock.connect(sys.argv[1])
parts = [sock.recv()]
while sock.getsockopt(zmq.RCVMORE):
    parts.append(sock.recv())
with open(sys.argv[2], "wb") as handle:
    handle.write(len(parts).to_bytes(4, "little"))
    for part in parts:
        handle.write(len(part).to_bytes(4, "little"))
        handle.write(part)
"#,
        )
        .unwrap();
        let mut child = std::process::Command::new("python3")
            .arg(&script)
            .arg(&endpoint)
            .arg(&out)
            .spawn()
            .expect("python3");
        std::thread::sleep(Duration::from_millis(250));
        send_parts(&push, &bsread_parts(&sample())).expect("send");
        let status = child.wait().expect("wait");
        assert!(status.success(), "pyzmq pull failed");
        let bytes = std::fs::read(&out).unwrap();
        let count = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        let mut parts = Vec::new();
        let mut at = 4;
        for _ in 0..count {
            let len = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
            at += 4;
            parts.push(bytes[at..at + len].to_vec());
            at += len;
        }
        assert_eq!(decode_bsread_parts(&parts).unwrap(), sample());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pyzmq_push_reaches_rust_pull() {
        let (pull, endpoint) = bind_pull().expect("pull");
        let script = std::env::temp_dir().join(format!("bsread-push-{}.py", std::process::id()));
        let mut file = std::fs::File::create(&script).unwrap();
        writeln!(
            file,
            r#"
import sys, time, zmq
ctx = zmq.Context()
sock = ctx.socket(zmq.PUSH)
sock.connect(sys.argv[1])
time.sleep(0.2)
sock.send_multipart([sys.argv[2].encode(), sys.argv[3].encode(), bytes.fromhex(sys.argv[4])])
"#
        )
        .unwrap();
        drop(file);
        let message = sample();
        let parts = bsread_parts(&message);
        let value_hex = parts[2]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let mut child = std::process::Command::new("python3")
            .arg(&script)
            .arg(&endpoint)
            .arg(String::from_utf8(parts[0].clone()).unwrap())
            .arg(String::from_utf8(parts[1].clone()).unwrap())
            .arg(value_hex)
            .spawn()
            .expect("python3");
        let got = take_bsread(&pull).expect("recv");
        let status = child.wait().expect("wait");
        assert!(status.success());
        assert_eq!(got, message);
        let _ = std::fs::remove_file(&script);
    }

    #[test]
    fn pub_reaches_a_subscriber() {
        let (publisher, endpoint) = bind_pub().expect("pub");
        let subscriber = connect_sub(&endpoint, Duration::from_millis(800)).expect("sub");
        std::thread::sleep(Duration::from_millis(80));
        send_parts(&publisher, &bsread_parts(&sample())).expect("pub send");
        let got = take_bsread(&subscriber).expect("sub recv");
        assert_eq!(got.channel, "LIBZMQ:CH");
    }
}
