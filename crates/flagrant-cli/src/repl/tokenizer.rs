fn whitespace(ch: char) -> bool {
    ch == ' ' || ch == '\r' || ch == '\n' || ch == '\t'
}

pub fn split_command_line(input: &str) -> anyhow::Result<Vec<&str>> {
    let mut chars = input.char_indices();
    let mut output = Vec::new();
    let mut start: Option<usize> = None;

    while let Some((pos, ch)) = chars.next() {
        match ch {
            b if whitespace(b) => {
                if let Some(chunk_start) = start {
                    output.push(&input[chunk_start..pos]);
                    start = None;
                } else {
                    continue;
                }
            }
            '"' => loop {
                if let Some((p, ch)) = chars.next() {
                    if ch == '"' {
                        output.push(&input[pos + 1..p]);
                        start = None;
                        break;
                    }
                } else {
                    // quotation not ended
                    output.push(&input[pos + 1..]);
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
        output.push(&input[start_chunk..]);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unquoted_string() {
        let result = split_command_line("Ala ma kotkę").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result.first(), Some(&"Ala"));
        assert_eq!(result.get(1), Some(&"ma"));
        assert_eq!(result.get(2), Some(&"kotkę"));
    }

    #[test]
    fn unquoted_string_with_whitechars() {
        let result = split_command_line("  Ala ma   kotkę  ").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result.first(), Some(&"Ala"));
        assert_eq!(result.get(1), Some(&"ma"));
        assert_eq!(result.get(2), Some(&"kotkę"));
    }

    #[test]
    fn quoted_string() {
        let result = split_command_line("Ala \"ma kota\" a kot ma Alę").unwrap();
        assert_eq!(result.len(), 6);
    }

    #[test]
    fn quoted_string_with_whitechars() {
        let result = split_command_line("Ala  \"  ma  kota \"   a kot ma Alę  ").unwrap();
        assert_eq!(result.len(), 6);
        assert_eq!(result.first(), Some(&"Ala"));
        assert_eq!(result.get(1), Some(&"  ma  kota "));
        assert_eq!(result.get(2), Some(&"a"));
        assert_eq!(result.get(3), Some(&"kot"));
        assert_eq!(result.get(4), Some(&"ma"));
        assert_eq!(result.get(5), Some(&"Alę"));
    }

    #[test]
    fn quoted_string_with_whitechars_and_utfs() {
        let result = split_command_line("Żółty ryś \"  ma  jaskrę\"   a kot ma Alę  ").unwrap();
        assert_eq!(result.len(), 7);
        assert_eq!(result.first(), Some(&"Żółty"));
        assert_eq!(result.get(1), Some(&"ryś"));
        assert_eq!(result.get(2), Some(&"  ma  jaskrę"));
        assert_eq!(result.get(3), Some(&"a"));
        assert_eq!(result.get(4), Some(&"kot"));
        assert_eq!(result.get(5), Some(&"ma"));
        assert_eq!(result.get(6), Some(&"Alę"));
    }

    #[test]
    fn open_ended_quotation() {
        let result = split_command_line("Żółty ryś \"  ma  jaskrę").unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result.first(), Some(&"Żółty"));
        assert_eq!(result.get(1), Some(&"ryś"));
        assert_eq!(result.get(2), Some(&"  ma  jaskrę"));
    }
}
