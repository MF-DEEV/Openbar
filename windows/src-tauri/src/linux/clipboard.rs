// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! The clipboard on Linux (X11), where Windows has `native.rs` over the Win32
//! clipboard: GTK's clipboard, which tells when its owner changes and
//! converts pictures and text. `clipboard/mod.rs` includes this file as
//! `native`, and everything GTK is done on its main thread (`gtk_main`).
//!
//! What a copy holds is read richest first, as on Windows: files
//! (`text/uri-list`), text, then a picture. A copy a program marks as secret
//! (the `x-kde-passwordManagerHint` KeePassXC and others set) is left out.
//! The app a copy came from is the program that owns the clipboard, else the
//! one in front. Paste types Ctrl+V (Ctrl+Shift+V in a terminal) into the
//! window in front through the X server's test extension.

use std::ffi::CString;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use gtk::gdk::{self, Atom};
use gtk::gdk_pixbuf::Pixbuf;
use gtk::gio::{self, prelude::*};
use gtk::glib::translate::ToGlibPtr;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    self, AtomEnum, ConnectionExt as _, GetPropertyReply, KEY_PRESS_EVENT, KEY_RELEASE_EVENT,
    Window,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

use super::history::{Content, MAX_FILES, MAX_IMAGE_BYTES};
use crate::linux::gtk_main::on_main;

/// How long a read waits for the program that owns the clipboard to answer.
const ANSWER: Duration = Duration::from_secs(4);
const MAX_TEXT_BYTES: usize = 8 * 1024 * 1024;

/// What KDE's password managers (KeePassXC, Bitwarden) put on a secret copy.
const PRIVATE_TARGET: &str = "x-kde-passwordManagerHint";

/// A change worth reading, with who made it as far as X can tell.
pub struct Update {
    /// Openbar's own write (an OCR result, a color): recorded, with no app named.
    pub own: bool,
    /// The program the copy came from: the clipboard's owner, else the window in front.
    pub exe: Option<String>,
    /// Every program that held the front since the last change.
    pub candidates: Vec<String>,
}

/// Writes of our own the listener has still to hear: those from the history
/// are not recorded again, the others are recorded with no app.
static OWN_WRITES: AtomicU32 = AtomicU32::new(0);
static HISTORY_WRITES: AtomicU32 = AtomicU32::new(0);

fn take_one(counter: &AtomicU32) -> bool {
    counter
        .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
        .is_ok()
}

fn clipboard() -> gtk::Clipboard {
    gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD)
}

// MARK: What a copy holds, from the targets it offers

/// Whether a copy offers `target`, in any spelling of its charset.
fn offers(targets: &[String], wanted: &str) -> bool {
    targets.iter().any(|t| t.eq_ignore_ascii_case(wanted))
}

fn offers_text(targets: &[String]) -> bool {
    [
        "UTF8_STRING",
        "text/plain;charset=utf-8",
        "text/plain",
        "STRING",
    ]
    .iter()
    .any(|t| offers(targets, t))
}

/// Paths from a `text/uri-list` (or the file manager's `x-special/gnome-copied-files`,
/// whose first line is the action): local files only.
fn paths_from_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| l.starts_with("file://"))
        .filter_map(|uri| gio::File::for_uri(uri).path())
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

fn atom_names(targets: Vec<Atom>) -> Vec<String> {
    targets.iter().map(|a| a.name().to_string()).collect()
}

/// A picture as the PNG the history keeps.
fn content_of_pixbuf(pixbuf: &Pixbuf) -> Option<Content> {
    let png = pixbuf.save_to_bufferv("png", &[]).ok()?;
    if png.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let (width, height) = crate::clipboard::formats::png_dimensions(&png)?;
    Some(Content::Image {
        hash: crate::clipboard::formats::sha256_hex(&png),
        png,
        width,
        height,
    })
}

/// Starts reading what the clipboard holds, and sends it to `reply` when it
/// has arrived: the main loop has to run for the owner's answers to come in.
fn start_read(include_images_files: bool, reply: mpsc::Sender<Option<Content>>) {
    let board = clipboard();
    board.request_contents(&Atom::intern("TARGETS"), move |board, data| {
        let targets = data.targets().map(atom_names).unwrap_or_default();
        if offers(&targets, PRIVATE_TARGET) {
            let _ = reply.send(None);
            return;
        }
        if include_images_files && offers(&targets, "text/uri-list") {
            let reply = reply.clone();
            let (with_text, with_picture) = (offers_text(&targets), offers(&targets, "image/png"));
            board.request_contents(&Atom::intern("text/uri-list"), move |board, data| {
                let paths = std::str::from_utf8(&data.data())
                    .map(paths_from_list)
                    .unwrap_or_default();
                if !paths.is_empty() && paths.len() <= MAX_FILES {
                    let _ = reply.send(Some(Content::Files(paths)));
                } else {
                    // Not files after all (a remote location): what else it offers.
                    read_text_or_picture(
                        board,
                        with_text,
                        with_picture && include_images_files,
                        reply,
                    );
                }
            });
            return;
        }
        read_text_or_picture(
            board,
            offers_text(&targets),
            include_images_files && offers(&targets, "image/png"),
            reply,
        );
    });
}

fn read_text_or_picture(
    board: &gtk::Clipboard,
    text: bool,
    picture: bool,
    reply: mpsc::Sender<Option<Content>>,
) {
    if text {
        board.request_text(move |board, text| match text {
            Some(text) if text.len() <= MAX_TEXT_BYTES => {
                let _ = reply.send(Some(Content::Text(text.to_string())));
            }
            _ if picture => read_picture(board, reply),
            _ => {
                let _ = reply.send(None);
            }
        });
    } else if picture {
        read_picture(board, reply);
    } else {
        let _ = reply.send(None);
    }
}

fn read_picture(board: &gtk::Clipboard, reply: mpsc::Sender<Option<Content>>) {
    board.request_image(move |_, pixbuf| {
        let _ = reply.send(pixbuf.and_then(content_of_pixbuf));
    });
}

/// The content the history keeps, or None for nothing, a secret, or what
/// does not arrive in time.
pub fn read(include_images_files: bool) -> Option<Content> {
    let (tx, rx) = mpsc::channel();
    on_main(move || start_read(include_images_files, tx))?;
    rx.recv_timeout(ANSWER).ok().flatten()
}

// MARK: Writing

pub enum Payload {
    Text(String),
    /// PNG bytes.
    Image(Vec<u8>),
    Files(Vec<String>),
}

struct Offered {
    /// `text/uri-list` for most programs, and what GNOME's file manager and
    /// those that follow it read, which also says whether it is a copy or a cut.
    uris: Vec<String>,
}

unsafe extern "C" fn provide(
    _: *mut gtk::ffi::GtkClipboard,
    selection: *mut gtk::ffi::GtkSelectionData,
    info: u32,
    owned: gtk::glib::ffi::gpointer,
) {
    // SAFETY: `owned` is the `Offered` handed to GTK below, kept until `release`.
    let offered = unsafe { &*(owned as *const Offered) };
    let (target, text) = if info == 0 {
        ("text/uri-list", offered.uris.join("\r\n") + "\r\n")
    } else {
        (
            "x-special/gnome-copied-files",
            format!("copy\n{}", offered.uris.join("\n")),
        )
    };
    let atom = Atom::intern(target);
    // SAFETY: `selection` is the data GTK gave this call, and the bytes are copied.
    unsafe {
        gtk::ffi::gtk_selection_data_set(
            selection,
            atom.to_glib_none().0,
            8,
            text.as_ptr(),
            text.len() as i32,
        );
    }
}

unsafe extern "C" fn release(_: *mut gtk::ffi::GtkClipboard, owned: gtk::glib::ffi::gpointer) {
    // SAFETY: the box made in `put_files`, given back when the clipboard moves on.
    drop(unsafe { Box::from_raw(owned as *mut Offered) });
}

/// The files as a copy: offered under the two names file managers read.
fn put_files(board: &gtk::Clipboard, paths: &[String]) -> bool {
    let uris = paths
        .iter()
        .map(|p| gio::File::for_path(p).uri().to_string())
        .collect();
    let names = [
        CString::new("text/uri-list").unwrap(),
        CString::new("x-special/gnome-copied-files").unwrap(),
    ];
    let targets: Vec<gtk::ffi::GtkTargetEntry> = names
        .iter()
        .enumerate()
        .map(|(info, name)| gtk::ffi::GtkTargetEntry {
            target: name.as_ptr() as *mut _,
            flags: 0,
            info: info as u32,
        })
        .collect();
    let owned = Box::into_raw(Box::new(Offered { uris }));
    // SAFETY: the target names live through the call (GTK copies them), and
    // `owned` is freed by `release` when the clipboard is taken over, or here if it was refused.
    let accepted = unsafe {
        gtk::ffi::gtk_clipboard_set_with_data(
            board.to_glib_none().0,
            targets.as_ptr(),
            targets.len() as u32,
            Some(provide),
            Some(release),
            owned as *mut _,
        )
    } != 0;
    if !accepted {
        // SAFETY: GTK did not keep `owned`, so it is still ours.
        drop(unsafe { Box::from_raw(owned) });
    }
    accepted
}

/// Puts `payload` on the clipboard. A write from the history is not recorded
/// again by the listener; one of Openbar's own is, with no app named.
pub fn write(payload: &Payload, from_history: bool) -> bool {
    let counter = if from_history {
        &HISTORY_WRITES
    } else {
        &OWN_WRITES
    };
    counter.fetch_add(1, Ordering::SeqCst);
    let payload = match payload {
        Payload::Text(text) => Payload::Text(text.clone()),
        Payload::Image(png) => Payload::Image(png.clone()),
        Payload::Files(paths) => Payload::Files(paths.clone()),
    };
    let done = on_main(move || {
        let board = clipboard();
        match &payload {
            Payload::Text(text) => {
                board.set_text(text);
                true
            }
            Payload::Files(paths) => put_files(&board, paths),
            // The picture is made here, for a pixbuf belongs to the thread it is made on.
            Payload::Image(png) => match Pixbuf::from_read(std::io::Cursor::new(png.clone())) {
                Ok(pixbuf) => {
                    board.set_image(&pixbuf);
                    true
                }
                Err(_) => false,
            },
        }
    })
    .unwrap_or(false);
    if !done {
        // Nothing was set, so no change will be heard to count this one.
        take_one(counter);
    }
    done
}

// MARK: Listening, and who copied

/// The window and program that own the clipboard and the one in front.
fn owner_and_front() -> (Option<String>, Option<String>) {
    let Ok((connection, screen)) = RustConnection::connect(None) else {
        return (None, None);
    };
    let Some(root) = connection.setup().roots.get(screen).map(|s| s.root) else {
        return (None, None);
    };
    let atom = |name: &str| -> Option<xproto::Atom> {
        Some(
            connection
                .intern_atom(false, name.as_bytes())
                .ok()?
                .reply()
                .ok()?
                .atom,
        )
    };
    let program = |window: Window| -> Option<String> {
        let pid_atom = atom("_NET_WM_PID")?;
        let reply: GetPropertyReply = connection
            .get_property(false, window, pid_atom, AtomEnum::CARDINAL, 0, 1)
            .ok()?
            .reply()
            .ok()?;
        let pid = reply.value32()?.next()?;
        crate::linux::process::image_path(pid).map(|p| p.to_string_lossy().into_owned())
    };
    let owner = atom("CLIPBOARD")
        .and_then(|selection| connection.get_selection_owner(selection).ok()?.reply().ok())
        .map(|r| r.owner)
        .filter(|w| *w != 0)
        .and_then(program);
    let front = atom("_NET_ACTIVE_WINDOW")
        .and_then(|active| {
            connection
                .get_property(false, root, active, AtomEnum::WINDOW, 0, 1)
                .ok()?
                .reply()
                .ok()
        })
        .and_then(|reply| reply.value32()?.next())
        .filter(|w| *w != 0)
        .and_then(program);
    (owner, front)
}

/// Calls `on_update` for each change of the clipboard, on GTK's thread.
pub fn listen(on_update: impl Fn(Update) + Send + Sync + 'static) {
    on_main(move || {
        let _ = clipboard().connect_local("owner-change", false, move |_| {
            if take_one(&HISTORY_WRITES) {
                return None;
            }
            let own = take_one(&OWN_WRITES);
            let (owner, front) = if own { (None, None) } else { owner_and_front() };
            on_update(Update {
                own,
                exe: owner.clone().or_else(|| front.clone()),
                candidates: [owner, front].into_iter().flatten().collect(),
            });
            None
        });
    });
}

// MARK: Pasting

/// The window in front, which a paste goes to: not one of ours.
pub fn paste_target(_island_previous: isize) -> Option<isize> {
    let (connection, screen) = RustConnection::connect(None).ok()?;
    let root = connection.setup().roots.get(screen)?.root;
    let active = connection
        .intern_atom(false, b"_NET_ACTIVE_WINDOW")
        .ok()?
        .reply()
        .ok()?
        .atom;
    let window = connection
        .get_property(false, root, active, AtomEnum::WINDOW, 0, 1)
        .ok()?
        .reply()
        .ok()?
        .value32()?
        .next()
        .filter(|w| *w != 0)?;
    Some(window as isize)
}

/// X has no elevated windows to refuse input.
pub fn blocks_input(_target: isize) -> bool {
    false
}

/// Whether a window of this class takes Ctrl+Shift+V for paste: terminals,
/// where Ctrl+V means something else.
fn is_terminal(class: &str) -> bool {
    const TERMINALS: [&str; 18] = [
        "xfce4-terminal",
        "gnome-terminal",
        "konsole",
        "alacritty",
        "kitty",
        "wezterm",
        "xterm",
        "urxvt",
        "rxvt",
        "terminator",
        "tilix",
        "foot",
        "lxterminal",
        "mate-terminal",
        "qterminal",
        "guake",
        "yakuake",
        "ghostty",
    ];
    let class = class.to_lowercase();
    TERMINALS.iter().any(|t| class.contains(t))
}

fn keycode(connection: &RustConnection, keysym: u32) -> Option<u8> {
    let setup = connection.setup();
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let map = connection
        .get_keyboard_mapping(min, max - min + 1)
        .ok()?
        .reply()
        .ok()?;
    let per = usize::from(map.keysyms_per_keycode);
    map.keysyms
        .chunks(per)
        .position(|syms| syms.contains(&keysym))
        .map(|i| min + i as u8)
}

/// Types the paste shortcut into the window in front, through XTEST.
pub fn send_paste(target: isize) -> bool {
    let Ok((connection, screen)) = RustConnection::connect(None) else {
        return false;
    };
    let Some(root) = connection.setup().roots.get(screen).map(|s| s.root) else {
        return false;
    };
    // The class of the window decides whether Shift goes with it.
    let class = connection
        .get_property(
            false,
            target as Window,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            0,
            256,
        )
        .ok()
        .and_then(|c| c.reply().ok())
        .map(|r| String::from_utf8_lossy(&r.value).into_owned())
        .unwrap_or_default();
    let (Some(control), Some(v)) = (keycode(&connection, 0xffe3), keycode(&connection, 0x76))
    else {
        return false;
    };
    let shift = is_terminal(&class)
        .then(|| keycode(&connection, 0xffe1))
        .flatten();
    let mut keys = vec![control];
    keys.extend(shift);
    let fake = |kind: u8, code: u8| {
        connection
            .xtest_fake_input(kind, code, 0, root, 0, 0, 0)
            .is_ok()
    };
    let pressed = keys.iter().all(|k| fake(KEY_PRESS_EVENT, *k));
    let typed = pressed && fake(KEY_PRESS_EVENT, v) && fake(KEY_RELEASE_EVENT, v);
    let released = keys.iter().rev().all(|k| fake(KEY_RELEASE_EVENT, *k));
    typed && released && connection.flush().is_ok()
}

/// The X server's bell, for a paste that had nowhere to go.
pub fn beep() {
    if let Ok((connection, _)) = RustConnection::connect(None) {
        let _ = connection.bell(0);
        let _ = connection.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn what_a_copy_offers_is_matched_in_any_spelling_of_its_charset() {
        let targets = names(&["TARGETS", "text/plain;charset=UTF-8", "text/html"]);
        assert!(offers_text(&targets));
        assert!(!offers_text(&names(&["image/png"])));
        assert!(offers(&names(&["Text/URI-List"]), "text/uri-list"));
        assert!(offers(
            &names(&["x-kde-passwordManagerHint"]),
            PRIVATE_TARGET
        ));
        assert!(!offers(&names(&["UTF8_STRING"]), PRIVATE_TARGET));
        assert!(offers_text(&names(&["STRING"])));
    }

    #[test]
    fn a_file_copy_gives_local_paths_decoded_and_ignores_the_rest() {
        let list = "file:///home/me/My%20Doc.txt\r\nfile:///tmp/a%C3%A9.png\r\nhttp://example.com/x\r\n# a comment\r\n";
        assert_eq!(
            paths_from_list(list),
            ["/home/me/My Doc.txt", "/tmp/aé.png"]
        );
        // The file manager's own form: the action, then the same list.
        assert_eq!(
            paths_from_list("copy\nfile:///a\nfile:///b c"),
            ["/a", "/b c"]
        );
        assert!(paths_from_list("copy\nhttp://x\n").is_empty());
        assert!(paths_from_list("").is_empty());
    }

    #[test]
    fn terminals_paste_with_shift_and_other_windows_do_not() {
        for terminal in [
            "xfce4-terminal",
            "Xfce4-terminal",
            "kitty",
            "Alacritty",
            "org.wezfurlong.wezterm",
            "URxvt",
        ] {
            assert!(is_terminal(terminal), "{terminal}");
        }
        for other in ["firefox", "Navigator", "code", "", "Thunar"] {
            assert!(!is_terminal(other), "{other}");
        }
    }

    #[test]
    fn writes_of_our_own_are_counted_off_as_they_are_heard() {
        let counter = AtomicU32::new(0);
        assert!(!take_one(&counter), "nothing was written, nothing to hear");
        counter.fetch_add(2, Ordering::SeqCst);
        assert!(take_one(&counter));
        assert!(take_one(&counter));
        assert!(!take_one(&counter));
    }

    /// Lets GTK's main loop run for `ms`: the clipboard's answers come through it.
    fn pump(ms: u64) {
        let until = std::time::Instant::now() + Duration::from_millis(ms);
        while std::time::Instant::now() < until {
            while gtk::events_pending() {
                gtk::main_iteration();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// `read` on its own thread, as the capture loop does, while the loop runs here.
    fn read_while_pumping(include: bool) -> Option<Content> {
        let reader = std::thread::spawn(move || read(include));
        while !reader.is_finished() {
            pump(10);
        }
        reader.join().unwrap()
    }

    #[test]
    #[ignore = "uses the clipboard of DISPLAY: only a nested X server, never the desktop's own"]
    fn the_clipboard_of_a_nested_x_server_is_written_read_and_heard() {
        let display = std::env::var("DISPLAY").unwrap_or_default();
        assert!(
            !display.is_empty() && !display.starts_with(":0"),
            "refusing to use the clipboard of {display:?}: use a nested server such as Xephyr on :97"
        );
        gtk::init().expect("GTK needs a display");
        let heard =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::<(bool, Option<String>)>::new()));
        let sink = heard.clone();
        listen(move |update| sink.lock().unwrap().push((update.own, update.exe)));
        // `listen` hands its work to GTK's thread, which is this one.
        pump(200);
        let take = || std::mem::take(&mut *heard.lock().unwrap());

        // Text of our own: heard once as ours, and read back.
        assert!(write(&Payload::Text("hello".into()), false));
        pump(300);
        assert_eq!(take(), [(true, None)]);
        assert_eq!(
            read_while_pumping(true),
            Some(Content::Text("hello".into()))
        );
        assert_eq!(
            read_while_pumping(false),
            Some(Content::Text("hello".into()))
        );

        // From the history: read back, and not heard.
        assert!(write(&Payload::Text("again é".into()), true));
        pump(300);
        assert!(
            take().is_empty(),
            "a history copy must not be recorded again"
        );
        assert_eq!(
            read_while_pumping(true),
            Some(Content::Text("again é".into()))
        );

        // Files, under both names a file manager reads.
        let dir = std::env::temp_dir().join(format!("openbar-clip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let files = vec![
            dir.join("a.txt").to_string_lossy().into_owned(),
            dir.join("b c.txt").to_string_lossy().into_owned(),
        ];
        assert!(write(&Payload::Files(files.clone()), true));
        pump(300);
        assert_eq!(
            read_while_pumping(true),
            Some(Content::Files(files.clone()))
        );
        // With pictures and files left out, a file copy is no text either.
        assert_eq!(read_while_pumping(false), None);

        // A picture.
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 3, 2);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[200u8; 3 * 2 * 4]).unwrap();
        }
        assert!(write(&Payload::Image(png), true));
        pump(300);
        match read_while_pumping(true) {
            Some(Content::Image { width, height, .. }) => assert_eq!((width, height), (3, 2)),
            other => panic!("not the picture: {other:?}"),
        }
        assert_eq!(read_while_pumping(false), None);
        // A picture that is not one is refused, and not counted as a write to hear.
        assert!(!write(&Payload::Image(b"not a png".to_vec()), false));

        // Another program's copy is heard as not ours, and read.
        take();
        let mut xclip = std::process::Command::new("xclip")
            .args(["-selection", "clipboard", "-i"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .expect("xclip");
        std::io::Write::write_all(xclip.stdin.as_mut().unwrap(), b"from xclip").unwrap();
        drop(xclip.stdin.take());
        pump(500);
        let others = take();
        println!("heard from outside: {others:?}");
        assert!(others.iter().any(|(own, _)| !own), "{others:?}");
        assert_eq!(
            read_while_pumping(true),
            Some(Content::Text("from xclip".into()))
        );
        let _ = xclip.kill();
        let _ = xclip.wait();

        // The paste shortcut is accepted by the server's test extension.
        let root = RustConnection::connect(None)
            .map(|(c, s)| c.setup().roots[s].root)
            .unwrap();
        assert!(send_paste(root as isize));
        beep();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
