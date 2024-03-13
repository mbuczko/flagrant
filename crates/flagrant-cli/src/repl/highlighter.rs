use std::borrow::Cow::{self, Borrowed};

use flagrant::models::environment::Environment;
use rustyline::highlight::Highlighter;

pub struct PromptHighlighter {
    env: Option<Environment>
}

impl PromptHighlighter {
    pub fn new() -> PromptHighlighter {
        Self { env: None }
    }
}

impl<'a> Highlighter for PromptHighlighter {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        default: bool,
    ) -> Cow<'b, str> {
        let _ = default;
        Borrowed(prompt)
    }
}
