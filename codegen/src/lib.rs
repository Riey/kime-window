use quick_xml::events::Event;
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

pub fn gen_emoji(emoji_out: &Path, annotation_path: &Path) -> quick_xml::Result<()> {
    let emoji_out = fs::File::create(emoji_out)?;
    let mut emoji_out = BufWriter::new(emoji_out);

    let annotation = fs::File::open(annotation_path)?;
    let mut annotation_map: BTreeMap<String, String> = BTreeMap::new();

    let mut r = quick_xml::Reader::from_reader(BufReader::new(annotation));
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
            write!(emoji_out, "\\u{{{:x}}}", ch as u32)?;
        }
        writeln!(emoji_out, "\", \"{}\"),", text)?;
    }

    writeln!(emoji_out, "];",)?;

    Ok(())
}
