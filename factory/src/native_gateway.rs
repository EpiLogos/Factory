//! Bounded UDS transport for the Actuation Agency Gateway's
//! `actuation.gateway/v1` frames: one JSON object per line, the same frame
//! discipline as the gateway's own wire codec. This client speaks raw JSON
//! frames; the bearer token lives only in the session hello and the process
//! environment, never in configuration, receipts or error text.

use serde_json::Value;
use std::fmt::{Display, Formatter};
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::{SocketAddr, UnixStream};
use std::path::Path;
use std::time::{Duration, Instant};

/// The gateway protocol the hello frame negotiates and every reply confirms.
pub const GATEWAY_CONTRACT: &str = "actuation.gateway/v1";
/// The environment variable that must carry the gateway bearer token. It is
/// never accepted through invocation configuration.
pub const GATEWAY_TOKEN_ENV: &str = "ACTUATION_GATEWAY_TOKEN";
/// The same bound as native owner process output: a frame is capped, never
/// trusted to be small.
pub const MAX_GATEWAY_FRAME_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug)]
pub enum GatewayError {
    Io(io::Error),
    Json(serde_json::Error),
    /// A frame that violates the codec law, or a refusal reply. The gateway's
    /// own error string is retained verbatim.
    Frame(String),
    /// The gateway closed the connection without a reply frame.
    Closed,
}

impl Display for GatewayError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Json(error) => write!(formatter, "invalid gateway frame: {error}"),
            Self::Frame(detail) => write!(formatter, "{detail}"),
            Self::Closed => write!(formatter, "gateway closed the connection"),
        }
    }
}
impl std::error::Error for GatewayError {}
impl From<io::Error> for GatewayError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<serde_json::Error> for GatewayError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub struct GatewayConnection {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}

fn socket_timeout(deadline: Instant) -> io::Result<Duration> {
    let now = Instant::now();
    if now >= deadline {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "gateway transport budget exhausted",
        ));
    }
    Ok(deadline - now)
}

impl GatewayConnection {
    /// One granted connection over the host's UDS carrier. Unix connects are
    /// local and immediate; every later read and write is bounded by the
    /// caller's remaining deadline.
    pub fn connect(socket: &Path, deadline: Instant) -> Result<Self, GatewayError> {
        let address = SocketAddr::from_pathname(socket).map_err(|error| {
            GatewayError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid gateway socket path {}: {error}", socket.display()),
            ))
        })?;
        let stream = UnixStream::connect_addr(&address).map_err(|error| {
            GatewayError::Io(io::Error::new(
                error.kind(),
                format!("cannot reach gateway at {}: {error}", socket.display()),
            ))
        })?;
        let timeout = socket_timeout(deadline)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Self { stream, reader })
    }

    /// Refresh both socket timeouts from the remaining budget before each op.
    fn bound(&self, deadline: Instant) -> Result<(), GatewayError> {
        let timeout = socket_timeout(deadline)?;
        self.stream.set_read_timeout(Some(timeout))?;
        self.stream.set_write_timeout(Some(timeout))?;
        Ok(())
    }

    /// Send one frame and return the raw reply frame. A socket timeout
    /// surfaces as EAGAIN/WouldBlock on some platforms and TimedOut on
    /// others; both mean the same thing here, so they are normalised to one
    /// named deadline failure.
    pub fn call(&mut self, frame: &Value, deadline: Instant) -> Result<Value, GatewayError> {
        self.bound(deadline)?;
        write_frame(&mut self.stream, frame).map_err(normalise_deadline)?;
        match read_frame(&mut self.reader).map_err(normalise_deadline)? {
            Some(reply) => Ok(reply),
            None => Err(GatewayError::Closed),
        }
    }
}

fn normalise_deadline(error: GatewayError) -> GatewayError {
    match error {
        GatewayError::Io(io)
            if matches!(
                io.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ) =>
        {
            GatewayError::Io(io::Error::new(
                io::ErrorKind::TimedOut,
                "gateway transport budget exhausted",
            ))
        }
        other => other,
    }
}

/// The gateway's own frame law, bounded on read and write: a single JSON
/// object per line. Clean EOF is an empty frame; anything else that is not
/// one object is refused.
pub fn write_frame(writer: &mut impl Write, frame: &Value) -> Result<(), GatewayError> {
    let mut line = serde_json::to_string(frame)?;
    line.push('\n');
    if line.len() > MAX_GATEWAY_FRAME_BYTES {
        return Err(GatewayError::Frame(
            "gateway frame exceeded its byte bound".into(),
        ));
    }
    writer.write_all(line.as_bytes())?;
    writer.flush()?;
    Ok(())
}

pub fn read_frame(reader: &mut impl BufRead) -> Result<Option<Value>, GatewayError> {
    let mut line = Vec::new();
    loop {
        let available = match reader.fill_buf() {
            Ok(available) => available,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error.into()),
        };
        if available.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Err(GatewayError::Io(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "gateway frame ended without a newline",
                )))
            };
        }
        let taken = available
            .iter()
            .position(|&byte| byte == b'\n')
            .map(|index| index + 1)
            .unwrap_or(available.len());
        line.extend_from_slice(&available[..taken]);
        reader.consume(taken);
        if line.len() > MAX_GATEWAY_FRAME_BYTES {
            return Err(GatewayError::Frame(
                "gateway frame exceeded its byte bound".into(),
            ));
        }
        if line.last() == Some(&b'\n') {
            break;
        }
    }
    while line.last() == Some(&b'\n') || line.last() == Some(&b'\r') {
        line.pop();
    }
    let text = String::from_utf8(line)
        .map_err(|error| GatewayError::Io(io::Error::new(io::ErrorKind::InvalidData, error)))?;
    let frame: Value = serde_json::from_str(&text)?;
    if !frame.is_object() {
        return Err(GatewayError::Frame(
            "gateway frames must be JSON objects".into(),
        ));
    }
    Ok(Some(frame))
}

/// A reply that is never silence: every refusal names its standing, and the
/// gateway's own error string is surfaced, not swallowed.
pub fn demand_ok(reply: &Value, stage: &str) -> Result<(), GatewayError> {
    if reply.get("ok") == Some(&Value::Bool(true)) {
        return Ok(());
    }
    let detail = reply
        .get("error")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| reply.to_string());
    Err(GatewayError::Frame(format!("{stage} refused: {detail}")))
}

/// Post receipts wrap the durable event in `event`; wait replies list events
/// directly. Both serialise the event's fields at the top level.
pub fn receipt_event(receipt: &Value) -> &Value {
    receipt.get("event").unwrap_or(&Value::Null)
}

pub fn event_field<'a>(event: &'a Value, key: &str) -> Option<&'a Value> {
    event
        .get(key)
        .or_else(|| event.get("fields").and_then(|fields| fields.get(key)))
}

pub fn event_sequence(event: &Value) -> Option<u64> {
    event_field(event, "sequence").and_then(Value::as_u64)
}

pub fn event_ref(event: &Value) -> Option<&str> {
    event_field(event, "event_ref").and_then(Value::as_str)
}

pub fn event_kind(event: &Value) -> Option<&str> {
    event_field(event, "kind").and_then(Value::as_str)
}

pub fn event_return_ref(event: &Value) -> Option<&str> {
    event_field(event, "return_ref").and_then(Value::as_str)
}

#[cfg(test)]
mod gateway_transport_regressions {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn frames_roundtrip_as_single_json_objects_per_line() {
        let frame = serde_json::json!({"op":"hello","protocol":GATEWAY_CONTRACT,"subject":"connector:test"});
        let mut writer: Vec<u8> = Vec::new();
        write_frame(&mut writer, &frame).unwrap();
        assert!(writer.ends_with(b"\n"));
        let mut reader = BufReader::new(Cursor::new(writer));
        assert_eq!(read_frame(&mut reader).unwrap().unwrap(), frame);
        assert!(
            read_frame(&mut reader).unwrap().is_none(),
            "clean EOF is an empty frame"
        );
    }

    #[test]
    fn non_object_and_malformed_frames_are_refused() {
        let mut reader = BufReader::new(Cursor::new(b"[1,2]\n".to_vec()));
        let error = read_frame(&mut reader).unwrap_err();
        assert!(error.to_string().contains("JSON objects"));
        let mut reader = BufReader::new(Cursor::new(b"not json\n".to_vec()));
        assert!(read_frame(&mut reader).is_err());
        let mut reader = BufReader::new(Cursor::new(b"truncated".to_vec()));
        assert!(read_frame(&mut reader).is_err());
    }

    #[test]
    fn oversized_frames_are_refused_not_truncated() {
        let oversized = format!("\"{}\"\n", "x".repeat(MAX_GATEWAY_FRAME_BYTES + 1));
        let mut reader = BufReader::new(Cursor::new(oversized.into_bytes()));
        assert!(matches!(
            read_frame(&mut reader),
            Err(GatewayError::Frame(detail)) if detail.contains("byte bound")
        ));
    }

    #[test]
    fn refusals_surface_the_gateway_error_string() {
        let refused = serde_json::json!({"ok":false,"denied":true,"error":"authentication failed"});
        let error = demand_ok(&refused, "hello").unwrap_err();
        assert!(error
            .to_string()
            .contains("hello refused: authentication failed"));
        let shapeless = serde_json::json!({"unexpected":true});
        let error = demand_ok(&shapeless, "attach").unwrap_err();
        assert!(error.to_string().contains("attach refused"));
    }

    #[test]
    fn event_fields_are_read_from_receipts_and_wait_pages() {
        let receipt = serde_json::json!({
            "stream_ref":"stream:test","cursor":{"last_sequence":2},"lifecycle":"open",
            "event":{"event_ref":"actuation:event:a","sequence":2,"kind":"return",
                     "return_ref":"return:a","content":"done"}
        });
        let event = receipt_event(&receipt);
        assert_eq!(event_ref(event), Some("actuation:event:a"));
        assert_eq!(event_sequence(event), Some(2));
        assert_eq!(event_kind(event), Some("return"));
        assert_eq!(event_return_ref(event), Some("return:a"));
        let nested = serde_json::json!({"fields":{"sequence":7,"kind":"tool-result"}});
        assert_eq!(event_sequence(&nested), Some(7));
    }
}
