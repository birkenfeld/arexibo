mod util;
mod layout;

use std::path::Path;
use std::time::Instant;

fn main() {
    let start = Instant::now();
    let n = std::env::args().nth(1).unwrap();
    let xl = layout::Translator::new(Path::new(&format!("env/res/{n}.xlf")),
                                     Path::new(&format!("env/res/{n}.xlf.html"))).unwrap();
    xl.translate().unwrap();
    println!("{:?}", start.elapsed());
}
