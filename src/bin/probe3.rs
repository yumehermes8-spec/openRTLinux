fn main() {
    let mut c = righttype::linuxos::capture::Capture::open().expect("open");
    println!("{}", c.describe());
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_secs(14) {
        for k in c.poll() {
            println!("key code={} value={} shift={} caps={}", k.code, k.value, k.mods.shift, k.mods.caps);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    println!("probe done");
}
