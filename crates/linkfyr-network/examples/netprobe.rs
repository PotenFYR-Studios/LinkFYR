fn main() {
    let ifs = netdev::get_interfaces();
    println!("count: {}", ifs.len());
    for i in ifs.iter().take(5) {
        println!(
            "{} index={} up={} kind={:?}",
            i.name,
            i.index,
            i.is_up(),
            i.if_type
        );
    }
    match netdev::get_default_interface() {
        Ok(d) => println!("default: {}", d.name),
        Err(_) => println!("default: none"),
    }
}
