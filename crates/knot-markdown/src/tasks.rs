//! Extract GFM checklist items from a markdown source. Used by the
//! workspace todo view's indexer.
//!
//! A task item's assignee is detected when the item's first inline content
//! is a link whose href matches the mention sentinel `knot://user/<uuid>`
//! (see `MentionExtension` on the editor side).

use chrono::{DateTime, Utc};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use uuid::Uuid;

const USER_HREF_PREFIX: &str = "knot://user/";
const TIME_HREF_PREFIX: &str = "knot://time/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub item_index: i32,
    pub text: String,
    pub assignee_user_id: Option<Uuid>,
    pub checked: bool,
    /// "Due by" timestamp lifted from an inline `knot://time/<iso>` link
    /// in the task content that's preceded by an explicit "by" or "due"
    /// cue. Bare datetimes are ignored to avoid misclassifying things
    /// like "meeting at 3pm".
    pub due_at: Option<DateTime<Utc>>,
}

/// Walk the markdown source and return one `Task` per `- [ ]` / `- [x]`
/// item, in document order. `item_index` is zero-based and forms a stable
/// id together with the doc id.
pub fn extract_tasks(md: &str) -> Vec<Task> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);

    let mut out: Vec<Task> = Vec::new();
    let mut current: Option<TaskState> = None;
    let mut item_index: i32 = 0;
    let mut link_depth: u32 = 0;
    let mut pending_assignee: Option<Uuid> = None;
    // Track whether we've seen the *first* link inside the current item's
    // opening text — only the first counts as the assignee.
    let mut first_link_consumed = false;
    // When inside a `knot://time/` link that was promoted to a due date,
    // suppress its display text from the task body just like we do for
    // mention chips.
    let mut inside_due_link = false;

    for ev in Parser::new_ext(md, opts) {
        match ev {
            Event::Start(Tag::Item) => {
                current = Some(TaskState::default());
                first_link_consumed = false;
                pending_assignee = None;
                inside_due_link = false;
            }
            Event::TaskListMarker(checked) => {
                if let Some(s) = current.as_mut() {
                    s.is_task = true;
                    s.checked = checked;
                }
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                link_depth = link_depth.saturating_add(1);
                // Capture only the first link of a task item as the assignee
                // candidate, and only if no inline text has been seen yet.
                if let Some(s) = current.as_ref()
                    && s.is_task
                    && !first_link_consumed
                    && s.text.trim().is_empty()
                    && let Some(rest) = dest_url.strip_prefix(USER_HREF_PREFIX)
                    && let Ok(id) = Uuid::parse_str(rest.trim_end_matches('/'))
                {
                    pending_assignee = Some(id);
                }
                // Promote a knot://time link to the task's due_at when it
                // follows an explicit "by"/"due" cue in the preceding text.
                // Whether or not we capture (only the first cued chip
                // wins), suppress *every* knot://time chip's display
                // text — the chip is presentation, not body content.
                if let Some(s) = current.as_mut()
                    && s.is_task
                    && let Some(rest) = dest_url.strip_prefix(TIME_HREF_PREFIX)
                {
                    inside_due_link = true;
                    if s.due_at.is_none()
                        && trailing_due_cue(&s.text)
                        && let Ok(ts) = DateTime::parse_from_rfc3339(rest.trim_end_matches('/'))
                    {
                        s.due_at = Some(ts.with_timezone(&Utc));
                    }
                }
            }
            Event::End(TagEnd::Link) => {
                link_depth = link_depth.saturating_sub(1);
                if link_depth == 0 && !first_link_consumed {
                    first_link_consumed = true;
                    if let Some(s) = current.as_mut() {
                        s.assignee_user_id = pending_assignee.take();
                    }
                }
                if link_depth == 0 {
                    inside_due_link = false;
                }
            }
            Event::Text(t) | Event::Code(t) => {
                if let Some(s) = current.as_mut() {
                    // Suppress text events that are the display content of a
                    // pending mention link — once promoted to an assignee
                    // the `@DisplayName` chip is metadata, not task body.
                    if pending_assignee.is_some() && link_depth > 0 {
                        continue;
                    }
                    // Same treatment for the due-date chip's display text.
                    if inside_due_link && link_depth > 0 {
                        continue;
                    }
                    if !s.text.is_empty() && !s.text.ends_with(' ') {
                        s.text.push(' ');
                    }
                    s.text.push_str(&t);
                }
            }
            Event::End(TagEnd::Item) => {
                if let Some(s) = current.take()
                    && s.is_task
                {
                    let text = s.text.trim().to_string();
                    out.push(Task {
                        item_index,
                        text,
                        assignee_user_id: s.assignee_user_id,
                        checked: s.checked,
                        due_at: s.due_at,
                    });
                    item_index += 1;
                }
                pending_assignee = None;
                inside_due_link = false;
            }
            _ => {}
        }
    }
    out
}

#[derive(Default)]
struct TaskState {
    is_task: bool,
    checked: bool,
    assignee_user_id: Option<Uuid>,
    due_at: Option<DateTime<Utc>>,
    text: String,
}

/// Returns true when `s` (trimmed of trailing whitespace) ends with a
/// word that signals an upcoming due-date — "by" or "due", case
/// insensitive. Bytewise compare so we don't pull in a regex crate.
fn trailing_due_cue(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    let tail = &bytes[..end];
    // The cue word must be preceded by start-of-string or whitespace so
    // we don't trip on "ruby" / "subdue".
    let starts_word = |start: usize| start == 0 || tail[start - 1].is_ascii_whitespace();
    let lower = |b: u8| b.to_ascii_lowercase();
    if tail.len() >= 2 {
        let start = tail.len() - 2;
        if starts_word(start)
            && lower(tail[start]) == b'b'
            && lower(tail[start + 1]) == b'y'
        {
            return true;
        }
    }
    if tail.len() >= 3 {
        let start = tail.len() - 3;
        if starts_word(start)
            && lower(tail[start]) == b'd'
            && lower(tail[start + 1]) == b'u'
            && lower(tail[start + 2]) == b'e'
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_simple_unchecked() {
        let md = "- [ ] Buy milk\n";
        let got = extract_tasks(md);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].text, "Buy milk");
        assert!(!got[0].checked);
        assert_eq!(got[0].assignee_user_id, None);
    }

    #[test]
    fn extract_checked_and_unchecked() {
        let md = "- [ ] open\n- [x] done\n- [ ] another\n";
        let got = extract_tasks(md);
        assert_eq!(got.len(), 3);
        assert!(!got[0].checked);
        assert!(got[1].checked);
        assert_eq!(got[1].text, "done");
    }

    #[test]
    fn extract_skips_plain_bullets() {
        let md = "- regular\n- [ ] task\n";
        let got = extract_tasks(md);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].text, "task");
    }

    #[test]
    fn extract_assignee_from_leading_mention() {
        let uid = Uuid::new_v4();
        let md = format!("- [ ] [@Alice](knot://user/{uid}) Buy milk\n");
        let got = extract_tasks(&md);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].assignee_user_id, Some(uid));
        // The mention's display text is metadata, not task body — only the
        // remainder lands in `text`.
        assert_eq!(got[0].text, "Buy milk");
    }

    #[test]
    fn extract_ignores_mid_item_mention() {
        let uid = Uuid::new_v4();
        let md = format!("- [ ] Buy milk [@Alice](knot://user/{uid})\n");
        let got = extract_tasks(&md);
        assert_eq!(got.len(), 1);
        // Mention is not at the start → no assignee captured.
        assert_eq!(got[0].assignee_user_id, None);
    }

    #[test]
    fn extract_due_at_with_by_cue() {
        let md = "- [ ] Ship the report by [Jun 4](knot://time/2026-06-04T17:00:00Z)\n";
        let got = extract_tasks(md);
        assert_eq!(got.len(), 1);
        let ts = got[0].due_at.expect("due_at should be Some");
        assert_eq!(ts.to_rfc3339(), "2026-06-04T17:00:00+00:00");
        // The chip's display text is metadata, not body.
        assert_eq!(got[0].text, "Ship the report by");
    }

    #[test]
    fn extract_due_at_with_due_cue_case_insensitive() {
        let md = "- [ ] Review pr DUE [tomorrow](knot://time/2026-06-05T09:00:00Z)\n";
        let got = extract_tasks(md);
        assert_eq!(got.len(), 1);
        assert!(got[0].due_at.is_some());
    }

    #[test]
    fn extract_ignores_bare_datetime_without_cue() {
        let md = "- [ ] Meeting at [3pm](knot://time/2026-06-04T15:00:00Z)\n";
        let got = extract_tasks(md);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].due_at, None);
    }

    #[test]
    fn extract_suppresses_second_time_chip_even_without_cue() {
        // First chip with cue → captured as due_at. Second chip without
        // a cue → its display text is still suppressed so it doesn't
        // leak into the task body.
        let md = "- [ ] Ship by [Jun 4](knot://time/2026-06-04T17:00:00Z) and review at [3pm](knot://time/2026-06-04T15:00:00Z)\n";
        let got = extract_tasks(md);
        assert_eq!(got.len(), 1);
        assert!(got[0].due_at.is_some());
        // Neither chip's display text leaks; pulldown's surrounding-space
        // events still concatenate to a double space, which is fine for
        // the body's purposes (the extractor doesn't normalise further).
        assert!(!got[0].text.contains("Jun 4"));
        assert!(!got[0].text.contains("3pm"));
        assert!(got[0].text.contains("Ship by"));
        assert!(got[0].text.contains("review at"));
    }

    #[test]
    fn extract_ignores_due_in_middle_of_word() {
        // "subdue" ends in "due" but the cue must be a standalone word.
        let md = "- [ ] subdue [link](knot://time/2026-06-04T15:00:00Z)\n";
        let got = extract_tasks(md);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].due_at, None);
    }

    #[test]
    fn item_index_advances_per_task_only() {
        let md = "- [ ] a\n- regular\n- [x] b\n";
        let got = extract_tasks(md);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].item_index, 0);
        assert_eq!(got[1].item_index, 1);
    }
}
