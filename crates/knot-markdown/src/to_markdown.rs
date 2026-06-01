//! Walks the canonical "default" XmlFragment of a Y.Doc and emits Markdown.

use knot_crdt::{DocHandle, YrsEngine};
use thiserror::Error;
use yrs::{GetString, ReadTxn, Transact, Xml, XmlElementRef, XmlFragment, XmlOut};

#[derive(Debug, Error)]
pub enum SerError {
    #[error("yrs read: {0}")]
    Yrs(String),
    #[error("unsupported node: {0}")]
    UnsupportedNode(String),
}

pub fn serialise(_engine: &YrsEngine, doc: &DocHandle) -> Result<String, SerError> {
    let yrs_doc = doc.inner();
    let txn = yrs_doc.transact();

    let frag = match txn.get_xml_fragment("default") {
        Some(f) => f,
        None => return Ok("\n".to_string()),
    };

    let mut buf = String::new();
    let len = frag.len(&txn);
    for i in 0..len {
        let child = frag
            .get(&txn, i)
            .ok_or_else(|| SerError::Yrs("child missing".into()))?;
        write_block(&mut buf, &txn, &child)?;
        if i + 1 < len {
            buf.push('\n');
        }
    }
    if !buf.ends_with('\n') {
        buf.push('\n');
    }
    Ok(buf)
}

fn write_block<T: ReadTxn>(buf: &mut String, txn: &T, node: &XmlOut) -> Result<(), SerError> {
    let el = match node {
        XmlOut::Element(el) => el,
        _ => {
            return Err(SerError::UnsupportedNode(
                "non-element at block level".into(),
            ));
        }
    };
    let tag = el.tag().to_string();
    match tag.as_str() {
        "paragraph" => {
            write_inlines(buf, txn, el)?;
            buf.push('\n');
        }
        "heading" => {
            let level: u8 = el
                .get_attribute(txn, "level")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1)
                .clamp(1, 6);
            for _ in 0..level {
                buf.push('#');
            }
            buf.push(' ');
            write_inlines(buf, txn, el)?;
            buf.push('\n');
        }
        other => return Err(SerError::UnsupportedNode(other.into())),
    }
    Ok(())
}

fn write_inlines<T: ReadTxn>(
    buf: &mut String,
    txn: &T,
    parent: &XmlElementRef,
) -> Result<(), SerError> {
    let len = parent.len(txn);
    for i in 0..len {
        let child = parent
            .get(txn, i)
            .ok_or_else(|| SerError::Yrs("inline missing".into()))?;
        match child {
            XmlOut::Text(t) => {
                let s = t.get_string(txn);
                buf.push_str(&s);
            }
            XmlOut::Element(el) => {
                let tag = el.tag().to_string();
                return Err(SerError::UnsupportedNode(format!("inline element: {tag}")));
            }
            XmlOut::Fragment(_) => {
                return Err(SerError::UnsupportedNode("inline fragment".into()));
            }
        }
    }
    Ok(())
}
