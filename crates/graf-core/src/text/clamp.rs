/// Rounds `offset` down to the nearest UTF-8 character boundary of
/// `content`.
///
/// Completion is asked for a caret position expressed in UTF-16 offsets
/// (TextKit's unit) and walks the text as UTF-8, so a caret that lands inside
/// a multi-byte character has to move back to a boundary before the prefix
/// can be sliced.
///
/// Its own module because the `buffer.rs` it used to sit under is gone: Swift
/// owns the live text in one `NSTextStorage`, so no Rust buffer survives, and
/// this was the only part of it still needed.
pub(crate) fn clamp_str_boundary(content: &str, offset: usize) -> usize {
    let mut offset = offset.min(content.len());
    while offset > 0 && !content.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_offset_inside_the_content_is_returned_unchanged() {
        assert_eq!(clamp_str_boundary("abc", 2), 2);
    }

    #[test]
    fn an_offset_past_the_end_clamps_to_the_length() {
        assert_eq!(clamp_str_boundary("ab", 5), 2);
    }

    #[test]
    fn an_offset_inside_a_multibyte_character_moves_back_to_its_start() {
        // "é" is two bytes; a caret between them is not a valid slice point.
        assert_eq!(clamp_str_boundary("aé", 2), 1);
        // Four bytes of emoji, probed at every interior byte.
        let emoji = "🎯";
        for offset in 1..emoji.len() {
            assert_eq!(clamp_str_boundary(emoji, offset), 0);
        }
    }

    #[test]
    fn empty_content_clamps_to_zero() {
        assert_eq!(clamp_str_boundary("", 0), 0);
        assert_eq!(clamp_str_boundary("", 9), 0);
    }
}
