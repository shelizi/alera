use std::io::Write;

pub fn run() -> ! {
    let snapshot = alera_core::agent_descriptor::snapshot_dart::emit();
    std::io::stdout()
        .write_all(snapshot.as_bytes())
        .expect("failed to write agent descriptor snapshot");
    std::process::exit(0);
}
