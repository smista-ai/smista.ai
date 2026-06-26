//! Reference to one sealable content record, used as a crypto map key.
//!
//! The end-to-end-encryption maps the protocol carries
//! (`to_encrypt`/`to_decrypt` on a [`TurnResponse`](super::TurnResponse), and the
//! `encrypted`/`plaintext` maps on a [`ContinueRequest`](super::ContinueRequest))
//! are keyed by a [`ContentRef`] rather than a free-form string, so the router
//! knows exactly which content store to dispatch each payload to.
//!
//! A [`ContentRef`] pairs a content kind with the record id within that kind's
//! store. It serializes to, and parses from, the compact `kind:id` form — for
//! example `message:0194abcd` — so it can be a JSON object key while staying a
//! typed value in Rust. The same form is produced by [`Display`] and parsed by
//! [`FromStr`].
//!
//! # Examples
//!
//! ```
//! use std::str::FromStr;
//!
//! use smista_core::api::ContentRef;
//!
//! let reference = ContentRef::from_str("message:0194abcd").unwrap();
//! assert_eq!(reference, ContentRef::Message("0194abcd".to_string()));
//! assert_eq!(reference.to_string(), "message:0194abcd");
//! ```

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{CoreError, ParseError};

/// A reference to one sealable content record, by its store and id.
///
/// The textual form is `kind:id`, where `kind` names the content store the
/// record's body lives in. Each variant maps to one of the session-scoped
/// `_content` stores (and the trace payload store) that end-to-end encryption
/// seals.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(export))]
#[cfg_attr(feature = "ts", ts(type = "string"))]
pub enum ContentRef {
    /// A session message body (`session_message_content`).
    Message(String),
    /// Tool-call arguments, result or error (`session_tool_call_content`).
    ToolCall(String),
    /// A produced diff body (`session_diff_content`).
    Diff(String),
    /// A plan snapshot (`session_plan_content`).
    Plan(String),
    /// A per-session memory fact (`context_memory_content`).
    Memory(String),
    /// A trace event payload (`trace_event_content`).
    Trace(String),
    /// The run's request-context bundle (`session_run_input_content`).
    RunInput(String),
}

/// Separator between the kind tag and the record id in the textual form.
const SEPARATOR: char = ':';

impl ContentRef {
    /// The stable kind tag used in the textual form.
    #[must_use]
    pub fn kind_tag(&self) -> &'static str {
        match self {
            Self::Message(_) => "message",
            Self::ToolCall(_) => "tool_call",
            Self::Diff(_) => "diff",
            Self::Plan(_) => "plan",
            Self::Memory(_) => "memory",
            Self::Trace(_) => "trace",
            Self::RunInput(_) => "run_input",
        }
    }

    /// The record id within the kind's store.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Message(id)
            | Self::ToolCall(id)
            | Self::Diff(id)
            | Self::Plan(id)
            | Self::Memory(id)
            | Self::Trace(id)
            | Self::RunInput(id) => id,
        }
    }
}

impl Display for ContentRef {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}{SEPARATOR}{}", self.kind_tag(), self.id())
    }
}

impl FromStr for ContentRef {
    type Err = CoreError;

    /// Parses the `kind:id` form, as produced by [`Display`].
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::InvalidContentRef`] if `s` lacks a `:` separator,
    /// has an empty id, or names an unknown content kind.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((kind, id)) = s.split_once(SEPARATOR) else {
            return Err(ParseError::InvalidContentRef(s.to_string()).into());
        };
        if id.is_empty() {
            return Err(ParseError::InvalidContentRef(s.to_string()).into());
        }
        let id = id.to_string();
        match kind {
            "message" => Ok(Self::Message(id)),
            "tool_call" => Ok(Self::ToolCall(id)),
            "diff" => Ok(Self::Diff(id)),
            "plan" => Ok(Self::Plan(id)),
            "memory" => Ok(Self::Memory(id)),
            "trace" => Ok(Self::Trace(id)),
            "run_input" => Ok(Self::RunInput(id)),
            _ => Err(ParseError::InvalidContentRef(s.to_string()).into()),
        }
    }
}

impl Serialize for ContentRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ContentRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ContentRefVisitor;

        impl Visitor<'_> for ContentRefVisitor {
            type Value = ContentRef;

            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("a content reference in the form `kind:id`")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                value.parse().map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_str(ContentRefVisitor)
    }
}

#[cfg(feature = "openapi")]
impl utoipa::PartialSchema for ContentRef {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::SchemaType::new(
                utoipa::openapi::schema::Type::String,
            ))
            .description(Some(
                "A content reference in the form `kind:id`, where `kind` is one of \
                 `message`, `tool_call`, `diff`, `plan`, `memory`, `trace`, \
                 `run_input`.",
            ))
            .into()
    }
}

#[cfg(feature = "openapi")]
impl utoipa::ToSchema for ContentRef {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("ContentRef")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_display_as_kind_colon_id() {
        assert_eq!(
            ContentRef::ToolCall("abc".to_string()).to_string(),
            "tool_call:abc"
        );
    }

    #[test]
    fn should_roundtrip_every_kind() {
        let refs = [
            ContentRef::Message("m".to_string()),
            ContentRef::ToolCall("t".to_string()),
            ContentRef::Diff("d".to_string()),
            ContentRef::Plan("p".to_string()),
            ContentRef::Memory("mem".to_string()),
            ContentRef::Trace("tr".to_string()),
            ContentRef::RunInput("ri".to_string()),
        ];
        for reference in refs {
            assert_eq!(ContentRef::from_str(&reference.to_string()), Ok(reference));
        }
    }

    #[test]
    fn should_serialize_as_a_bare_string() {
        assert_eq!(
            serde_json::to_string(&ContentRef::Message("0194".to_string())).unwrap(),
            "\"message:0194\""
        );
    }

    #[test]
    fn should_deserialize_from_a_bare_string() {
        assert_eq!(
            serde_json::from_str::<ContentRef>("\"plan:0195\"").unwrap(),
            ContentRef::Plan("0195".to_string())
        );
    }

    #[test]
    fn should_serialize_as_a_map_key() {
        let mut map = std::collections::BTreeMap::new();
        map.insert(ContentRef::Message("0194".to_string()), "body".to_string());
        let value = serde_json::to_value(&map).unwrap();
        assert_eq!(value["message:0194"], "body");
        assert_eq!(
            serde_json::from_value::<std::collections::BTreeMap<ContentRef, String>>(value)
                .unwrap(),
            map
        );
    }

    #[test]
    fn should_render_and_parse_run_input_ref() {
        let reference = ContentRef::RunInput("0194".to_string());
        assert_eq!(reference.kind_tag(), "run_input");
        let serialized = serde_json::to_string(&reference).unwrap();
        assert_eq!(serialized, "\"run_input:0194\"");
        assert_eq!(
            serde_json::from_str::<ContentRef>(&serialized).unwrap(),
            reference
        );
    }

    #[test]
    fn should_reject_missing_separator() {
        let err = ContentRef::from_str("message").unwrap_err();
        assert!(matches!(
            err,
            CoreError::Parse(ParseError::InvalidContentRef(_))
        ));
    }

    #[test]
    fn should_reject_unknown_kind() {
        assert!(ContentRef::from_str("widget:1").is_err());
    }

    #[test]
    fn should_reject_empty_id() {
        assert!(ContentRef::from_str("message:").is_err());
    }
}
