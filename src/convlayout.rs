mod util;
mod layout;

use std::path::Path;
use std::time::Instant;

fn main() {
    let start = Instant::now();
    let n = std::env::args().nth(1).unwrap();
    layout::translate(Path::new(&format!("env/res/{n}.xlf")),
                      Path::new(&format!("env/res/{n}.xlf.html"))).unwrap();
    println!("{:?}", start.elapsed());
}
