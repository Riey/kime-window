mod emoji;

use ahash::AHashMap;
use gtk::prelude::*;

use std::cell::Cell;
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::rc::Rc;
use std::time::Instant;

type Dict = AHashMap<&'static str, Vec<(&'static str, &'static str)>>;

fn spawn_window<
    'a,
    F: Fn(&str, &str) -> bool + 'static,
    I: Iterator<Item = (&'static str, &'static str)>,
>(
    window: &gtk::Window,
    entires: impl IntoIterator<IntoIter = I, Item = (&'static str, &'static str)>
        + std::iter::ExactSizeIterator,
    filter: F,
) -> Option<&'static str> {
    let ret = Rc::new(Cell::new(None));

    let window_box = gtk::Box::new(gtk::Orientation::Vertical, 5);

    let scroll = gtk::ScrolledWindowBuilder::new()
    .build();

    let lbox = gtk::ListBoxBuilder::new()
        .selection_mode(gtk::SelectionMode::Single)
        .activate_on_single_click(true)
        .build();

    let entry = gtk::SearchEntryBuilder::new().build();

    let mut ids = Vec::with_capacity(entires.len());

    for (value, description) in entires {
        let label_str = format!("{}: {}", value, description);
        let label = gtk::LabelBuilder::new().label(&label_str).build();
        let window1 = window.clone();
        let row = gtk::ListBoxRowBuilder::new().child(&label).build();
        let ret = Rc::downgrade(&ret);
        let id = row.connect_activate(move |_| {
            ret.upgrade().unwrap().set(Some(value));
            window1.hide();
            gtk::main_quit();
        });
        lbox.add(&row);
        ids.push((id, row));
    }

    let entry1 = entry.clone();
    lbox.set_filter_func(Some(Box::new(move |row: &gtk::ListBoxRow| {
        let label = row.get_child().unwrap().downcast::<gtk::Label>().unwrap();
        filter(label.get_text().as_str(), entry1.get_text().as_str())
    })));

    let lbox1 = lbox.clone();
    let search_changed = entry.connect_search_changed(move |_| {
        lbox1.invalidate_filter();
    });

    let deleted = window.connect_delete_event(|window, _| {
        window.hide();
        gtk::main_quit();
        gtk::Inhibit(true)
    });

    window_box.add(&entry);
    window_box.add(&lbox);
    scroll.add(&window_box);
    window.add(&scroll);
    window.show_all();

    gtk::main();

    window.remove(&scroll);

    glib::signal_handler_disconnect(window, deleted);
    glib::signal_handler_disconnect(&entry, search_changed);

    for (id, row) in ids {
        glib::signal_handler_disconnect(&row, id);
    }

    ret.take()
}

fn load_hanja_dict() -> Dict {
    include_flate::flate!(static HANJA_DATA: str from "hanja/hanja.txt");

    let mut dict = Dict::new();

    for line in HANJA_DATA.lines() {
        if line.starts_with('#') {
            continue;
        }

        let mut parts = line.split(':');

        match parts.next().and_then(|hangul| {
            parts
                .next()
                .and_then(|hanja| parts.next().map(|description| (hangul, hanja, description)))
        }) {
            Some((hangul, hanja, description)) => {
                // skip unused hanja
                if description.is_empty() {
                    continue;
                }

                dict.entry(hangul).or_default().push((hanja, description));
            }
            None => continue,
        }
    }

    dict
}

fn main() {
    gtk::init().unwrap();

    std::fs::remove_file("/tmp/kime_window.sock").ok();

    let start = Instant::now();
    let dict = load_hanja_dict();
    let elapsed = start.elapsed();

    eprintln!("Hanja dict loaded! elapsed: {}ms", elapsed.as_millis());

    let window = gtk::WindowBuilder::new()
        .default_width(1000)
        .default_height(800)
        .resizable(false)
        .build();

    let sock = UnixListener::bind("/tmp/kime_window.sock").expect("Connect socket");
    let mut buf = Vec::with_capacity(8196);

    loop {
        let mut client = sock.accept().unwrap().0;
        eprintln!("Connect new client!");
        buf.clear();
        let len = client.read_to_end(&mut buf).expect("Read client");
        let data = &buf[..len];

        if let Some((ty, data)) = data.split_first() {
            match *ty {
                // hanja
                b'h' => {
                    if let Ok(data) = std::str::from_utf8(data) {
                        let data = data.trim_end();
                        if let Some(entires) = dict.get(data) {
                            let hanja = spawn_window(&window, entires.iter().copied(), |l, s| {
                                l.contains(s)
                            })
                            .unwrap_or(data);
                            client.write_all(hanja.as_bytes()).ok();
                        }
                    } else {
                        eprintln!("Not UTF-8");
                    }
                }
                // emoji
                b'e' => {
                    if let Some(emoji) =
                        spawn_window(&window, emoji::EMOJIS.iter().copied(), |l, s| l.contains(s))
                    {
                        client.write_all(emoji.as_bytes()).ok();
                    }
                }
                other => {
                    eprintln!("Unknown type: {}", other);
                }
            }
        }

        client.shutdown(std::net::Shutdown::Both).ok();
    }
}
