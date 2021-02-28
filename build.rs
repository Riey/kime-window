use std::env;
use std::path::PathBuf;

fn main() {
    let annotation_path = match env::var("KIME_WINDOW_ANNOTATION") {
        Ok(path) => PathBuf::from(path),
        _ => PathBuf::from("/usr/share/unicode/cldr/common/annotations/en.xml"),
    };

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("emoji_gen.rs");

    codegen::gen_emoji(&out_path, &annotation_path).unwrap();
}
