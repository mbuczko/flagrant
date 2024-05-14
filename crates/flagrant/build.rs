use std::{path::Path, time::Duration};
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebounceEventResult};

fn main() {
    // Select recommended watcher for debouncer.
    // Using a callback here, could also be a channel.
    let mut debouncer = new_debouncer(Duration::from_secs(1), |res: DebounceEventResult| {
        match res {
            Ok(events) => events.iter().for_each(|_event| {
                std::process::Command::new("cargo")
                    .arg("build")
                    .status()
                    .expect("Failed to execute command");
            }),
            Err(e) => println!("Error {:?}",e),
        }
    }).unwrap();

    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    debouncer.watcher().watch(Path::new("resources"), RecursiveMode::Recursive).unwrap();

}
