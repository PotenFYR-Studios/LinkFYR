use linkfyr_network::{InterfaceMonitor, OsMonitor};

fn main() {
    let m = OsMonitor::new();
    match m.snapshot() {
        Ok(list) => {
            println!("snapshot count: {}", list.len());
            for i in list.iter().take(5) {
                println!(
                    "{} status={:?} kind={:?}",
                    i.friendly_name, i.status, i.kind
                );
            }
        }
        Err(e) => println!("snapshot error: {e}"),
    }
    match m.counters() {
        Ok(c) => println!("counters count: {}", c.len()),
        Err(e) => println!("counters error: {e}"),
    }
}
