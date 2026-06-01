//! Markdown round-trip suite over fixtures.
//!
//! Each fixture file's contents are compared exactly. Tests construct a
//! Y.Doc with a small builder, serialize to Markdown via `to_markdown`,
//! and assert byte-equality.

use std::{fs, path::PathBuf};

use knot_crdt::{DocHandle, Engine, YrsEngine};
use yrs::{Transact, Xml, XmlElementPrelim, XmlFragment, XmlTextPrelim};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture(name: &str) -> String {
    let path = fixtures_dir().join(name);
    let mut s =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// Builder helper. Each method appends to the "default" XmlFragment.
struct DocBuilder {
    engine: YrsEngine,
    doc: DocHandle,
}

impl DocBuilder {
    fn new() -> Self {
        let engine = YrsEngine;
        let doc = engine.new_doc();
        Self { engine, doc }
    }

    fn paragraph(self, text: &str) -> Self {
        let yrs_doc = self.doc.inner();
        let frag = yrs_doc.get_or_insert_xml_fragment("default");
        let mut txn = yrs_doc.transact_mut();
        let p = frag.push_back(&mut txn, XmlElementPrelim::empty("paragraph"));
        p.push_back(&mut txn, XmlTextPrelim::new(text));
        drop(txn);
        self
    }

    fn heading(self, level: u8, text: &str) -> Self {
        let yrs_doc = self.doc.inner();
        let frag = yrs_doc.get_or_insert_xml_fragment("default");
        let mut txn = yrs_doc.transact_mut();
        let h = frag.push_back(&mut txn, XmlElementPrelim::empty("heading"));
        h.insert_attribute(&mut txn, "level", level.to_string());
        h.push_back(&mut txn, XmlTextPrelim::new(text));
        drop(txn);
        self
    }

    fn to_markdown(&self) -> String {
        knot_markdown::to_markdown::serialise(&self.engine, &self.doc).expect("serialise")
    }
}

#[test]
fn paragraph_fixture() {
    let got = DocBuilder::new()
        .paragraph("hello world")
        .paragraph("second line")
        .to_markdown();
    assert_eq!(got, fixture("paragraph.md"));
}

#[test]
fn heading_fixture() {
    let got = DocBuilder::new()
        .heading(1, "one")
        .heading(2, "two")
        .heading(6, "six")
        .to_markdown();
    assert_eq!(got, fixture("headings.md"));
}
