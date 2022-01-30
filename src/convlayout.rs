mod util;
mod layout;

use std::path::Path;

fn main() {
    layout::translate(Path::new("env/res/84.xlf"), Path::new("env/res/84.xlf.html")).unwrap();
}
