//! Round-trip serializer between the canonical ProseMirror schema and Markdown.
//!
//! Targets the v0.1 schema only (paragraphs, headings, blockquotes, code
//! blocks, lists, horizontal rules, hard breaks, and standard marks).

pub mod schema;

// to_markdown / from_markdown land in T7-T10.
