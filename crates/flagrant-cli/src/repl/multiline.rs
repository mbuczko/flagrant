use rustyline::{Cmd, EventHandler, KeyCode, KeyEvent, Modifiers};

use super::readline::ReplEditor;

pub fn multiline_value(editor: &mut ReplEditor) -> anyhow::Result<String> {
    editor.bind_sequence(
        KeyEvent(KeyCode::Enter, Modifiers::NONE),
        EventHandler::Simple(Cmd::Newline),
    );
    editor.bind_sequence(
        KeyEvent(KeyCode::Char('d'), Modifiers::CTRL),
        EventHandler::Simple(Cmd::AcceptLine),
    );

    println!("--- Editing value. Press CTRL-D to finish ---");

    let value = editor.readline("")?;

    // restore default behaviour of Enter and CTRL-D keys
    editor.bind_sequence(
        KeyEvent(KeyCode::Enter, Modifiers::NONE),
        EventHandler::Simple(Cmd::AcceptLine),
    );
    editor.bind_sequence(
        KeyEvent(KeyCode::Char('d'), Modifiers::CTRL),
        EventHandler::Simple(Cmd::EndOfFile),
    );
    Ok(value)
}
