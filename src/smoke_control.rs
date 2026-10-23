//! Test-build-only command source. Never included by the default desktop feature.
//! The smoke launcher owns stdin as a private pipe; no listener/socket/file is opened.
use crate::{event::Event, tray::TrayCommand};
use std::io::{BufRead, IsTerminal, Read};
use std::sync::mpsc::Sender;

fn parse(line: &[u8]) -> Option<TrayCommand> {
    match line {
        b"open-settings\n" => Some(TrayCommand::OpenSettings),
        b"rest-now\n" => Some(TrayCommand::RestNow),
        b"quit\n" => Some(TrayCommand::Quit),
        _ => None,
    }
}

pub fn start(tx: Sender<Event>) -> std::io::Result<()> {
    if !std::env::args_os().any(|arg| arg == "--smoke-stdin") {
        return Ok(());
    }
    if std::io::stdin().is_terminal() {
        return Err(std::io::Error::other(
            "Smoke control requires redirected stdin",
        ));
    }
    std::thread::Builder::new()
        .name("eyeflow-smoke-control".into())
        .spawn(move || {
            let stdin = std::io::stdin();
            let mut input = stdin.lock();
            loop {
                let mut line = Vec::new();
                // A malformed/oversized command terminates the channel, rather
                // than buffering arbitrary input or accepting a partial command.
                let read = input.by_ref().take(64).read_until(b'\n', &mut line);
                if !matches!(read, Ok(1..=63)) {
                    break;
                }
                let Some(command) = parse(&line) else { break };
                if tx.send(Event::SmokeCommand(command)).is_err() {
                    break;
                }
                crate::event::wake_ui();
                if command == TrayCommand::Quit {
                    break;
                }
            }
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explicit_complete_commands_are_accepted() {
        assert_eq!(parse(b"open-settings\n"), Some(TrayCommand::OpenSettings));
        assert_eq!(parse(b"rest-now\n"), Some(TrayCommand::RestNow));
        assert_eq!(parse(b"quit\n"), Some(TrayCommand::Quit));
        for invalid in [
            b"quit".as_slice(),
            b"quit\nrest-now\n",
            b"toggle-sound\n",
            b"QUIT\n",
        ] {
            assert_eq!(parse(invalid), None);
        }
    }
}
