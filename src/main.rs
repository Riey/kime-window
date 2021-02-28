mod emoji {
    include!(concat!(env!("OUT_DIR"), "/emoji_gen.rs"));
}

use ahash::AHashMap;
use gio::prelude::*;
use gtk::prelude::*;
use libappindicator::{AppIndicator, AppIndicatorStatus};

use std::cell::Cell;
use std::time::Instant;

type Dict = AHashMap<&'static str, Vec<(&'static str, &'static str)>>;

enum IconColor {
    Black,
    White,
}

thread_local! {
    static RET: Cell<Option<&'static str>> = Cell::new(None);
}

fn build_list_box<I: Iterator<Item = (&'static str, &'static str)>>(
    entires: impl IntoIterator<IntoIter = I, Item = (&'static str, &'static str)>,
    main_loop: &glib::MainLoop,
) -> gtk::ListBox {
    let lbox = gtk::ListBoxBuilder::new()
        .selection_mode(gtk::SelectionMode::Single)
        .activate_on_single_click(true)
        .build();

    for (value, description) in entires {
        let label_str = format!("{}: {}", value, description);
        let label = gtk::LabelBuilder::new().label(&label_str).build();
        let row = gtk::ListBoxRowBuilder::new().child(&label).build();
        let main_loop = main_loop.clone();
        row.connect_activate(move |_| {
            RET.with(|ret| ret.set(Some(value)));
            main_loop.quit();
        });
        lbox.add(&row);
    }

    lbox
}

struct UiBase {
    window: gtk::Window,
    window_box: gtk::Box,
    entry: gtk::SearchEntry,
}

impl UiBase {
    pub fn new(main_loop: &glib::MainLoop) -> Self {
        let window = gtk::WindowBuilder::new()
            .default_width(1000)
            .default_height(800)
            .resizable(false)
            .build();

        let main_loop = main_loop.clone();
        window.connect_delete_event(move |_, _| {
            main_loop.quit();
            gtk::Inhibit(true)
        });

        let window_box = gtk::Box::new(gtk::Orientation::Vertical, 5);

        let scroll = gtk::ScrolledWindowBuilder::new().build();

        let entry = gtk::SearchEntryBuilder::new().build();

        window_box.add(&entry);
        scroll.add(&window_box);
        window.add(&scroll);

        Self {
            window,
            window_box,
            entry,
        }
    }
}

fn spawn_window(
    ui: &UiBase,
    lbox: &gtk::ListBox,
    main_loop: &glib::MainLoop,
) -> Option<&'static str> {
    let entry1 = ui.entry.clone();
    lbox.set_filter_func(Some(Box::new(move |row: &gtk::ListBoxRow| {
        let label = row.get_child().unwrap().downcast::<gtk::Label>().unwrap();
        label
            .get_text()
            .as_str()
            .contains(entry1.get_text().as_str())
    })));
    let lbox1 = lbox.clone();
    let search_changed = ui.entry.connect_search_changed(move |_| {
        lbox1.invalidate_filter();
    });

    ui.window_box.add(lbox);
    ui.window.show_all();
    main_loop.run();
    ui.window.hide();
    ui.window_box.remove(lbox);

    glib::signal_handler_disconnect(&ui.entry, search_changed);

    RET.with(|r| r.take())
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

struct KimeIndicator {
    indicator: AppIndicator,
    color: IconColor,
}

impl KimeIndicator {
    pub fn new(color: IconColor) -> anyhow::Result<Self> {
        let mut indicator = AppIndicator::new("kime", "");
        let dir = xdg::BaseDirectories::with_prefix("kime/icons")?;
        let icon = dir
            .find_data_file("kime-han-white-64x64.png")
            .expect("Find icon path");
        indicator.set_icon_theme_path(icon.parent().unwrap().as_os_str().to_str().unwrap());

        let mut menu = gtk::Menu::new();

        indicator.set_menu(&mut menu);
        indicator.set_status(AppIndicatorStatus::Active);

        Ok(Self { indicator, color })
    }

    pub fn han(&mut self) {
        self.indicator.set_icon(match self.color {
            IconColor::Black => "kime-han-black-64x64",
            IconColor::White => "kime-han-white-64x64",
        });
    }

    pub fn eng(&mut self) {
        self.indicator.set_icon(match self.color {
            IconColor::Black => "kime-eng-black-64x64",
            IconColor::White => "kime-eng-white-64x64",
        });
    }
}

fn main() -> anyhow::Result<()> {
    let mut args = pico_args::Arguments::from_env();

    if args.contains(["-h", "--help"]) {
        println!("-h or --help: print help");
        return Ok(());
    }

    let color = if args.contains("--white") {
        IconColor::White
    } else {
        IconColor::Black
    };

    let sock_path = std::path::Path::new("/tmp/kime_window.sock");

    std::fs::remove_file(sock_path).ok();

    gtk::init().unwrap();

    let start = Instant::now();
    let dict = load_hanja_dict();
    let elapsed = start.elapsed();

    eprintln!("Hanja dict loaded! elapsed: {}ms", elapsed.as_millis());

    let mut indicator = KimeIndicator::new(color)?;

    let ctx = glib::MainContext::ref_thread_default();
    let main_loop = glib::MainLoop::new(Some(&ctx), true);
    let ui = UiBase::new(&main_loop);

    let start = Instant::now();
    let emoji_lbox = build_list_box(emoji::EMOJIS.iter().copied(), &main_loop);
    let elapsed = start.elapsed();

    eprintln!("Build emoji list box! elapsed: {}ms", elapsed.as_millis());

    ctx.acquire();

    let sock = gio::Socket::new(
        gio::SocketFamily::Unix,
        gio::SocketType::Stream,
        gio::SocketProtocol::Default,
    )?;
    let listener = gio::SocketListener::new();

    let addr = gio::UnixSocketAddress::new(sock_path);
    sock.bind(&addr, true)?;
    sock.listen()?;
    listener.add_socket(&sock, None::<&glib::Object>)?;

    ctx.spawn_local(async move {
        let mut current_lang = [b'e', b'n', b'g'];
        indicator.eng();

        loop {
            let client: gio::SocketConnection = listener.accept_async_future().await.unwrap().0;
            let input = client.get_input_stream().unwrap();
            let output = client.get_output_stream().unwrap();

            let (buf, len, _) = input
                .read_all_async_future([0; 128], glib::PRIORITY_DEFAULT_IDLE)
                .await
                .unwrap();

            let data: &[u8] = &buf[..len];

            if let Some((ty, mut data)) = data.split_first() {
                match *ty {
                    // icon
                    b'i' => {
                        match data.split_last() {
                            Some((b'\n', left)) => data = left,
                            _ => {}
                        }

                        match data {
                            // English
                            b"eng" => {
                                current_lang.copy_from_slice(data);
                                indicator.eng();
                            }
                            // Hangul
                            b"han" => {
                                current_lang.copy_from_slice(data);
                                indicator.han();
                            }
                            other => {
                                eprintln!("Unknown language icon: {:?}", other);
                            }
                        }
                    }
                    b'l' => {
                        output
                            .write_all_async_future(current_lang, glib::PRIORITY_DEFAULT_IDLE)
                            .await
                            .ok();
                    }
                    // hanja
                    b'h' => {
                        if let Ok(data) = std::str::from_utf8(data) {
                            let data = data.trim_end();
                            if let Some(entires) = dict.get(data) {
                                if let Some(hanja) =
                                    spawn_window(&ui, &build_list_box(entires.iter().copied(), &main_loop), &main_loop)
                                {
                                    output
                                        .write_all_async_future(
                                            hanja.as_bytes(),
                                            glib::PRIORITY_DEFAULT_IDLE,
                                        )
                                        .await
                                        .ok();
                                }
                            }
                        } else {
                            eprintln!("Not UTF-8");
                        }
                    }
                    // emoji
                    b'e' => {
                        if let Some(emoji) = spawn_window(&ui, &emoji_lbox, &main_loop) {
                            output
                                .write_all_async_future(
                                    emoji.as_bytes(),
                                    glib::PRIORITY_DEFAULT_IDLE,
                                )
                                .await
                                .ok();
                        }
                    }
                    other => {
                        eprintln!("Unknown type: {}", other);
                    }
                }
            }

            client.close_async_future(glib::PRIORITY_LOW).await.unwrap();
        }
    });

    ctx.release();

    gtk::main();

    Ok(())
}
