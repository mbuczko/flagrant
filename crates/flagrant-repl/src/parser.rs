use std::cmp::min;

use super::command::Arg;

fn whitespace(ch: char) -> bool {
    ch == ' ' || ch == '\r' || ch == '\n' || ch == '\t'
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
            '"' => loop {
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
