use ahash::AHashMap;
use quick_xml::events::Event;
use std::fs;
use std::io::{BufReader, BufWriter, Write};

fn gen_emoji() -> quick_xml::Result<()> {
    let emoji_out = fs::File::create(concat!(env!("CARGO_MANIFEST_DIR"), "/../src/emoji.rs"))?;
    let mut emoji_out = BufWriter::new(emoji_out);

    let en_annotation = fs::File::open("/usr/share/unicode/cldr/common/annotations/en.xml")?;
    let mut annotation_map: AHashMap<String, String> = AHashMap::new();

    let mut r = quick_xml::Reader::from_reader(BufReader::new(en_annotation));
    let mut buf = Vec::with_capacity(256);
    let mut buf2 = Vec::with_capacity(256);

    loop {
        let e = r.read_event(&mut buf)?;

        match e {
            Event::Start(ref e) if e.name() == b"annotation" => {
                let mut text = r.read_text(b"annotation", &mut buf2)?;

                let cp_attr = e
                    .attributes()
                    .find(|a| a.as_ref().map_or(false, |a| a.key == b"cp"))
                    .unwrap()?;

                let value = cp_attr.unescape_and_decode_value(&r)?;

                let d = annotation_map.entry(value).or_default();

                if !d.is_empty() {
                    text.reserve(d.len() + 3);
                    text.push_str(" (");
                    text.push_str(d);
                    text.push_str(")");
                }

                *d = text;

                buf2.clear();
            }
            Event::Eof => break,
            _ => {}
        }

        buf.clear();
    }

    writeln!(
        emoji_out,
        "pub static EMOJIS: [(&str, &str); {}] = [\n",
        annotation_map.len()
    )?;

    for (cp, text) in annotation_map.iter() {
        write!(emoji_out, "(\"")?;
        for ch in cp.chars() {
            if ch == '\\' {
                write!(emoji_out, "\\\\")?;
            } else {
                write!(emoji_out, "\\u{{{:x}}}", ch as u32)?;
            }
        }
        writeln!(emoji_out, "\", \"{}\"),", text)?;
    }

    writeln!(emoji_out, "];",)?;

    Ok(())
}

fn main() {
    gen_emoji().expect("gen_emoji");
}