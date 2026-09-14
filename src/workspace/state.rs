//! Pure-logic home for the workspace: the index and naming arithmetic runs
//! without GPUI so it can be unit-tested directly.

/// Index of the active tab after removing `removed_idx` from a list that had
/// `len_before` tabs. Mirrors `force_close_tab`.
pub(crate) fn active_index_after_close(
    active_idx: usize,
    removed_idx: usize,
    len_before: usize,
) -> usize {
    let len_after = len_before.saturating_sub(1);
    if removed_idx < active_idx {
        active_idx.saturating_sub(1)
    } else if active_idx >= len_after {
        // The removed tab was at/after the active one (or last): clamp onto
        // the surviving tail.
        len_after.saturating_sub(1)
    } else {
        active_idx
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DraftKind {
    Typst,
    Diagram,
}

impl DraftKind {
    fn basename(self) -> &'static str {
        match self {
            Self::Typst => "document",
            Self::Diagram => "diagram",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Typst => "typ",
            Self::Diagram => "graf",
        }
    }
}

pub(crate) fn next_draft_title(kind: DraftKind, existing: &[String]) -> String {
    let occupied = |name: &str| existing.iter().any(|title| title == name);
    let mut counter = 1;
    loop {
        let candidate = format!("{}-{counter}.{}", kind.basename(), kind.extension());
        if !occupied(&candidate) {
            return candidate;
        }
        counter += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closing_before_the_active_tab_shifts_the_active_index() {
        assert_eq!(active_index_after_close(3, 0, 5), 2);
    }

    #[test]
    fn closing_after_the_active_tab_keeps_the_active_index() {
        assert_eq!(active_index_after_close(1, 3, 5), 1);
    }

    #[test]
    fn closing_the_last_tab_activates_the_new_last() {
        assert_eq!(active_index_after_close(4, 4, 5), 3);
    }

    #[test]
    fn closing_the_only_remaining_tab_clamps_to_zero() {
        assert_eq!(active_index_after_close(0, 0, 1), 0);
    }

    #[test]
    fn draft_titles_survive_tab_churn() {
        let mut titles: Vec<String> = vec!["document-1.typ".into()];
        let first = next_draft_title(DraftKind::Typst, &titles);
        assert_eq!(first, "document-2.typ");
        titles.push(first.clone());

        // The user closes "document-1" and "document-3" appeared meanwhile;
        // reuse of "document-1" is fine, but a fresh title must be free.
        titles.push("document-3.typ".into());
        titles.retain(|t| t != "document-1.typ");
        let next = next_draft_title(DraftKind::Typst, &titles);
        assert_ne!(next, "document-3.typ");
        assert!(!titles.contains(&next));
    }

    #[test]
    fn draft_titles_start_at_one_and_use_the_kind_extension() {
        assert_eq!(next_draft_title(DraftKind::Typst, &[]), "document-1.typ");
        assert!(next_draft_title(DraftKind::Diagram, &[]).ends_with(".graf"));
    }
}

/// Which persona the shared prompt editor serves at the moment; entering a
/// persona is explicit, so routing in `on_prompt_changed` keys off this
/// state instead of sniffing `active_modal` matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PromptTarget {
    #[default]
    Idle,
    Find,
    QuickOpen,
    Palette,
}

#[cfg(test)]
mod prompt_target_tests {
    use super::*;

    #[test]
    fn default_is_idle() {
        assert_eq!(PromptTarget::default(), PromptTarget::Idle);
        assert_ne!(PromptTarget::Idle, PromptTarget::Find);
        assert_ne!(PromptTarget::QuickOpen, PromptTarget::Palette);
    }
}
