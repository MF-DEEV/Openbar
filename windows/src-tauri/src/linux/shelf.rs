// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Dragging files, links and text onto the island, and tiles off it, on Linux.
//! Windows has OLE for both (`files/drop_target.rs`, `files/drag_out.rs`); here
//! it is GTK's own drag and drop on the island's web view. A drag from another
//! program is taken for the shelf, and a tile goes out as a URI list, which file
//! managers, editors and browsers take as the files. A drag that starts in the
//! page itself (a text selection) is left to the page.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use gtk::gdk::{self, DragAction};
use gtk::glib;
use gtk::prelude::*;
use tauri::{AppHandle, Manager, Runtime, WebviewWindow};

use crate::files::store::Dropped;

const URI_LIST: &str = "text/uri-list";
/// The text targets, the best first.
const TEXTS: [&str; 3] = ["UTF8_STRING", "text/plain;charset=utf-8", "text/plain"];
/// The GType name of the page's widget.
const WEB_VIEW: &str = "WebKitWebView";
const URIS_INFO: u32 = 1;
const TEXT_INFO: u32 = 2;

/// Which target to ask a drag for, by the names it offers: its URI list (files
/// and links), else its text.
fn wanted(offered: &[String]) -> Option<&'static str> {
    if offered.iter().any(|name| name == URI_LIST) {
        return Some(URI_LIST);
    }
    TEXTS
        .into_iter()
        .find(|text| offered.iter().any(|name| name == text))
}

/// The URIs of a `text/uri-list`: one to a line, and a line starting with `#`
/// is a comment.
fn uri_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(String::from)
        .collect()
}

/// Whether a `file:` URI names a file on this machine: it has no host, or
/// `localhost`, or `host`, which is this machine's own name. GIO would read
/// the path of another machine's file as a path here.
fn is_local(uri: &str, host: &str) -> bool {
    let Some(rest) = uri.strip_prefix("file:") else {
        return false;
    };
    match rest.strip_prefix("//") {
        None => true,
        Some(after) => {
            let name = after.split('/').next().unwrap_or("");
            name.is_empty()
                || name.eq_ignore_ascii_case("localhost")
                || name.eq_ignore_ascii_case(host)
        }
    }
}

/// What a drop of these URIs is for the shelf: the files in it, each with the
/// id it is found again by, else the first link.
fn dropped_uris(uris: &[String], host: &str, identity: &dyn Fn(&str) -> Option<String>) -> Dropped {
    let mut dropped = Dropped::default();
    for uri in uris {
        if uri
            .get(..5)
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case("file:"))
        {
            let path = is_local(uri, host)
                .then(|| gtk::gio::File::for_uri(uri).path())
                .flatten();
            if let Some(path) = path {
                let path = path.to_string_lossy().into_owned();
                let id = identity(&path);
                dropped.paths.push((path, id));
            }
        } else if dropped.url.is_none() {
            dropped.url = Some(uri.clone());
        }
    }
    dropped
}

fn dropped_data(
    target: &str,
    data: &gtk::SelectionData,
    identity: &dyn Fn(&str) -> Option<String>,
) -> Dropped {
    if target == URI_LIST {
        let text = String::from_utf8_lossy(&data.data()).into_owned();
        dropped_uris(&uri_list(&text), &glib::host_name(), identity)
    } else {
        Dropped {
            text: data.text().map(|text| text.to_string()),
            ..Default::default()
        }
    }
}

fn is_something(dropped: &Dropped) -> bool {
    !dropped.paths.is_empty()
        || dropped.url.is_some()
        || dropped
            .text
            .as_ref()
            .is_some_and(|text| !text.trim().is_empty())
}

fn offered(context: &gdk::DragContext) -> Vec<String> {
    context
        .list_targets()
        .iter()
        .map(|atom| atom.name().to_string())
        .collect()
}

/// What the page is told of a drag over the island.
pub enum Drag {
    Enter,
    Leave,
    Drop(Dropped),
}

enum Claim {
    /// Not ours: the page's own drag, or one with nothing the shelf takes.
    Pass,
    /// A drag of our own, a tile or a screenshot: it does not land back on the island.
    Refuse,
    Take,
}

fn claim(context: &gdk::DragContext) -> Claim {
    match context.drag_get_source_widget() {
        // A text selection dragged in the page is the page's.
        Some(source) if source.type_().name() == WEB_VIEW => Claim::Pass,
        Some(_) => Claim::Refuse,
        None if wanted(&offered(context)).is_some() => Claim::Take,
        None => Claim::Pass,
    }
}

#[derive(Default)]
struct Session {
    /// The page was told a drag came in.
    entered: Cell<bool>,
    /// A drop is asked for its data.
    dropping: Cell<bool>,
}

/// Makes `widget` (already a drag destination) take the drags other programs
/// bring: `accepts` says whether the shelf may take one at all, `told` is what
/// the page hears. A drag that started in our own windows is refused.
pub fn accept_drops(
    widget: &gtk::Widget,
    accepts: impl Fn() -> bool + 'static,
    identity: impl Fn(&str) -> Option<String> + 'static,
    told: impl Fn(Drag) + 'static,
) {
    let session = Rc::new(Session::default());
    let accepts = Rc::new(accepts);
    let identity = Rc::new(identity);
    let told = Rc::new(told);

    widget.connect_drag_motion({
        let (session, told) = (session.clone(), told.clone());
        move |_, context, _, _, time| match claim(context) {
            Claim::Pass => false,
            Claim::Refuse => {
                context.drag_status(DragAction::empty(), time);
                true
            }
            Claim::Take => {
                if accepts() {
                    if !session.entered.replace(true) {
                        told(Drag::Enter);
                    }
                    context.drag_status(DragAction::COPY, time);
                } else {
                    context.drag_status(DragAction::empty(), time);
                }
                true
            }
        }
    });

    widget.connect_drag_leave({
        let (session, told) = (session.clone(), told.clone());
        move |_, context, _| {
            if !session.entered.get() || !matches!(claim(context), Claim::Take) {
                return;
            }
            // A drop comes as a leave and then the drop: the page hears of a leave
            // only when nothing is dropped.
            let (session, told) = (session.clone(), told.clone());
            glib::idle_add_local_once(move || {
                if session.entered.get() && !session.dropping.get() {
                    session.entered.set(false);
                    told(Drag::Leave);
                }
            });
        }
    });

    widget.connect_drag_drop({
        let session = session.clone();
        move |widget, context, _, _, time| match claim(context) {
            Claim::Pass => false,
            Claim::Refuse => {
                context.drag_finish(false, false, time);
                true
            }
            Claim::Take => {
                let target = wanted(&offered(context)).unwrap_or(URI_LIST);
                session.dropping.set(true);
                widget.drag_get_data(context, &gdk::Atom::intern(target), time);
                true
            }
        }
    });

    widget.connect_drag_data_received(move |_, context, _, _, data, _, time| {
        // The page's own requests are answered by the page.
        if !session.dropping.replace(false) {
            return;
        }
        session.entered.set(false);
        let target = data.target().name().to_string();
        let dropped = dropped_data(&target, data, &*identity);
        context.drag_finish(is_something(&dropped), false, time);
        told(Drag::Drop(dropped));
    });
}

/// What is dragged out: files as a URI list, and text.
pub struct Payload {
    pub uris: Vec<String>,
    pub text: Option<String>,
}

/// Starts a drag of `payload` from `widget`, whose pointer button is down.
/// `done` is told whether a destination took it. False when no drag could start.
pub fn begin_drag(
    widget: &gtk::Widget,
    payload: Payload,
    actions: DragAction,
    done: impl FnOnce(bool) + 'static,
) -> bool {
    let targets = gtk::TargetList::new(&[]);
    if !payload.uris.is_empty() {
        targets.add(&gdk::Atom::intern(URI_LIST), 0, URIS_INFO);
    }
    if payload.text.is_some() {
        targets.add_text_targets(TEXT_INFO);
    }
    let handlers: Rc<RefCell<Vec<glib::SignalHandlerId>>> = Rc::default();
    let failed = Rc::new(Cell::new(false));
    let done = Rc::new(RefCell::new(Some(done)));

    handlers.borrow_mut().push(
        widget.connect_drag_data_get(move |_, _, selection, info, _| match info {
            URIS_INFO => {
                let uris: Vec<&str> = payload.uris.iter().map(String::as_str).collect();
                selection.set_uris(&uris);
            }
            TEXT_INFO => {
                if let Some(text) = &payload.text {
                    selection.set_text(text);
                }
            }
            _ => {}
        }),
    );
    handlers.borrow_mut().push(widget.connect_drag_failed({
        let failed = failed.clone();
        move |_, _, _| {
            failed.set(true);
            false.into()
        }
    }));
    let finish = {
        let (handlers, failed, done) = (handlers.clone(), failed.clone(), done.clone());
        move |widget: &gtk::Widget, took: bool| {
            for id in handlers.borrow_mut().drain(..) {
                widget.disconnect(id);
            }
            if let Some(done) = done.borrow_mut().take() {
                done(took && !failed.get());
            }
        }
    };
    handlers.borrow_mut().push(widget.connect_drag_end({
        let finish = finish.clone();
        move |widget, context| {
            finish(widget.upcast_ref(), !context.selected_action().is_empty());
        }
    }));
    match widget.drag_begin_with_coordinates(&targets, actions, 1, None, -1, -1) {
        Some(_) => true,
        None => {
            finish(widget, false);
            false
        }
    }
}

/// A drag takes the button's release from the web view, which then believes the
/// button is still down and passes over the next press. This tells it the
/// button was let go.
fn release_button(view: &gtk::Widget) {
    let Some(window) = view.window() else {
        return;
    };
    let Some(pointer) = window
        .display()
        .default_seat()
        .and_then(|seat| seat.pointer())
    else {
        return;
    };
    let (_, x, y, state) = window.device_position(&pointer);
    let (_, root_x, root_y) = pointer.position();
    let mut event = gdk::Event::new(gdk::EventType::ButtonRelease);
    event.set_device(Some(&pointer));
    // SAFETY: the event is a fresh button event of our own, set up completely
    // here and queued once; GDK frees it, which releases the window reference.
    unsafe {
        use gtk::glib::translate::ToGlibPtr;
        let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none().0;
        let button = raw.cast::<gdk::ffi::GdkEventButton>();
        (*button).window = window.to_glib_full();
        (*button).send_event = 1;
        (*button).time = gdk::ffi::GDK_CURRENT_TIME as u32;
        (*button).x = f64::from(x);
        (*button).y = f64::from(y);
        (*button).state = (state | gdk::ModifierType::BUTTON1_MASK).bits();
        (*button).button = 1;
        (*button).x_root = f64::from(root_x);
        (*button).y_root = f64::from(root_y);
    }
    event.put();
}

/// A window's web view and the window itself.
fn widgets_of<R: Runtime>(window: &WebviewWindow<R>) -> Option<(gtk::Widget, gtk::Widget)> {
    let toplevel: gtk::Widget = window.gtk_window().ok()?.upcast();
    let view = find_web_view(&toplevel)?;
    Some((view, toplevel))
}

fn find_web_view(widget: &gtk::Widget) -> Option<gtk::Widget> {
    if widget.type_().name() == WEB_VIEW {
        return Some(widget.clone());
    }
    widget
        .downcast_ref::<gtk::Container>()?
        .children()
        .iter()
        .find_map(find_web_view)
}

/// Puts the drop on the island's web view. Runs on the main thread.
pub fn install<R: Runtime>(app: &AppHandle<R>) {
    let Some((view, _)) = app
        .get_webview_window(crate::island::LABEL)
        .and_then(|window| widgets_of(&window))
    else {
        return;
    };
    let (accepting, told) = (app.clone(), app.clone());
    accept_drops(
        &view,
        move || crate::files::accepts_drops(&accepting),
        crate::files::file_id,
        move |drag| match drag {
            Drag::Enter => crate::files::drag_phase(&told, "enter", false),
            Drag::Leave => crate::files::drag_phase(&told, "leave", false),
            Drag::Drop(dropped) => {
                let items = crate::files::store::items_from_drop(dropped);
                let added = crate::files::add(&told, items);
                crate::files::drag_phase(&told, "drop", added);
            }
        },
    );
}

/// Drags `payload` out of `window`, whose pointer button is down; answers once
/// dropped, true when a destination took it.
pub async fn drag_from<R: Runtime>(
    window: &WebviewWindow<R>,
    payload: Payload,
    actions: DragAction,
) -> bool {
    let (tx, rx) = mpsc::channel::<bool>();
    let source = window.clone();
    let started = window.app_handle().run_on_main_thread(move || {
        let Some((view, toplevel)) = widgets_of(&source) else {
            let _ = tx.send(false);
            return;
        };
        let answer = tx.clone();
        if !begin_drag(&toplevel, payload, actions, move |took| {
            // Once GTK has cleaned up after the drag, not in the middle of it.
            glib::idle_add_local_once(move || release_button(&view));
            let _ = answer.send(took);
        }) {
            let _ = tx.send(false);
        }
    });
    if started.is_err() {
        return false;
    }
    tauri::async_runtime::spawn_blocking(move || {
        rx.recv_timeout(Duration::from_secs(600)).unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

/// Drags these files and texts out of the island; answers once dropped, true
/// when a destination took them.
pub async fn drag_out<R: Runtime>(
    app: &AppHandle<R>,
    paths: Vec<String>,
    texts: Vec<String>,
    allow_move: bool,
) -> bool {
    let Some(window) = app.get_webview_window(crate::island::LABEL) else {
        return false;
    };
    let payload = Payload {
        uris: paths
            .iter()
            .map(|path| gtk::gio::File::for_path(path).uri().to_string())
            .collect(),
        text: (!texts.is_empty()).then(|| texts.join("\n")),
    };
    let actions = if allow_move {
        DragAction::COPY | DragAction::MOVE
    } else {
        DragAction::COPY
    };
    drag_from(&window, payload, actions).await
}

/// Drags one file out of a window of ours (a screenshot's preview), as a copy.
pub async fn drag_file<R: Runtime>(window: &WebviewWindow<R>, path: &str) -> bool {
    let payload = Payload {
        uris: vec![gtk::gio::File::for_path(path).uri().to_string()],
        text: None,
    };
    drag_from(window, payload, DragAction::COPY).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_drag_is_asked_for_its_files_first_and_else_its_text() {
        assert_eq!(
            wanted(&names(&["text/plain", "text/uri-list"])),
            Some(URI_LIST)
        );
        assert_eq!(
            wanted(&names(&["text/plain", "UTF8_STRING"])),
            Some("UTF8_STRING")
        );
        assert_eq!(wanted(&names(&["text/plain"])), Some("text/plain"));
        assert_eq!(wanted(&names(&["image/png", "text/html"])), None);
        assert_eq!(wanted(&[]), None);
    }

    #[test]
    fn a_uri_list_is_a_line_to_a_uri_without_the_comments() {
        let text =
            "# from a file manager\r\nfile:///home/me/a%20b.txt\r\n\r\n  file:///tmp/c.txt  \n";
        assert_eq!(
            uri_list(text),
            ["file:///home/me/a%20b.txt", "file:///tmp/c.txt"]
        );
        assert!(uri_list("").is_empty());
        assert!(uri_list("# only a comment\n").is_empty());
    }

    #[test]
    fn files_are_paths_with_their_ids_and_the_first_link_is_the_url() {
        let identity = |path: &str| Some(format!("id:{path}"));
        let dropped = dropped_uris(
            &names(&[
                "file:///home/me/a%20b.txt",
                "file://localhost/tmp/%C5%82.txt",
                "file://mybox/tmp/here.txt",
                "https://example.com/x",
            ]),
            "mybox",
            &identity,
        );
        let path = |p: &str| (p.to_string(), Some(format!("id:{p}")));
        assert_eq!(
            dropped.paths,
            [
                path("/home/me/a b.txt"),
                path("/tmp/ł.txt"),
                path("/tmp/here.txt")
            ]
        );
        assert_eq!(
            dropped.url.as_deref(),
            Some("https://example.com/x"),
            "the link is kept beside the files"
        );
        assert!(dropped.text.is_none());

        let links = dropped_uris(
            &names(&["https://a.example/", "https://b.example/"]),
            "mybox",
            &identity,
        );
        assert!(links.paths.is_empty());
        assert_eq!(
            links.url.as_deref(),
            Some("https://a.example/"),
            "only the first"
        );
    }

    #[test]
    fn a_file_of_another_machine_is_not_a_path_here_nor_a_link() {
        let identity = |_: &str| None;
        let remote = dropped_uris(
            &names(&["file://elsewhere/tmp/x", "FILE://elsewhere/tmp/y"]),
            "mybox",
            &identity,
        );
        assert!(remote.paths.is_empty());
        assert!(remote.url.is_none());
        assert!(is_local("file:///tmp/x", "mybox"));
        assert!(is_local("file:/tmp/x", "mybox"));
        assert!(is_local("file://LOCALHOST/tmp/x", "mybox"));
        assert!(is_local("file://MyBox/tmp/x", "mybox"));
        assert!(!is_local("file://other/tmp/x", "mybox"));
        assert!(!is_local("https://mybox/x", "mybox"));
    }

    /// The other program of a drag, in Python's GTK: a window that offers `uris` when dragged from
    /// (`source`), or takes what is dropped on it and writes the URIs to `log` (`target`).
    const PARTNER: &str = r#"
import sys
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk, Gdk
role, x, y, log = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), sys.argv[4]
uris = sys.argv[5:]
win = Gtk.Window()
win.set_default_size(300, 200)
win.move(x, y)
box = Gtk.EventBox()
win.add(box)
entry = Gtk.TargetEntry.new("text/uri-list", 0, 1)
text_entries = [Gtk.TargetEntry.new("UTF8_STRING", 0, 2), Gtk.TargetEntry.new("text/plain", 0, 2)]
actions = Gdk.DragAction.COPY | Gdk.DragAction.MOVE
if role == "source":
    box.drag_source_set(Gdk.ModifierType.BUTTON1_MASK, [entry], actions)
    box.connect("drag-data-get", lambda w, ctx, data, info, time: data.set_uris(uris))
else:
    def received(w, ctx, x, y, data, info, time):
        if info == 1:
            open(log, "w").write("\n".join(data.get_uris()))
        else:
            open(log, "w").write("text:" + (data.get_text() or ""))
        Gtk.drag_finish(ctx, True, False, time)
    box.drag_dest_set(Gtk.DestDefaults.ALL, [entry] + text_entries, actions)
    box.connect("drag-data-received", received)
win.connect("destroy", Gtk.main_quit)
win.show_all()
print("ready", flush=True)
Gtk.main()
"#;

    fn pump(milliseconds: u64) {
        let end = std::time::Instant::now() + Duration::from_millis(milliseconds);
        while std::time::Instant::now() < end {
            while gtk::events_pending() {
                gtk::main_iteration();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn pump_until(limit_ms: u64, done: impl Fn() -> bool) -> bool {
        let end = std::time::Instant::now() + Duration::from_millis(limit_ms);
        while std::time::Instant::now() < end {
            pump(20);
            if done() {
                return true;
            }
        }
        false
    }

    fn xdotool(arguments: &[&str]) {
        let status = std::process::Command::new("xdotool")
            .args(arguments)
            .status()
            .expect("xdotool");
        assert!(status.success(), "xdotool {arguments:?}");
    }

    /// Moves the pointer in small steps, as a hand does, while GTK keeps up.
    fn glide(from: (i32, i32), to: (i32, i32)) {
        for step in 1..=12 {
            let x = from.0 + (to.0 - from.0) * step / 12;
            let y = from.1 + (to.1 - from.1) * step / 12;
            xdotool(&["mousemove", &x.to_string(), &y.to_string()]);
            pump(40);
        }
    }

    fn partner(
        role: &str,
        at: (i32, i32),
        log: &std::path::Path,
        uris: &[&str],
    ) -> std::process::Child {
        use std::io::BufRead;
        let mut child = std::process::Command::new("python3")
            .args(["-c", PARTNER, role, &at.0.to_string(), &at.1.to_string()])
            .arg(log)
            .args(uris)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("python3 with GTK");
        let mut line = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert_eq!(line.trim(), "ready");
        child
    }

    #[test]
    #[ignore = "drags between this program and a Python GTK one on a nested X server: DISPLAY=:97 OPENBAR_LIVE_DND=:97"]
    fn live_files_are_dragged_in_from_another_program_and_out_to_one() {
        let display = std::env::var("OPENBAR_LIVE_DND").expect("OPENBAR_LIVE_DND");
        assert_eq!(std::env::var("DISPLAY").as_deref(), Ok(display.as_str()));
        assert!(
            display != ":0" && !display.starts_with(":0."),
            "refusing {display}: the real desktop is the person's own"
        );
        gtk::init().expect("GTK on the nested X server");
        let dir = std::env::temp_dir().join(format!("openbar-dnd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("received.txt");
        let files = [
            format!("file://{}/a%20b.txt", dir.display()),
            format!("file://{}/%C5%82.txt", dir.display()),
        ];
        let uris: Vec<&str> = files.iter().map(String::as_str).collect();

        // This program's window, at (400, 0), with the island's kind of drop on it.
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_default_size(300, 200);
        window.move_(400, 0);
        let area = gtk::EventBox::new();
        window.add(&area);
        let heard: Rc<RefCell<Vec<String>>> = Rc::default();
        let entry = gtk::TargetEntry::new(URI_LIST, gtk::TargetFlags::empty(), 1);
        area.drag_dest_set(gtk::DestDefaults::empty(), &[entry], DragAction::COPY);
        let listener = heard.clone();
        accept_drops(
            area.upcast_ref(),
            || true,
            |path| Some(format!("id:{path}")),
            move |drag| {
                listener.borrow_mut().push(match drag {
                    Drag::Enter => "enter".to_string(),
                    Drag::Leave => "leave".to_string(),
                    Drag::Drop(dropped) => format!(
                        "drop {}",
                        dropped
                            .paths
                            .iter()
                            .map(|(path, id)| format!("{path}={}", id.clone().unwrap_or_default()))
                            .collect::<Vec<_>>()
                            .join(" ")
                    ),
                });
            },
        );
        window.show_all();
        pump(300);

        // In: the other program's window at (0, 0) is dragged onto this one.
        let mut source = partner("source", (0, 0), &log, &uris);
        pump(500);
        xdotool(&["mousemove", "150", "100"]);
        pump(100);
        xdotool(&["mousedown", "1"]);
        pump(100);
        glide((150, 100), (550, 100));
        pump(200);
        xdotool(&["mouseup", "1"]);
        let dropped = pump_until(5000, || {
            heard.borrow().iter().any(|e| e.starts_with("drop"))
        });
        let _ = source.kill();
        let _ = source.wait();
        let events = heard.borrow().clone();
        assert!(dropped, "nothing was dropped: {events:?}");
        assert_eq!(
            events.first().map(String::as_str),
            Some("enter"),
            "{events:?}"
        );
        let expected = format!(
            "drop {d}/a b.txt=id:{d}/a b.txt {d}/ł.txt=id:{d}/ł.txt",
            d = dir.display()
        );
        assert_eq!(events.last(), Some(&expected), "{events:?}");
        assert!(
            !events.contains(&"leave".to_string()),
            "a drop is not a leave: {events:?}"
        );

        // Out: this program starts a drag of the same files over a window at (0, 300).
        let mut target = partner("target", (0, 300), &log, &[]);
        pump(500);
        window.move_(400, 300);
        pump(300);
        xdotool(&["mousemove", "550", "400"]);
        pump(100);
        xdotool(&["mousedown", "1"]);
        pump(150);
        let took: Rc<Cell<Option<bool>>> = Rc::default();
        let verdict = took.clone();
        assert!(begin_drag(
            window.upcast_ref(),
            Payload {
                uris: files.to_vec(),
                text: None,
            },
            DragAction::COPY | DragAction::MOVE,
            move |taken| verdict.set(Some(taken)),
        ));
        pump(150);
        glide((550, 400), (150, 400));
        pump(200);
        xdotool(&["mouseup", "1"]);
        let finished = pump_until(5000, || took.get().is_some());
        pump(300);
        let _ = target.kill();
        let _ = target.wait();
        assert!(finished, "the drag never ended");
        assert_eq!(took.get(), Some(true), "the other program took the files");
        let received = std::fs::read_to_string(&log).unwrap_or_default();
        assert_eq!(
            received.lines().collect::<Vec<_>>(),
            uris,
            "the files, as they were offered"
        );

        // And text: the same, with the words in place of the files.
        let mut target = partner("target", (0, 300), &log, &[]);
        pump(500);
        xdotool(&["mousemove", "550", "400"]);
        pump(100);
        xdotool(&["mousedown", "1"]);
        pump(150);
        let took: Rc<Cell<Option<bool>>> = Rc::default();
        let verdict = took.clone();
        assert!(begin_drag(
            window.upcast_ref(),
            Payload {
                uris: Vec::new(),
                text: Some("Remember the milk".to_string()),
            },
            DragAction::COPY,
            move |taken| verdict.set(Some(taken)),
        ));
        pump(150);
        glide((550, 400), (150, 400));
        pump(200);
        xdotool(&["mouseup", "1"]);
        let finished = pump_until(5000, || took.get().is_some());
        pump(300);
        let _ = target.kill();
        let _ = target.wait();
        assert!(finished, "the text drag never ended");
        assert_eq!(took.get(), Some(true), "the other program took the text");
        assert_eq!(
            std::fs::read_to_string(&log).unwrap_or_default(),
            "text:Remember the milk"
        );

        // Our own drag, let go over our own drop target, is refused and fails.
        heard.borrow_mut().clear();
        window.move_(400, 300);
        pump(300);
        xdotool(&["mousemove", "500", "350"]);
        pump(100);
        xdotool(&["mousedown", "1"]);
        pump(150);
        let took: Rc<Cell<Option<bool>>> = Rc::default();
        let verdict = took.clone();
        assert!(begin_drag(
            window.upcast_ref(),
            Payload {
                uris: files.to_vec(),
                text: None,
            },
            DragAction::COPY,
            move |taken| verdict.set(Some(taken)),
        ));
        pump(150);
        glide((500, 350), (600, 450));
        pump(200);
        xdotool(&["mouseup", "1"]);
        let finished = pump_until(5000, || took.get().is_some());
        assert!(finished, "the drag never ended");
        assert_eq!(
            took.get(),
            Some(false),
            "the island does not take its own tiles back"
        );
        assert!(
            heard
                .borrow()
                .iter()
                .all(|event| !event.starts_with("drop")),
            "{:?}",
            heard.borrow()
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_drop_is_something_when_it_has_a_file_a_link_or_text_with_words() {
        assert!(!is_something(&Dropped::default()));
        assert!(is_something(&Dropped {
            paths: vec![("/a".into(), None)],
            ..Default::default()
        }));
        assert!(is_something(&Dropped {
            url: Some("https://a.example/".into()),
            ..Default::default()
        }));
        assert!(is_something(&Dropped {
            text: Some("hello".into()),
            ..Default::default()
        }));
        assert!(!is_something(&Dropped {
            text: Some("  \n".into()),
            ..Default::default()
        }));
    }
}
