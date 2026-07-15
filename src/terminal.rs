pub(crate) use harbor_terminal::*;

use crate::pty::Pty;

pub(crate) fn send_paste(modes: InputModes, text: &str, pty: &mut Pty) {
    pty.write(&modes.paste(text.as_bytes()));
}
