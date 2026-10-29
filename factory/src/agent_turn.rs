//! How an agent's last turn ended, as Factory's agent client reads it.
//!
//! Factory drives an agent through AIKit's encounter (`native_owner.rs`
//! `AikitEncounter`: send, delivery, cancel, status, read). AIKit reports how a
//! turn stopped as its host saw it on the wire — `TurnStop`:
//! `Completed { stop_reason }`, `Cancelled`, `OperationalLimit { max_signals }`
//! or `Failed { reason }` (`aikit-adapters/src/agent_session_host.rs:98-104`) —
//! in the session's status (`last_turn.stop`) and in a delivery's `detail`.
//! This module names the cause and says whether a retry can clear it.
//!
//! Ported from Grok Bot's turn runtime (xAI's desktop agent client on
//! Anysphere's agent stack, bundle `com.anysphere.sand`; read at 0.70.2, its
//! `app.asar` sha256 `8dd7b6c4…4cf7`). Its error registry, shipped in
//! `dist/electron-main/main-core.cjs` (sha256 `3f39655c…7ef3a`), names every way a
//! turn fails, whether a retry helps, and the predicate that decides it
//! (`turn-runtime.ts` `classifyAgentError`, `transient-stream-error.ts`):
//!
//! | Grok Bot | cause here | a retry |
//! |---|---|---|
//! | SAND-E0401 providerOverloaded (`isProviderCapacityError`) | provider-overloaded | clears it |
//! | SAND-E0408 backendCapacityDeferred (server-paced retry-after) | capacity-deferred | clears it, after the pace |
//! | SAND-E0402 firstTokenStall (`isFirstTokenStallError`) | first-token-stall | clears it |
//! | SAND-E0403 streamReset (`isTransientStreamError`) | stream-reset | clears it |
//! | SAND-E0415 streamIdleStall (`isStreamIdleError`) | stream-idle-stall | clears it |
//! | SAND-E0418 turnBackendUnreachable (`isBackendUnreachableError`) | backend-unreachable | clears it |
//! | SAND-E0404 contextWindowOverflow (`isContextOverflowDeadEnd`), SAND-E0414 conversationTooLarge | context-overflow | repeats it |
//! | SAND-E0405 backendRejected (terminal structured error) | backend-rejected | repeats it |
//! | SAND-E0407 agentUnclassified (the `classifyAgentError` fallback) | unclassified | repeats it |
//!
//! Its rule is kept whole: only a named retryable cause offers a retry, and a
//! failure outside every named shape is terminal — a retry is never offered on
//! a guess. A server-paced deferral carries its pace: Grok Bot's runner sleeps
//! it; Factory runs nothing, so it says the pace and the retry waits for it.
//!
//! A turn that completed may still have stopped short. Its stop reasons are the
//! Agent Client Protocol's (`end_turn`, `max_tokens`, `max_turn_requests`,
//! `refusal`, `cancelled`; Buzz's harness parses the same five,
//! `StopReason_from_str`), with Pi's `stop` and `length`; a reason no table
//! names is the provider's own word for a completed turn.
//!
//! Decoded in O-I `campaign-evidence/2026-10-08-grok-bot-re/` (the registry's
//! turn rows), with Buzz's stop reasons in
//! `campaign-evidence/2026-10-07-buzz-grok-agent-messaging-re/CONTRACT.md`.

use serde::Serialize;
use serde_json::Value;

/// Why a turn ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TurnCause {
    Completed,
    Cancelled,
    OutputCap,
    TurnCap,
    Refusal,
    OperationalLimit,
    ProviderOverloaded,
    CapacityDeferred,
    FirstTokenStall,
    StreamReset,
    StreamIdleStall,
    BackendUnreachable,
    ContextOverflow,
    BackendRejected,
    Unclassified,
}

impl TurnCause {
    /// Whether a retry of the same turn can clear this cause.
    pub fn retryable(self) -> bool {
        matches!(
            self,
            Self::ProviderOverloaded
                | Self::CapacityDeferred
                | Self::FirstTokenStall
                | Self::StreamReset
                | Self::StreamIdleStall
                | Self::BackendUnreachable
        )
    }

    /// The turn did what was asked of it, or someone stopped it: nothing failed.
    pub fn settled(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }

    /// Where the rule comes from.
    fn source(self) -> &'static str {
        match self {
            Self::Completed => "acp:end_turn",
            Self::Cancelled => "acp:cancelled",
            Self::OutputCap => "acp:max_tokens",
            Self::TurnCap => "acp:max_turn_requests",
            Self::Refusal => "acp:refusal",
            Self::OperationalLimit => "aikit:operational-limit",
            Self::ProviderOverloaded => "grok-bot:SAND-E0401",
            Self::CapacityDeferred => "grok-bot:SAND-E0408",
            Self::FirstTokenStall => "grok-bot:SAND-E0402",
            Self::StreamReset => "grok-bot:SAND-E0403",
            Self::StreamIdleStall => "grok-bot:SAND-E0415",
            Self::BackendUnreachable => "grok-bot:SAND-E0418",
            Self::ContextOverflow => "grok-bot:SAND-E0404",
            Self::BackendRejected => "grok-bot:SAND-E0405",
            Self::Unclassified => "grok-bot:SAND-E0407",
        }
    }

    /// What happened, in words.
    fn words(self) -> &'static str {
        match self {
            Self::Completed => "the turn completed",
            Self::Cancelled => "the turn was stopped",
            Self::OutputCap => "the reply was cut at the model's output limit",
            Self::TurnCap => "the turn used every model request it was allowed",
            Self::Refusal => "the model refused the request",
            Self::OperationalLimit => "AIKit stopped the turn at its own bound",
            Self::ProviderOverloaded => "the model provider was overloaded",
            Self::CapacityDeferred => "the model provider deferred the turn at capacity",
            Self::FirstTokenStall => "the model sent nothing before its first-token deadline",
            Self::StreamReset => "the turn's stream dropped on a transport reset",
            Self::StreamIdleStall => "the turn's stream went silent mid-reply",
            Self::BackendUnreachable => "the model provider could not be reached",
            Self::ContextOverflow => "the conversation outgrew the model's context window",
            Self::BackendRejected => "the model provider rejected the request",
            Self::Unclassified => "the turn failed in a way no rule names",
        }
    }

    /// The next step a retry cannot take.
    fn instead(self) -> &'static str {
        match self {
            Self::OutputCap | Self::TurnCap => "send it the next step instead",
            Self::Refusal => "change what the task asks",
            Self::OperationalLimit => "send it the next step, or raise AIKit's bound",
            Self::ContextOverflow => "start a fresh session for the task",
            Self::BackendRejected => "fix the provider's credentials or the request",
            _ => "read the session's thread for what failed",
        }
    }
}

/// How one turn ended, with the owner's words kept beside the cause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TurnOutcome {
    pub cause: TurnCause,
    /// A retry of the same turn can clear the cause.
    pub retryable: bool,
    /// The provider's pace for that retry, when it named one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
    /// The rule that decided it: a Grok Bot registry code, an ACP stop reason,
    /// or AIKit's own bound.
    pub rule: &'static str,
    /// How AIKit reported the stop, verbatim.
    pub stop: Value,
}

impl TurnOutcome {
    /// The turn failed or stopped short of what it was asked.
    pub fn failed(&self) -> bool {
        !self.cause.settled()
    }

    /// What happened and what a retry does about it, in one sentence.
    pub fn words(&self) -> String {
        let detail = owner_detail(&self.stop)
            .map(|detail| format!(" ({detail})"))
            .unwrap_or_default();
        if self.retryable {
            let pace = self
                .retry_after_ms
                .map(|ms| format!(", after the {} s the provider asked for", ms.div_ceil(1000)))
                .unwrap_or_default();
            format!(
                "The last turn failed: {}{detail}; a retry can clear it{pace}.",
                self.cause.words()
            )
        } else {
            format!(
                "The last turn ended: {}{detail}; a retry would repeat it, so {}.",
                self.cause.words(),
                self.cause.instead()
            )
        }
    }
}

/// The owner's own words for a stop: a failure's reason, a completed turn's
/// stop reason, AIKit's bound.
fn owner_detail(stop: &Value) -> Option<String> {
    if let Some(reason) = stop.pointer("/Failed/reason").and_then(Value::as_str) {
        return Some(bounded(reason));
    }
    if let Some(reason) = stop
        .pointer("/Completed/stop_reason")
        .and_then(Value::as_str)
    {
        return Some(format!("stop reason {reason}"));
    }
    stop.pointer("/OperationalLimit/max_signals")
        .and_then(Value::as_u64)
        .map(|max| format!("{max} signals"))
}

fn bounded(text: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    if line.chars().count() > 160 {
        format!("{}…", line.chars().take(159).collect::<String>())
    } else {
        line.to_owned()
    }
}

/// How a turn ended, from AIKit's `TurnStop` as its status or a delivery's
/// `detail` carries it (the value, or that value as JSON text). `None` when it
/// is not a stop AIKit reports.
pub fn classify(stop: &Value) -> Option<TurnOutcome> {
    let stop = match stop {
        Value::String(text) if text != "Cancelled" => serde_json::from_str(text).ok()?,
        other => other.clone(),
    };
    let (cause, retry_after_ms) = if stop == "Cancelled" {
        (TurnCause::Cancelled, None)
    } else if let Some(reason) = stop
        .pointer("/Completed/stop_reason")
        .and_then(Value::as_str)
    {
        (completed(reason), None)
    } else if stop.pointer("/OperationalLimit").is_some() {
        (TurnCause::OperationalLimit, None)
    } else {
        failure(stop.pointer("/Failed/reason").and_then(Value::as_str)?)
    };
    Some(TurnOutcome {
        cause,
        retryable: cause.retryable(),
        retry_after_ms,
        rule: cause.source(),
        stop,
    })
}

/// A completed turn's stop reason (ACP's five, Pi's `stop` and `length`).
fn completed(reason: &str) -> TurnCause {
    match reason
        .to_ascii_lowercase()
        .replace(['-', ' '], "_")
        .as_str()
    {
        "max_tokens" | "maxtokens" | "length" => TurnCause::OutputCap,
        "max_turn_requests" | "maxturnrequests" => TurnCause::TurnCap,
        "refusal" => TurnCause::Refusal,
        "cancelled" | "canceled" | "aborted" => TurnCause::Cancelled,
        _ => TurnCause::Completed,
    }
}

/// A failure's reason, as Grok Bot's runtime classifies the same shapes. The
/// order is the order its predicates are asked in: a dead end before a
/// transient, a capacity error before a structured rejection.
fn failure(reason: &str) -> (TurnCause, Option<u64>) {
    let text = reason.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|needle| text.contains(needle));
    let tokens: Vec<&str> = text
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|token| !token.is_empty())
        .collect();
    let code = |codes: &[&str]| codes.iter().any(|code| tokens.contains(code));
    if has(&[
        "context window",
        "context_window",
        "context length",
        "context_length",
        "prompt is too long",
        "input is too long",
        "maximum context",
        "too many tokens",
        "exceeds the context",
        "conversation too large",
        "conversation is too large",
    ]) {
        return (TurnCause::ContextOverflow, None);
    }
    if has(&[
        "overloaded",
        "rate limit",
        "rate_limit",
        "ratelimit",
        "too many requests",
        "capacity",
        "resource_exhausted",
    ]) || code(&["429", "529"])
    {
        return match retry_after_ms(&text) {
            Some(pace) => (TurnCause::CapacityDeferred, Some(pace)),
            None => (TurnCause::ProviderOverloaded, None),
        };
    }
    if has(&[
        "first token",
        "first_token",
        "before any output",
        "no response within",
    ]) {
        return (TurnCause::FirstTokenStall, None);
    }
    if has(&[
        "stream idle",
        "idle timeout",
        "went silent",
        "stopped streaming",
        "stream stalled",
    ]) {
        return (TurnCause::StreamIdleStall, None);
    }
    if has(&[
        "econnreset",
        "socket hang up",
        "epipe",
        "broken pipe",
        "connection reset",
        "stream reset",
        "premature close",
        "unexpected eof",
        "stream closed",
    ]) {
        return (TurnCause::StreamReset, None);
    }
    if has(&[
        "econnrefused",
        "enotfound",
        "eai_again",
        "getaddrinfo",
        "connection refused",
        "could not resolve",
        "network is unreachable",
        "host unreachable",
    ]) {
        return (TurnCause::BackendUnreachable, None);
    }
    if has(&[
        "invalid_request",
        "invalid request",
        "unauthorized",
        "forbidden",
        "authentication",
        "permission denied",
        "invalid api key",
        "invalid_api_key",
    ]) || code(&["400", "401", "403", "404", "422"])
    {
        return (TurnCause::BackendRejected, None);
    }
    (TurnCause::Unclassified, None)
}

/// The pace a provider named for its retry: `retry-after: 30` and
/// `retry after 30s` are seconds (HTTP's `Retry-After`), `retry_after_ms: 1500`
/// and a number followed by `ms` are milliseconds.
fn retry_after_ms(text: &str) -> Option<u64> {
    let at = [
        "retry-after",
        "retry after",
        "retry_after",
        "retryafter",
        "retry in",
    ]
    .iter()
    .filter_map(|marker| text.find(marker).map(|at| at + marker.len()))
    .min()?;
    let rest = &text[at..];
    let millis_key = rest.starts_with("_ms") || rest.starts_with("ms");
    let start = rest.find(|c: char| c.is_ascii_digit())?;
    if start > 12 {
        return None;
    }
    let digits: String = rest[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    let value: u64 = digits.parse().ok()?;
    let unit = rest[start + digits.len()..].trim_start();
    if millis_key || unit.starts_with("ms") {
        Some(value)
    } else {
        value.checked_mul(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn failed(reason: &str) -> TurnOutcome {
        classify(&json!({"Failed": {"reason": reason}})).unwrap()
    }

    #[test]
    fn each_failure_takes_grok_bots_cause_and_only_a_named_transient_offers_a_retry() {
        let cases = [
            (
                "529 overloaded_error: Overloaded",
                TurnCause::ProviderOverloaded,
                true,
            ),
            (
                "HTTP 429 Too Many Requests",
                TurnCause::ProviderOverloaded,
                true,
            ),
            (
                "rate limit reached; retry-after: 30",
                TurnCause::CapacityDeferred,
                true,
            ),
            (
                "timed out waiting for the first token",
                TurnCause::FirstTokenStall,
                true,
            ),
            ("read ECONNRESET", TurnCause::StreamReset, true),
            ("socket hang up", TurnCause::StreamReset, true),
            (
                "stream idle timeout after output",
                TurnCause::StreamIdleStall,
                true,
            ),
            (
                "getaddrinfo ENOTFOUND api.example",
                TurnCause::BackendUnreachable,
                true,
            ),
            (
                "connect ECONNREFUSED 127.0.0.1:443",
                TurnCause::BackendUnreachable,
                true,
            ),
            (
                "400 invalid_request_error: prompt is too long: 210000 tokens",
                TurnCause::ContextOverflow,
                false,
            ),
            ("context_length_exceeded", TurnCause::ContextOverflow, false),
            (
                "401 Unauthorized: invalid api key",
                TurnCause::BackendRejected,
                false,
            ),
            ("Native provider failure", TurnCause::Unclassified, false),
            (
                "Pi settled without a terminal assistant result",
                TurnCause::Unclassified,
                false,
            ),
        ];
        for (reason, cause, retry) in cases {
            let outcome = failed(reason);
            assert_eq!(outcome.cause, cause, "{reason}");
            assert_eq!(outcome.retryable, retry, "{reason}");
            assert!(outcome.failed(), "{reason}");
        }
        assert_eq!(
            failed("rate limit reached; retry-after: 30").retry_after_ms,
            Some(30_000)
        );
        assert_eq!(
            failed("overloaded; retry_after_ms: 1500").retry_after_ms,
            Some(1_500)
        );
        assert_eq!(failed("Overloaded").retry_after_ms, None);
        assert_eq!(
            failed("Native provider failure").rule,
            "grok-bot:SAND-E0407"
        );
    }

    #[test]
    fn a_completed_turn_may_have_stopped_short_and_a_cancel_is_not_a_failure() {
        let completed =
            |reason: &str| classify(&json!({"Completed": {"stop_reason": reason}})).unwrap();
        assert_eq!(completed("end_turn").cause, TurnCause::Completed);
        assert_eq!(completed("stop").cause, TurnCause::Completed);
        assert!(!completed("end_turn").failed());
        assert_eq!(completed("max_tokens").cause, TurnCause::OutputCap);
        assert_eq!(completed("length").cause, TurnCause::OutputCap);
        assert_eq!(completed("max_turn_requests").cause, TurnCause::TurnCap);
        assert_eq!(completed("refusal").cause, TurnCause::Refusal);
        assert!(completed("refusal").failed() && !completed("refusal").retryable);
        let cancelled = classify(&json!("Cancelled")).unwrap();
        assert_eq!(cancelled.cause, TurnCause::Cancelled);
        assert!(!cancelled.failed());
        let bound = classify(&json!({"OperationalLimit": {"max_signals": 4096}})).unwrap();
        assert_eq!(bound.cause, TurnCause::OperationalLimit);
        assert!(bound.words().contains("4096 signals"), "{}", bound.words());
        // A delivery's detail carries the same stop as JSON text.
        let detail = classify(&json!("{\"Failed\":{\"reason\":\"read ECONNRESET\"}}")).unwrap();
        assert_eq!(detail.cause, TurnCause::StreamReset);
        assert!(classify(&json!({"Unknown": {}})).is_none());
    }

    #[test]
    fn the_words_say_what_happened_and_what_a_retry_does_about_it() {
        let paced = failed("rate limit reached; retry-after: 30");
        assert_eq!(
            paced.words(),
            "The last turn failed: the model provider deferred the turn at capacity (rate limit reached; retry-after: 30); a retry can clear it, after the 30 s the provider asked for."
        );
        let dead_end = failed("prompt is too long");
        assert_eq!(
            dead_end.words(),
            "The last turn ended: the conversation outgrew the model's context window (prompt is too long); a retry would repeat it, so start a fresh session for the task."
        );
    }
}
