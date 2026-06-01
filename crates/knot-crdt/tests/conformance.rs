use knot_crdt::{Engine, YrsEngine};

#[test]
fn empty_doc_has_state_vector() {
    let e = YrsEngine;
    let doc = e.new_doc();
    let sv = e.encode_state_vector(&doc).expect("encode SV");
    // The SV of a fresh doc may be empty (no clients have updated yet);
    // we just require that the call succeeds and returns a Vec.
    let _ = sv;
}

#[test]
fn apply_update_is_idempotent() {
    let e = YrsEngine;
    let src = e.new_doc();
    let full = e.encode_state_as_update(&src, None).expect("full state");

    let dst = e.new_doc();
    e.apply_update(&dst, &full).expect("first apply");
    let sv1 = e.encode_state_vector(&dst).expect("sv1");
    e.apply_update(&dst, &full).expect("second apply");
    let sv2 = e.encode_state_vector(&dst).expect("sv2");
    assert_eq!(
        sv1, sv2,
        "state vector must not change under idempotent re-apply"
    );
}

#[test]
fn two_doc_sync_converges() {
    let e = YrsEngine;
    let a = e.new_doc();
    let b = e.new_doc();

    // a → b
    let sv_b = e.encode_state_vector(&b).unwrap();
    let upd_for_b = e.encode_state_as_update(&a, Some(&sv_b)).unwrap();
    e.apply_update(&b, &upd_for_b).unwrap();

    // b → a
    let sv_a = e.encode_state_vector(&a).unwrap();
    let upd_for_a = e.encode_state_as_update(&b, Some(&sv_a)).unwrap();
    e.apply_update(&a, &upd_for_a).unwrap();

    assert_eq!(
        e.encode_state_vector(&a).unwrap(),
        e.encode_state_vector(&b).unwrap(),
        "post-sync state vectors must agree"
    );
}
