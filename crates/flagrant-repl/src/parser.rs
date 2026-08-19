use std::cmp::min;
use std::str::CharIndices;

use super::command::Arg;

fn whitespace(ch: char) -> bool {
    ch == ' ' || ch == '\r' || ch == '\n' || ch == '\t'
}

/// Consumes from `chars` up to and including the `close` bracket matching the `open`
/// bracket already consumed at the start of the token, tracking nesting depth so an inner
/// `[`/`{` doesn't end the scan early. Whitespace within is literal content, not an
/// argument separator - this is what lets a raw JSON value like `["dev", "staging"]` be
/// typed as a single argument without wrapping the whole thing in `"..."`. Quoted-string
/// contents (including escaped quotes) are skipped over as a unit so a bracket or
/// whitespace inside one (e.g. `["a b"]`) doesn't affect nesting depth either.
///
/// Returns the end index of the bracketed span (exclusive). If the closing bracket is
/// never found, consumes to the end of input - same "unterminated" fallback as a quoted
/// argument missing its closing `"`.
fn scan_bracketed(chars: &mut CharIndices, input: &str, open: char, close: char) -> usize {
    let mut depth = 1;
    while let Some((p, c)) = chars.next() {
        match c {
            '"' => {
                while let Some((_, c2)) = chars.next() {
                    if c2 == '\\' {
                        chars.next();
                    } else if c2 == '"' {
                        break;
                    }
                }
            }
            c if c == open => depth += 1,
            c if c == close => {
                depth -= 1;
                if depth == 0 {
                    return p + c.len_utf8();
                }
            }
            _ => {}
        }
    }
    input.len()
}

pub fn split_command_line(input: &str) -> anyhow::Result<Vec<Arg<'_>>> {
    let mut chars = input.char_indices();
    let mut output = Vec::new();
    let mut start: Option<usize> = None;

    while let Some((pos, ch)) = chars.next() {
        match ch {
            b if whitespace(b) => {
                if let Some(chunk_start) = start {
                    output.push(Arg(&input[chunk_start..pos], chunk_start));
                    start = None;
                } else {
                    continue;
                }
            }
            // Only treat `"` as starting a quoted argument at the start of a token - not
            // when it appears mid-token (e.g. a JSON array like ["dev","staging"] typed
            // with no surrounding spaces), where it should just be a literal character.
            // Previously this branch fired unconditionally, silently discarding whatever
            // of the current token had already been accumulated in `start`.
            '"' if start.is_none() => loop {
                if let Some((p, ch)) = chars.next() {
                    if ch == '"' {
                        output.push(Arg(&input[pos + 1..p], pos + 1));
                        start = None;
                        break;
                    }
                } else {
                    // Quotation not ended
                    output.push(Arg(&input[pos + 1..], pos + 1));
                    break;
                }
            },
            // A JSON array/object value (e.g. the `in`/`not_in` rule comparator's value)
            // also stays a single argument even with internal whitespace, same as a quoted
            // one - only when the bracket starts a fresh token, same reasoning as `"` above.
            '[' if start.is_none() => {
                let end = scan_bracketed(&mut chars, input, '[', ']');
                output.push(Arg(&input[pos..end], pos));
            }
            '{' if start.is_none() => {
                let end = scan_bracketed(&mut chars, input, '{', '}');
                output.push(Arg(&input[pos..end], pos));
            }
            _ => {
                if start.is_none() {
                    start = Some(pos);
                }
            }
        }
    }
    if let Some(start_chunk) = start {
        output.push(Arg(&input[start_chunk..], start_chunk));
    }
    Ok(output)
}

/// For given position in command line returns corresponding argument
/// and offset from the start of argument.
pub fn find_arg_by_position(args: &[Arg], pos: usize) -> (usize, usize) {
    let (_, arg_n, arg_offset) = args.iter().fold((0, 0, 0), |(acc, n, len), str| {
        if pos >= acc {
            (acc + str.len() + 1, n + 1, min(pos - acc, str.len()))
        } else {
            (acc, n, len)
        }
    });
    ((arg_n as usize).saturating_sub(1), arg_offset)
}

#[cfg(test)]
mod tests {
    use std::vec;

    use super::*;

    #[test]
    fn split_line() {
        assert_eq!(
            split_command_line("\"foo bar").unwrap(),
            vec![Arg("foo bar", 1)]
        );
        assert_eq!(
            split_command_line("FOO \"foo bar").unwrap(),
            vec![Arg("FOO", 0), Arg("foo bar", 5)]
        );
    }
    #[test]
    fn embedded_quotes_stay_part_of_the_token() {
        // A JSON array typed with no surrounding whitespace (e.g. as the `in`/`not_in`
        // rule value) must stay a single token - `"` should only start a quoted argument
        // at the start of a token, not mid-token.
        assert_eq!(
            split_command_line(r#"RULE add g1 environment in ["dev","staging"]"#).unwrap(),
            vec![
                Arg("RULE", 0),
                Arg("add", 5),
                Arg("g1", 9),
                Arg("environment", 12),
                Arg("in", 24),
                Arg(r#"["dev","staging"]"#, 27),
            ]
        );
    }
    #[test]
    fn bracketed_value_with_internal_whitespace_stays_one_token() {
        assert_eq!(
            split_command_line(r#"RULE add g1 environment in ["dev", "staging"]"#).unwrap(),
            vec![
                Arg("RULE", 0),
                Arg("add", 5),
                Arg("g1", 9),
                Arg("environment", 12),
                Arg("in", 24),
                Arg(r#"["dev", "staging"]"#, 27),
            ]
        );
    }
    #[test]
    fn nested_brackets_and_quoted_strings_inside_are_handled() {
        // A nested array, and a quoted string containing both a space and a stray closing
        // bracket, must not end the scan early.
        assert_eq!(
            split_command_line(r#"{"a": ["b c]", [1,2]]}"#).unwrap(),
            vec![Arg(r#"{"a": ["b c]", [1,2]]}"#, 0)]
        );
    }
    #[test]
    fn unterminated_bracket_consumes_to_end_of_input() {
        assert_eq!(
            split_command_line("RULE add g1 environment in [\"dev\"").unwrap(),
            vec![
                Arg("RULE", 0),
                Arg("add", 5),
                Arg("g1", 9),
                Arg("environment", 12),
                Arg("in", 24),
                Arg(r#"["dev""#, 27),
            ]
        );
    }
    #[test]
    fn arg_position() {
        // "foo bar bazz"
        let args = vec![Arg("foo", 0), Arg("bar", 4), Arg("bazz", 8)];
        assert_eq!(find_arg_by_position(&args, 2), (0, 2));
        assert_eq!(find_arg_by_position(&args, 3), (0, 3));
        assert_eq!(find_arg_by_position(&args, 4), (1, 0));
        assert_eq!(find_arg_by_position(&args, 5), (1, 1));
        assert_eq!(find_arg_by_position(&args, 7), (1, 3));
        assert_eq!(find_arg_by_position(&args, 8), (2, 0));
        assert_eq!(find_arg_by_position(&args, 11), (2, 3));
        assert_eq!(find_arg_by_position(&args, 30), (2, 4));
    }
}
