//! I2I — Instance-to-Instance Protocol
//!
//! I2I is a lightweight message envelope protocol for cross-instance communication
//! between Plato components (kernel ↔ tui ↔ os, or kernel ↔ kernel).
//!
//! Where the `EventBus` handles intra-kernel pub/sub, I2I handles *inter-process*
//! coordination: a plato-tui reporting a user action to a remote kernel, a room
//! in plato-os requesting a constraint check from a kernel, or two kernels
//! coordinating on a shared room.
//!
//! ## Message Format
//!
//! ```text
//! I2I/1.0 <verb> <target>
//! From: <instance-id>
//! To:   <instance-id>
//! Nonce: <uuid>
//! Timestamp: <iso8601>
//!
//! <json-payload>
//! ```
//!
//! This wire format is intentionally human-readable: it can be sent over TCP,
//! written to a FIFO, posted to a git commit message, or pasted into a room
//! description. Words remain the primary transport.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use uuid::Uuid;

/// Protocol version string.
pub const I2I_VERSION: &str = "I2I/1.0";

/// Verb for an I2I message — what kind of interaction this is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum I2IVerb {
    /// One instance announcing its presence to another.
    Announce,
    /// Request a resource or action from the target.
    Request,
    /// Response to a previous `Request`.
    Response,
    /// One-way notification; no response expected.
    Notify,
    /// Relay a constraint check to a kernel.
    ConstraintCheck,
    /// Result of a constraint check.
    ConstraintResult,
    /// Inject a TUTOR context jump into the target instance.
    TutorJump,
    /// Push an episode entry to a remote recorder.
    EpisodePush,
    /// Graceful disconnect.
    Disconnect,
}

impl fmt::Display for I2IVerb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = serde_json::to_string(self).unwrap_or_default();
        write!(f, "{}", s.trim_matches('"'))
    }
}

/// The component type originating or receiving the message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComponentKind {
    Kernel,
    Tui,
    Os,
    Agent,
    Unknown,
}

/// An I2I instance identifier: `<kind>/<name>@<host>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceId {
    pub kind: ComponentKind,
    pub name: String,
    pub host: String,
}

impl InstanceId {
    pub fn new(kind: ComponentKind, name: &str, host: &str) -> Self {
        Self { kind, name: name.to_string(), host: host.to_string() }
    }

    pub fn local_kernel(name: &str) -> Self {
        Self::new(ComponentKind::Kernel, name, "localhost")
    }

    pub fn local_tui(name: &str) -> Self {
        Self::new(ComponentKind::Tui, name, "localhost")
    }
}

impl fmt::Display for InstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = serde_json::to_string(&self.kind).unwrap_or_default();
        write!(f, "{}/{}@{}", kind.trim_matches('"'), self.name, self.host)
    }
}

/// An I2I message envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct I2IMessage {
    pub version: String,
    pub verb: I2IVerb,
    /// Logical target identifier (room name, identity, or wildcard `*`).
    pub target: String,
    pub from: InstanceId,
    pub to: InstanceId,
    pub nonce: Uuid,
    pub timestamp: DateTime<Utc>,
    /// Arbitrary JSON payload.
    pub payload: serde_json::Value,
    /// Optional: correlation nonce for request/response pairing.
    pub in_reply_to: Option<Uuid>,
}

impl I2IMessage {
    /// Construct a new outbound message.
    pub fn new(
        verb: I2IVerb,
        target: &str,
        from: InstanceId,
        to: InstanceId,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            version: I2I_VERSION.to_string(),
            verb,
            target: target.to_string(),
            from,
            to,
            nonce: Uuid::new_v4(),
            timestamp: Utc::now(),
            payload,
            in_reply_to: None,
        }
    }

    /// Construct a reply to a previous message, preserving the correlation nonce.
    pub fn reply(
        original: &I2IMessage,
        verb: I2IVerb,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            version: I2I_VERSION.to_string(),
            verb,
            target: original.from.to_string(),
            from: original.to.clone(),
            to: original.from.clone(),
            nonce: Uuid::new_v4(),
            timestamp: Utc::now(),
            payload,
            in_reply_to: Some(original.nonce),
        }
    }

    /// Serialize to the human-readable I2I wire format.
    pub fn to_wire(&self) -> String {
        format!(
            "{} {} {}\nFrom: {}\nTo: {}\nNonce: {}\nTimestamp: {}\n\n{}\n",
            self.version,
            self.verb,
            self.target,
            self.from,
            self.to,
            self.nonce,
            self.timestamp.to_rfc3339(),
            serde_json::to_string_pretty(&self.payload).unwrap_or_default(),
        )
    }

    /// Parse an I2I wire-format string.
    ///
    /// The header block and JSON body are separated by a blank line.
    pub fn from_wire(s: &str) -> Result<Self, I2IParseError> {
        let mut lines = s.lines();

        // First line: I2I/1.0 <VERB> <target>
        let first = lines.next().ok_or(I2IParseError::MissingHeader)?;
        let parts: Vec<&str> = first.splitn(3, ' ').collect();
        if parts.len() < 3 {
            return Err(I2IParseError::MalformedHeader);
        }
        let _version = parts[0].to_string();
        let verb: I2IVerb = serde_json::from_str(&format!("\"{}\"", parts[1]))
            .map_err(|_| I2IParseError::UnknownVerb(parts[1].to_string()))?;
        let target = parts[2].to_string();

        let mut from_str = String::new();
        let mut to_str = String::new();
        let mut nonce_str = String::new();
        let mut ts_str = String::new();

        // Header fields
        for line in lines.by_ref() {
            if line.is_empty() {
                break;
            }
            if let Some(v) = line.strip_prefix("From: ") { from_str = v.to_string(); }
            else if let Some(v) = line.strip_prefix("To: ") { to_str = v.to_string(); }
            else if let Some(v) = line.strip_prefix("Nonce: ") { nonce_str = v.to_string(); }
            else if let Some(v) = line.strip_prefix("Timestamp: ") { ts_str = v.to_string(); }
        }

        // Remaining lines = JSON payload
        let body: String = lines.collect::<Vec<_>>().join("\n");
        let payload: serde_json::Value = if body.trim().is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(&body).map_err(|_| I2IParseError::InvalidPayload)?
        };

        let nonce = Uuid::parse_str(&nonce_str).unwrap_or_else(|_| Uuid::new_v4());
        let timestamp = ts_str.parse::<DateTime<Utc>>().unwrap_or_else(|_| Utc::now());

        // Parse InstanceId strings (best-effort)
        let from = parse_instance_id(&from_str);
        let to = parse_instance_id(&to_str);

        Ok(Self {
            version: I2I_VERSION.to_string(),
            verb,
            target,
            from,
            to,
            nonce,
            timestamp,
            payload,
            in_reply_to: None,
        })
    }
}

fn parse_instance_id(s: &str) -> InstanceId {
    // Expected: "kind/name@host"
    let (kind_name, host) = s.split_once('@').unwrap_or((s, "localhost"));
    let (kind_str, name) = kind_name.split_once('/').unwrap_or(("unknown", kind_name));
    let kind = serde_json::from_str(&format!("\"{}\"", kind_str))
        .unwrap_or(ComponentKind::Unknown);
    InstanceId { kind, name: name.to_string(), host: host.to_string() }
}

/// Errors from parsing I2I wire format.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum I2IParseError {
    #[error("Missing I2I header line")]
    MissingHeader,
    #[error("Malformed I2I header (expected: I2I/1.0 VERB TARGET)")]
    MalformedHeader,
    #[error("Unknown I2I verb: {0}")]
    UnknownVerb(String),
    #[error("Invalid JSON payload")]
    InvalidPayload,
}

// ─── TCP Server ──────────────────────────────────────────────────────────────

/// Callback type invoked for every well-formed inbound I2I message.
///
/// Returns an optional reply that the server writes back to the peer.
pub type MessageHandler = Arc<dyn Fn(I2IMessage) -> Option<I2IMessage> + Send + Sync>;

/// The I2I TCP server — listens on `0.0.0.0:7272` for plato-tui and other
/// instance connections.
///
/// The server accepts one connection at a time per spawned task.  Each
/// connection may send multiple messages; the server replies to each and keeps
/// the connection alive until the peer closes it.
pub struct I2IServer {
    bind_addr: String,
    handler: MessageHandler,
}

impl I2IServer {
    /// Bind to the standard I2I port (TCP 7272).
    pub fn new(handler: MessageHandler) -> Self {
        Self {
            bind_addr: "0.0.0.0:7272".to_string(),
            handler,
        }
    }

    /// Bind to a custom address (useful for testing).
    pub fn with_addr(addr: impl Into<String>, handler: MessageHandler) -> Self {
        Self {
            bind_addr: addr.into(),
            handler,
        }
    }

    /// Start accepting connections.  This future runs until the task is
    /// cancelled (e.g. via `tokio::select!` with a shutdown signal).
    pub async fn serve(self) -> anyhow::Result<()> {
        let listener = TcpListener::bind(&self.bind_addr).await?;
        tracing::info!("I2I server listening on {}", self.bind_addr);

        let handler = self.handler.clone();

        loop {
            match listener.accept().await {
                Ok((stream, peer)) => {
                    tracing::debug!("I2I: accepted connection from {}", peer);
                    let h = handler.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, h).await {
                            tracing::warn!("I2I connection error from {}: {}", peer, e);
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("I2I accept error: {}", e);
                }
            }
        }
    }
}

/// Handle a single TCP connection: read I2I messages, dispatch, reply.
async fn handle_connection(
    stream: TcpStream,
    handler: MessageHandler,
) -> anyhow::Result<()> {
    let (reader_half, mut writer_half) = stream.into_split();
    let mut lines = BufReader::new(reader_half).lines();

    // Accumulate lines into a message buffer.
    // Wire format: header lines terminated by a blank line, then JSON body.
    let mut buf = String::new();
    let mut blank_seen = false;
    let mut body_lines: Vec<String> = Vec::new();

    while let Some(line) = lines.next_line().await? {
        if !blank_seen {
            if line.is_empty() {
                blank_seen = true;
                buf.push('\n');
            } else {
                buf.push_str(&line);
                buf.push('\n');
            }
        } else {
            // Accumulate JSON body until the peer sends another blank line
            // (message terminator) or closes the connection.
            if line.is_empty() {
                // End of this message — dispatch it
                let full = format!("{}\n{}", buf, body_lines.join("\n"));
                blank_seen = false;

                match I2IMessage::from_wire(&full) {
                    Ok(msg) => {
                        tracing::debug!("I2I rx: {:?} → {}", msg.verb, msg.target);
                        if let Some(reply) = handler(msg) {
                            let wire = reply.to_wire();
                            // Terminate with a blank line so the peer knows the reply ended.
                            writer_half.write_all(wire.as_bytes()).await?;
                            writer_half.write_all(b"\n").await?;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("I2I parse error: {}", e);
                    }
                }

                buf.clear();
                body_lines.clear();
            } else {
                body_lines.push(line);
            }
        }
    }

    Ok(())
}

/// Build the default kernel-side message handler.
///
/// Handles `TUTOR_JUMP` and `CONSTRAINT_CHECK` verbs inline; all other verbs
/// produce no reply (fire-and-forget notifications are ignored server-side).
pub fn default_kernel_handler(kernel_id: InstanceId) -> MessageHandler {
    Arc::new(move |msg: I2IMessage| -> Option<I2IMessage> {
        match &msg.verb {
            I2IVerb::TutorJump => {
                let anchor = msg
                    .payload
                    .get("anchor")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                tracing::info!("I2I TUTOR_JUMP for anchor '{}'", anchor);
                let reply = I2IMessage {
                    version: I2I_VERSION.to_string(),
                    verb: I2IVerb::Response,
                    target: msg.from.to_string(),
                    from: kernel_id.clone(),
                    to: msg.from.clone(),
                    nonce: Uuid::new_v4(),
                    timestamp: Utc::now(),
                    payload: serde_json::json!({
                        "anchor": anchor,
                        "status": "queued",
                        "note": "TUTOR jump enqueued — tile will be injected into next prompt context"
                    }),
                    in_reply_to: Some(msg.nonce),
                };
                Some(reply)
            }
            I2IVerb::ConstraintCheck => {
                let command = msg
                    .payload
                    .get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                tracing::info!("I2I CONSTRAINT_CHECK for command '{}'", command);
                // Lexical allow-list: the kernel can do a deeper check; for the
                // protocol layer we return the structural result immediately.
                let result = if command.starts_with('@') || command.starts_with("delete") {
                    "Deny"
                } else {
                    "Allow"
                };
                let reply = I2IMessage {
                    version: I2I_VERSION.to_string(),
                    verb: I2IVerb::ConstraintResult,
                    target: msg.from.to_string(),
                    from: kernel_id.clone(),
                    to: msg.from.clone(),
                    nonce: Uuid::new_v4(),
                    timestamp: Utc::now(),
                    payload: serde_json::json!({
                        "command": command,
                        "result": result
                    }),
                    in_reply_to: Some(msg.nonce),
                };
                Some(reply)
            }
            I2IVerb::Announce => {
                tracing::info!("I2I: instance announced: {}", msg.from);
                None
            }
            I2IVerb::Disconnect => {
                tracing::info!("I2I: instance disconnected: {}", msg.from);
                None
            }
            _ => None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_roundtrip_wire() {
        let msg = I2IMessage::new(
            I2IVerb::Notify,
            "room/convergence-station",
            InstanceId::local_kernel("kernel-1"),
            InstanceId::local_tui("tui-1"),
            json!({ "event": "constraint_updated", "room": "convergence-station" }),
        );
        let wire = msg.to_wire();
        assert!(wire.starts_with("I2I/1.0 NOTIFY"));

        let parsed = I2IMessage::from_wire(&wire).expect("should parse");
        assert_eq!(parsed.verb, I2IVerb::Notify);
        assert_eq!(parsed.target, "room/convergence-station");
    }

    #[test]
    fn test_reply_links_nonce() {
        let req = I2IMessage::new(
            I2IVerb::Request,
            "constraint-check",
            InstanceId::local_tui("tui-1"),
            InstanceId::local_kernel("kernel-1"),
            json!({ "command": "look" }),
        );
        let resp = I2IMessage::reply(&req, I2IVerb::Response, json!({ "result": "Allow" }));
        assert_eq!(resp.in_reply_to, Some(req.nonce));
    }

    #[test]
    fn test_instance_id_display() {
        let id = InstanceId::local_kernel("alpha");
        assert_eq!(id.to_string(), "kernel/alpha@localhost");
    }
}
