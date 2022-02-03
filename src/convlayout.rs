mod util;
mod layout;

use std::path::Path;

fn main() {
    let n = std::env::args().nth(1).unwrap();
    layout::translate(Path::new(&format!("env/res/{n}.xlf")),
                      Path::new(&format!("env/res/{n}.xlf.html"))).unwrap();
}
