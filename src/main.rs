use ahash::AHashMap;
use gtk::prelude::*;

use std::cell::{Cell, RefCell};
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::rc::Rc;
use std::time::Instant;

type Dict = AHashMap<&'static str, Vec<HanjaEntry>>;

fn spawn_window<'a>(window: &gtk::Window, hangul: &'a str, dict: &Dict) -> &'a str {
    let ret = Rc::new(Cell::new(""));
    let search_str = Rc::new(RefCell::new(glib::GString::from(hangul)));

    let hanja_entires = match dict.get(hangul) {
        Some(entires) => entires,
        _ => return hangul,
    };

    let window_box = gtk::Box::new(gtk::Orientation::Vertical, 5);

    let lbox = gtk::ListBoxBuilder::new()
        .selection_mode(gtk::SelectionMode::Single)
        .activate_on_single_click(true)
        .build();

    let search_str1 = search_str.clone();
    let mut ids = Vec::with_capacity(hanja_entires.len());

    for &entry in hanja_entires {
        let label_str = format!("{}: {}", entry.hanja, entry.description);
        let label = gtk::LabelBuilder::new().label(&label_str).build();
        let window1 = window.clone();
        let row = gtk::ListBoxRowBuilder::new().child(&label).build();
        let ret = Rc::downgrade(&ret);
        let id = row.connect_activate(move |_| {
            ret.upgrade().unwrap().set(entry.hanja);
            window1.hide();
            gtk::main_quit();
        });
        lbox.add(&row);
        ids.push((id, row));
    }

    lbox.set_filter_func(Some(Box::new(move |row: &gtk::ListBoxRow| {
        let label = row.get_child().unwrap().downcast::<gtk::Label>().unwrap();
        label
            .get_text()
            .as_str()
            .contains(search_str1.borrow().as_str())
    })));

    let entry = gtk::SearchEntryBuilder::new()
        .text(search_str.borrow().as_str())
        .build();

    let lbox1 = lbox.clone();
    let search_changed = entry.connect_search_changed(move |entry| {
        *search_str.borrow_mut() = entry.get_text();
        lbox1.invalidate_filter();
    });

    let deleted = window.connect_delete_event(|window, _| {
        window.hide();
        gtk::main_quit();
        gtk::Inhibit(true)
    });

    window_box.add(&entry);
    window_box.add(&lbox);
    window.add(&window_box);
    window.show_all();

    gtk::main();

    window.remove(&window_box);

    glib::signal_handler_disconnect(window, deleted);
    glib::signal_handler_disconnect(&entry, search_changed);

    for (id, row) in ids {
        glib::signal_handler_disconnect(&row, id);
    }

    match ret.take() {
        "" => hangul,
        other => other,
    }
}

#[derive(Clone, Copy)]
struct HanjaEntry {
    hanja: &'static str,
    description: &'static str,
}

fn load_dict() -> Dict {
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

                dict.entry(hangul)
                    .or_default()
                    .push(HanjaEntry { hanja, description });
            }
            None => continue,
        }
    }

    dict
}

fn main() {
    gtk::init().unwrap();

    std::fs::remove_file("/tmp/kime_hanja.sock").ok();

    let start = Instant::now();
    let dict = load_dict();
    let elapsed = start.elapsed();

    eprintln!("Hanja dict loaded! elapsed: {}ms", elapsed.as_millis());

    let window = gtk::WindowBuilder::new()
        .resizable(false)
        .decorated(false)
        .deletable(true)
        .width_request(600)
        .height_request(500)
        .build();

    let sock = UnixListener::bind("/tmp/kime_hanja.sock").expect("Connect socket");
    let mut buf = Vec::with_capacity(8196);

    loop {
        let mut client = sock.accept().unwrap().0;
        eprintln!("Connect new client!");
        buf.clear();
        let len = client.read_to_end(&mut buf).expect("Read client");
        let data = &buf[..len];

        if let Ok(data) = std::str::from_utf8(data) {
            let hanja = spawn_window(&window, data.trim_end(), &dict);
            client.write_all(hanja.as_bytes()).ok();
        } else {
            eprintln!("Not UTF-8");
        }

        client.shutdown(std::net::Shutdown::Both).ok();
    }
}
