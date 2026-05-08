use adw::prelude::*;
use eijiro_widget::{Entry, FullTextSearchEngine, IndexPaths, PrefixSearchEngine, SearchResult};
use glib::clone;
use glib::prelude::*;
use gtk::gdk;
use gtk4 as gtk;
use gtk4::gio;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use libadwaita as adw;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub fn run_gtk_ui(index_dir: PathBuf, limit: usize) -> anyhow::Result<()> {
    gtk::init()?;

    glib::set_prgname(Some("eijiro-widget"));
    glib::set_application_name("Eijiro Widget");

    let application = adw::Application::builder()
        .application_id("nt.ere22.eijiro-widget")
        .build();

    application.connect_startup(|_| {
        if let Some(display) = gdk::Display::default() {
            adw::StyleManager::for_display(&display).set_color_scheme(adw::ColorScheme::PreferDark);
        }
        let provider = gtk::CssProvider::new();
        provider.load_from_data(
            r"
            window,
            .background,
            window.eijiro-window,
            window.eijiro-window contents {
                background-color: transparent;
                background-image: none;
                box-shadow: none;
                border: none;
            }

            .main-content {
                background-color: alpha(@window_bg_color, 0.85);
                border-radius: 16px;
                border: 1px solid @borders;
                box-shadow: 0 10px 30px rgba(0, 0, 0, 0.5);
                padding: 16px;
            }

            .title-3 {
                font-weight: bold;
                font-size: 1.2em;
                color: @accent_color;
            }
            .caption {
                font-size: 0.9em;
                font-weight: bold;
                color: @accent_color;
            }
            .dim-label {
                opacity: 0.8;
                color: @window_fg_color;
            }
            .body {
                font-size: 1.05em;
                color: @window_fg_color;
            }
            list {
                background: transparent;
            }
            row {
                background: alpha(@window_fg_color, 0.03);
                border-radius: 10px;
                margin: 4px 0;
                padding: 8px;
            }
            row:selected {
                background: alpha(@accent_bg_color, 0.3);
            }
            entry {
                border-radius: 10px;
                background: alpha(@window_bg_color, 0.5);
                border: 1px solid @borders;
                padding: 10px;
                font-size: 1.1em;
                color: @text_color;
            }
            ",
        );
        gtk::style_context_add_provider_for_display(
            &gdk::Display::default().expect("Could not connect to a display."),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_USER,
        );
    });

    application
        .register(None::<&gio::Cancellable>)
        .map_err(|e| anyhow::anyhow!("Failed to register application: {e}"))?;

    if application.is_remote() {
        application.run_with_args::<&str>(&[]);
        return Ok(());
    }

    let ui = Arc::new(GtkUi::new(index_dir, limit));
    ui.start_engine_load();

    let hold_guard = application.hold();

    application.connect_activate(clone!(
        #[strong]
        ui,
        move |app| {
            let _ = &hold_guard;
            ui.show_or_build(app);
        }
    ));

    application.run_with_args::<&str>(&[]);
    Ok(())
}

// ---------------------------------------------------------------------------
// GtkUi
// ---------------------------------------------------------------------------

pub struct GtkUi {
    index_dir: PathBuf,
    limit: usize,
    prefix_engine: Arc<Mutex<Option<PrefixSearchEngine>>>,
    fulltext_engine: Arc<Mutex<Option<FullTextSearchEngine>>>,
    window: Mutex<Option<gtk::Window>>,
    search_entry: Mutex<Option<gtk::SearchEntry>>,
    list_box: Mutex<Option<gtk::ListBox>>,
}

impl GtkUi {
    pub fn new(index_dir: PathBuf, limit: usize) -> Self {
        Self {
            index_dir,
            limit,
            prefix_engine: Arc::new(Mutex::new(None)),
            fulltext_engine: Arc::new(Mutex::new(None)),
            window: Mutex::new(None),
            search_entry: Mutex::new(None),
            list_box: Mutex::new(None),
        }
    }

    pub fn start_engine_load(&self) {
        let index_dir = self.index_dir.clone();
        let prefix_arc = self.prefix_engine.clone();
        let fulltext_arc = self.fulltext_engine.clone();

        std::thread::spawn(move || {
            let paths = IndexPaths::new(&index_dir);
            if let Ok(engine) = PrefixSearchEngine::load(&paths) {
                *prefix_arc.lock().unwrap() = Some(engine);
            } else {
                log::error!("Failed to load PrefixSearchEngine");
            }
            if let Ok(engine) = FullTextSearchEngine::load(&paths) {
                *fulltext_arc.lock().unwrap() = Some(engine);
            } else {
                log::error!("Failed to load FullTextSearchEngine");
            }
        });
    }

    pub fn show_or_build(&self, app: &adw::Application) {
        let cached = self.window.lock().unwrap().clone();
        if let Some(window) = cached {
            self.reset_and_show(&window);
            return;
        }

        let window = self.build_window(app);
        *self.window.lock().unwrap() = Some(window.clone());

        window.present();

        if let Some(entry) = self.search_entry.lock().unwrap().as_ref() {
            entry.grab_focus();
        }
    }

    fn reset_and_show(&self, window: &gtk::Window) {
        if let Some(entry) = self.search_entry.lock().unwrap().as_ref() {
            entry.set_text("");
        }
        if let Some(lb) = self.list_box.lock().unwrap().as_ref() {
            while let Some(child) = lb.first_child() {
                lb.remove(&child);
            }
        }

        window.present();

        if let Some(entry) = self.search_entry.lock().unwrap().as_ref() {
            entry.grab_focus();
        }
    }

    #[allow(clippy::too_many_lines)]
    fn build_window(&self, app: &adw::Application) -> gtk::Window {
        let window = gtk::Window::builder()
            .application(app)
            .title("Eijiro Widget")
            .css_name("eijiro-window")
            .build();

        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_namespace("eijiro-widget");
        window.set_keyboard_mode(KeyboardMode::Exclusive);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Bottom, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);
        window.set_decorated(false);
        window.set_opacity(0.999);

        let display = gtk::prelude::WidgetExt::display(&window);
        let monitors = display.monitors();
        let monitor = monitors
            .item(0)
            .and_then(|obj: glib::Object| obj.downcast::<gdk::Monitor>().ok())
            .expect("Failed to get any monitor");
        let geometry = monitor.geometry();
        let screen_width = geometry.width();
        let screen_height = geometry.height();

        let target_width = (f64::from(screen_width) * 0.7).min(1000.0) as i32;
        let target_height = (f64::from(screen_height) * 0.6) as i32;
        let margin_top = (f64::from(screen_height) * 0.1) as i32;

        let main_container = gtk::Box::new(gtk::Orientation::Vertical, 12);
        main_container.add_css_class("main-content");
        main_container.set_halign(gtk::Align::Center);
        main_container.set_valign(gtk::Align::Start);
        main_container.set_margin_top(margin_top);
        main_container.set_width_request(target_width);
        main_container.set_height_request(target_height);

        let search_entry = gtk::SearchEntry::builder()
            .placeholder_text("Search...")
            .build();

        let scrolled_window = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .vexpand(true)
            .build();

        let list_box = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::Single)
            .css_classes(["navigation-sidebar"])
            .build();

        *self.search_entry.lock().unwrap() = Some(search_entry.clone());
        *self.list_box.lock().unwrap() = Some(list_box.clone());

        let search_key_controller = gtk::EventControllerKey::new();
        search_key_controller.connect_key_pressed(clone!(
            #[weak]
            list_box,
            #[weak]
            scrolled_window,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_controller, key, _code, _state| {
                match key {
                    gdk::Key::Down => {
                        let current_row = list_box.selected_row();
                        let next_row = current_row.as_ref().map_or_else(
                            || list_box.first_child(),
                            gtk::prelude::WidgetExt::next_sibling,
                        );
                        if let Some(row) =
                            next_row.and_then(|r| r.downcast::<gtk::ListBoxRow>().ok())
                        {
                            list_box.select_row(Some(&row));
                            smooth_scroll_to_row(&scrolled_window, &list_box, &row, 100);
                        }
                        glib::Propagation::Stop
                    }
                    gdk::Key::Up => {
                        let current_row = list_box.selected_row();
                        let prev_row = current_row
                            .as_ref()
                            .and_then(gtk::prelude::WidgetExt::prev_sibling);
                        if let Some(row) =
                            prev_row.and_then(|r| r.downcast::<gtk::ListBoxRow>().ok())
                        {
                            list_box.select_row(Some(&row));
                            smooth_scroll_to_row(&scrolled_window, &list_box, &row, 200);
                        }
                        glib::Propagation::Stop
                    }
                    _ => glib::Propagation::Proceed,
                }
            }
        ));
        search_entry.add_controller(search_key_controller);

        list_box.set_adjustment(Some(&scrolled_window.vadjustment()));
        scrolled_window.set_child(Some(&list_box));
        main_container.append(&search_entry);
        main_container.append(&scrolled_window);
        window.set_child(Some(&main_container));

        let key_controller = gtk::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        key_controller.connect_key_pressed(clone!(
            #[weak]
            window,
            #[weak]
            app,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_controller, key, _code, state| {
                if key == gdk::Key::Escape {
                    if state.contains(gdk::ModifierType::CONTROL_MASK) {
                        app.quit();
                    } else {
                        window.hide();
                    }
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            }
        ));
        window.add_controller(key_controller);

        let (result_tx, result_rx) = async_channel::unbounded::<SearchResult>();

        glib::MainContext::default().spawn_local(clone!(
            #[weak]
            list_box,
            #[weak]
            scrolled_window,
            async move {
                while let Ok(mut result) = result_rx.recv().await {
                    // キューに複数溜まっていれば最新のみ処理
                    while let Ok(next) = result_rx.try_recv() {
                        result = next;
                    }

                    // 新しい行を先にビルドしてから UI を一括更新
                    let new_rows: Vec<_> = result.entries.iter().map(create_entry_row).collect();

                    while let Some(child) = list_box.first_child() {
                        list_box.remove(&child);
                    }
                    for row in new_rows {
                        list_box.append(&row);
                    }

                    // 先頭を自動選択してスクロールトップ
                    if let Some(first) = list_box
                        .first_child()
                        .and_then(|c| c.downcast::<gtk::ListBoxRow>().ok())
                    {
                        list_box.select_row(Some(&first));
                        scrolled_window.vadjustment().set_value(0.0);
                    }
                }
            }
        ));

        let (query_tx, query_rx) = std::sync::mpsc::channel::<(String, bool)>();
        let prefix_arc = self.prefix_engine.clone();
        let fulltext_arc = self.fulltext_engine.clone();
        let limit = self.limit;

        std::thread::spawn(move || {
            while let Ok((mut query, mut is_fulltext)) = query_rx.recv() {
                // 積まれた古いクエリをスキップ
                while let Ok((nq, nf)) = query_rx.try_recv() {
                    query = nq;
                    is_fulltext = nf;
                }

                let result = if is_fulltext {
                    let lock = fulltext_arc.lock().unwrap();
                    match lock.as_ref() {
                        Some(e) => e.search_fulltext(&query, limit),
                        None => {
                            log::warn!("FullTextSearchEngine not yet loaded, skipping query");
                            continue;
                        }
                    }
                } else {
                    let lock = prefix_arc.lock().unwrap();
                    match lock.as_ref() {
                        Some(e) => e.search_prefix(&query, limit),
                        None => {
                            log::warn!("PrefixSearchEngine not yet loaded, skipping query");
                            continue;
                        }
                    }
                };

                if let Ok(res) = result {
                    let _ = result_tx.send_blocking(res);
                }
            }
        });

        search_entry.connect_search_changed(clone!(
            #[weak]
            list_box,
            move |entry| {
                let query = entry.text().to_string();
                if query.is_empty() {
                    while let Some(child) = list_box.first_child() {
                        list_box.remove(&child);
                    }
                    return;
                }

                let is_fulltext = query.starts_with(';') || query.ends_with(';');
                let clean_query = if is_fulltext {
                    query.trim_matches(';').trim().to_string()
                } else {
                    query
                };

                if clean_query.is_empty() {
                    return;
                }

                let _ = query_tx.send((clean_query, is_fulltext));
            }
        ));

        window
    }
}

// ---------------------------------------------------------------------------
// ユーティリティ
// ---------------------------------------------------------------------------

fn smooth_scroll_to_row(
    scrolled_window: &gtk::ScrolledWindow,
    list_box: &gtk::ListBox,
    row: &gtk::ListBoxRow,
    duration_ms: u32,
) {
    if let Some((_, y)) = row.translate_coordinates(list_box, 0.0, 0.0) {
        let is_first = list_box.first_child().is_some_and(|f| f == *row);
        let target_y = if is_first { 0.0 } else { (y - 12.0).max(0.0) };
        let vadjustment = scrolled_window.vadjustment();
        let animation = adw::TimedAnimation::builder()
            .widget(scrolled_window)
            .duration(duration_ms)
            .easing(adw::Easing::EaseOutCubic)
            .target(&adw::CallbackAnimationTarget::new(clone!(
                #[weak]
                vadjustment,
                move |val| vadjustment.set_value(val)
            )))
            .value_from(vadjustment.value())
            .value_to(target_y)
            .build();
        animation.play();
    }
}

fn create_entry_row(entry: &Entry) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let main_box = gtk::Box::new(gtk::Orientation::Vertical, 1);
    main_box.set_margin_top(2);
    main_box.set_margin_bottom(2);
    main_box.set_margin_start(4);
    main_box.set_margin_end(4);

    let headword_label = gtk::Label::builder()
        .label(entry.headword.as_ref())
        .xalign(0.0)
        .css_classes(["title-3"])
        .build();
    main_box.append(&headword_label);

    if let Some(entry_type) = &entry.entry_type {
        let type_label = gtk::Label::builder()
            .label(entry_type.as_ref())
            .xalign(0.0)
            .css_classes(["caption", "dim-label"])
            .build();
        main_box.append(&type_label);
    }

    for sense in &entry.senses {
        let sense_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        sense_box.set_margin_start(4);

        let desc_label = gtk::Label::builder()
            .label(sense.description.as_ref())
            .xalign(0.0)
            .wrap(true)
            .build();
        sense_box.append(&desc_label);

        if !sense.complements.is_empty() {
            let comp_text = sense
                .complements
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>()
                .join("; ");
            let comp_label = gtk::Label::builder()
                .label(format!("Note: {comp_text}"))
                .xalign(0.0)
                .wrap(true)
                .css_classes(["body", "dim-label"])
                .build();
            sense_box.append(&comp_label);
        }

        if !sense.examples.is_empty() {
            let example_text = sense
                .examples
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>()
                .join("\n");
            let example_label = gtk::Label::builder()
                .label(&example_text)
                .xalign(0.0)
                .wrap(true)
                .css_classes(["body", "dim-label"])
                .build();
            sense_box.append(&example_label);
        }

        main_box.append(&sense_box);
    }

    row.set_child(Some(&main_box));
    row
}
