use std::time::Duration;

/// Debounce for crash-recovery journal writes. Keystrokes only reschedule a
/// timer; the journal is snapshotted after the quiet period and written on a
/// background thread. Crash losses are bounded to this window (documented
/// 1-2s).
pub const RECOVERY_DEBOUNCE: Duration = Duration::from_millis(1500);

/// Debounce shared by label-index reloads and autocomplete. Both read a
/// single editor snapshot after the quiet period; parsing and completion run
/// on the background executor where possible.
pub const ASSIST_DEBOUNCE: Duration = Duration::from_millis(150);

/// True when the editor revision advanced since the last sync, meaning the
/// text may have changed. Cursor-only activity leaves the revision untouched
/// and must not trigger clones, fsync, or parses.
pub fn should_sync_editor_text(editor_rev: u64, last_synced_rev: u64) -> bool {
    editor_rev != last_synced_rev
}

/// Borrowed comparison used before any clone. Returns true only when the
/// document buffer actually differs from the editor snapshot.
pub fn doc_needs_update(doc_content: &str, editor_text: &str) -> bool {
    doc_content != editor_text
}

/// True when assist output for `scheduled_rev` is still current. Debounced
/// assist tasks capture the revision at schedule time and reject stale
/// results when the editor advanced while they were sleeping or computing.
pub fn assist_result_is_current(scheduled_rev: u64, current_rev: u64) -> bool {
    scheduled_rev == current_rev
}

/// Recovery snapshots cover every dirty document, but they are keyed to the
/// active editor revision so a newer keystroke invalidates an older pending
/// flush. Only the latest quiet-period snapshot writes.
pub fn recovery_result_is_current(scheduled_rev: u64, current_rev: u64) -> bool {
    scheduled_rev == current_rev
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_gate_skips_cursor_only_activity() {
        assert!(!should_sync_editor_text(7, 7));
        assert!(should_sync_editor_text(8, 7));
        // `set_text` resets the buffer revision to 0; that still counts as a
        // change versus the previously synced revision.
        assert!(should_sync_editor_text(0, 41));
    }

    #[test]
    fn borrowed_compare_avoids_clone_when_unchanged() {
        assert!(!doc_needs_update("hello", "hello"));
        assert!(doc_needs_update("hello", "hello!"));
        assert!(doc_needs_update("", "x"));
    }

    #[test]
    fn assist_stale_results_are_rejected_by_revision() {
        assert!(assist_result_is_current(9, 9));
        assert!(!assist_result_is_current(9, 10));
        assert!(!assist_result_is_current(10, 9));
    }

    #[test]
    fn recovery_flush_is_current_only_for_latest_revision() {
        assert!(recovery_result_is_current(4, 4));
        assert!(!recovery_result_is_current(4, 5));
    }

    #[test]
    fn debounce_windows_match_documented_budgets() {
        assert_eq!(RECOVERY_DEBOUNCE, Duration::from_millis(1500));
        assert!(RECOVERY_DEBOUNCE >= Duration::from_millis(1000));
        assert!(RECOVERY_DEBOUNCE <= Duration::from_millis(2000));
        assert_eq!(ASSIST_DEBOUNCE, Duration::from_millis(150));
    }
}
