//! Walks the canonical "default" XmlFragment of a Y.Doc and emits Markdown.

use knot_crdt::{DocHandle, YrsEngine};
use thiserror::Error;
use yrs::{GetString, ReadTxn, Transact, Xml, XmlElementRef, XmlFragment};

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

fn write_block<T: ReadTxn>(buf: &mut String, txn: &T, node: &yrs::XmlOut) -> Result<(), SerError> {
    use yrs::XmlOut;
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
        "blockquote" => {
            let len = el.len(txn);
            for i in 0..len {
                let child = el
                    .get(txn, i)
                    .ok_or_else(|| SerError::Yrs("bq child missing".into()))?;
                let mut inner = String::new();
                write_block(&mut inner, txn, &child)?;
                for line in inner.trim_end_matches('\n').split('\n') {
                    if line.is_empty() {
                        buf.push_str(">\n");
                    } else {
                        buf.push_str("> ");
                        buf.push_str(line);
                        buf.push('\n');
                    }
                }
            }
        }
        "code_block" => {
            let lang = el.get_attribute(txn, "language").unwrap_or_default();
            buf.push_str("```");
            buf.push_str(&lang);
            buf.push('\n');
            let len = el.len(txn);
            for i in 0..len {
                if let Some(XmlOut::Text(t)) = el.get(txn, i) {
                    let s = t.get_string(txn);
                    buf.push_str(&s);
                }
            }
            buf.push_str("\n```\n");
        }
        "horizontal_rule" => {
            buf.push_str("---\n");
        }
        "bullet_list" => {
            let len = el.len(txn);
            for i in 0..len {
                let item = el
                    .get(txn, i)
                    .ok_or_else(|| SerError::Yrs("li missing".into()))?;
                let XmlOut::Element(item_el) = item else {
                    continue;
                };
                write_list_item(buf, txn, &item_el, "- ")?;
            }
        }
        "ordered_list" => {
            let mut idx: u64 = el
                .get_attribute(txn, "start")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);
            let len = el.len(txn);
            for i in 0..len {
                let item = el
                    .get(txn, i)
                    .ok_or_else(|| SerError::Yrs("li missing".into()))?;
                let XmlOut::Element(item_el) = item else {
                    continue;
                };
                let prefix = format!("{idx}. ");
                write_list_item(buf, txn, &item_el, &prefix)?;
                idx += 1;
            }
        }
        other => return Err(SerError::UnsupportedNode(other.into())),
    }
    Ok(())
}

fn write_list_item<T: ReadTxn>(
    buf: &mut String,
    txn: &T,
    item: &yrs::XmlElementRef,
    prefix: &str,
) -> Result<(), SerError> {
    let pad: String = " ".repeat(prefix.chars().count());
    let len = item.len(txn);
    for i in 0..len {
        let child = item
            .get(txn, i)
            .ok_or_else(|| SerError::Yrs("li body missing".into()))?;
        let mut inner = String::new();
        write_block(&mut inner, txn, &child)?;
        for (j, line) in inner.trim_end_matches('\n').split('\n').enumerate() {
            if i == 0 && j == 0 {
                buf.push_str(prefix);
            } else {
                buf.push_str(&pad);
            }
            buf.push_str(line);
            buf.push('\n');
        }
    }
    Ok(())
}

fn write_inlines<T: ReadTxn>(
    buf: &mut String,
    txn: &T,
    parent: &XmlElementRef,
) -> Result<(), SerError> {
    use yrs::XmlOut;
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
                let tag = el.tag().as_ref();
                match tag {
                    "hard_break" => buf.push_str("  \n"),
                    other => return Err(SerError::UnsupportedNode(format!("inline {other}"))),
                }
            }
            _ => return Err(SerError::UnsupportedNode("inline".into())),
        }
    }
    Ok(())
}
