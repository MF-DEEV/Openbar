// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! Names, icons and thumbnails on Linux, where Windows asks its shell: the
//! icon theme and the desktop entries supply them. `shell.rs` includes this
//! file as `imp`.
//!
//! An icon is `path`'s own (a file's type, an executable's program), or, for
//! `icon:<name>`, the theme's icon of that name, which is how the mixer asks
//! for an app's. Drawing an icon is GTK's, which only its main thread may do,
//! so the work is handed there and waited for.

use std::path::Path;
use std::time::Duration;

use gtk::gdk_pixbuf::Pixbuf;
use gtk::gio::{self, prelude::*};
use gtk::prelude::*;
use gtk::{IconLookupFlags, IconTheme, glib};

/// A picture of the largest this will decode, for a thumbnail.
const THUMBNAIL_LIMIT: u64 = 64 * 1024 * 1024;

/// Programs that show what runs, in the order they are tried.
const TASK_MANAGERS: [&str; 7] = [
    "xfce4-taskmanager",
    "gnome-system-monitor",
    "plasma-systemmonitor",
    "mate-system-monitor",
    "lxtask",
    "deepin-system-monitor",
    "qps",
];

/// What an icon name may hold; a path or anything odder is not a name.
fn icon_name(name: &str) -> Option<&str> {
    let valid = !name.is_empty()
        && name.len() <= 128
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'));
    valid.then_some(name)
}

/// Runs `job` on the thread GTK may be used from, and waits for it.
fn on_main<T: Send + 'static>(job: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    glib::MainContext::default().invoke(move || {
        let _ = tx.send(job());
    });
    rx.recv_timeout(Duration::from_secs(5)).ok()
}

fn themed(name: &str, size: u32) -> Option<Pixbuf> {
    IconTheme::default()?
        .load_icon(icon_name(name)?, size as i32, IconLookupFlags::FORCE_SIZE)
        .ok()
        .flatten()
}

/// The icon the desktop shows for a file or folder: by its type, or for an
/// executable by its program's name when the theme has one.
fn file_icon(path: &str, size: u32) -> Option<Pixbuf> {
    let info = gio::File::for_path(path)
        .query_info(
            "standard::icon,standard::content-type,standard::type",
            gio::FileQueryInfoFlags::NONE,
            gio::Cancellable::NONE,
        )
        .ok()?;
    if info.file_type() == gio::FileType::Regular
        && let Some(stem) = Path::new(path).file_stem().and_then(|s| s.to_str())
        && info
            .content_type()
            .is_some_and(|t| t.contains("executable") || t.contains("sharedlib"))
        && let Some(pixbuf) = themed(stem, size)
    {
        return Some(pixbuf);
    }
    let icon = info.icon()?;
    IconTheme::default()?
        .lookup_by_gicon(&icon, size as i32, IconLookupFlags::FORCE_SIZE)?
        .load_icon()
        .ok()
}

/// A file's picture fit into `width` x `height`: the desktop's own thumbnail
/// if it made one, else the picture itself when it is one.
fn thumbnail(path: &str, width: u32, height: u32) -> Option<Pixbuf> {
    let file = gio::File::for_path(path);
    let info = file
        .query_info(
            "thumbnail::path,thumbnail::is-valid,standard::content-type,standard::size",
            gio::FileQueryInfoFlags::NONE,
            gio::Cancellable::NONE,
        )
        .ok()?;
    let fit =
        |from: &Path| Pixbuf::from_file_at_scale(from, width as i32, height as i32, true).ok();
    // The desktop marks a thumbnail it made of an older version of the file as not valid.
    let current = !info.has_attribute("thumbnail::is-valid") || info.boolean("thumbnail::is-valid");
    if let Some(made) = info
        .attribute_byte_string("thumbnail::path")
        .filter(|_| current)
        && let Some(pixbuf) = fit(Path::new(made.as_str()))
    {
        return Some(pixbuf);
    }
    let picture = info.content_type().is_some_and(|t| t.starts_with("image/"))
        && (info.size() as u64) <= THUMBNAIL_LIMIT;
    picture.then(|| fit(Path::new(path))).flatten()
}

/// The picture, as PNG bytes. GTK's main thread only.
fn render(path: &str, width: u32, height: u32, icon_only: bool) -> Option<Vec<u8>> {
    let size = width.min(height).max(1);
    let pixbuf = if let Some(name) = path.strip_prefix("icon:") {
        themed(name, size)
    } else if icon_only {
        file_icon(path, size)
    } else {
        thumbnail(path, width, height).or_else(|| file_icon(path, size))
    }?;
    pixbuf.save_to_bufferv("png", &[]).ok()
}

pub fn image(path: &str, width: u32, height: u32, icon_only: bool) -> Option<Vec<u8>> {
    let path = path.to_string();
    on_main(move || render(&path, width, height, icon_only)).flatten()
}

/// The program a command line starts, past an `env` and the variables it sets
/// (a desktop entry may be `env GTK_USE_PORTAL=1 /usr/lib/firefox/firefox %u`).
fn command_program(words: &[String]) -> Option<&str> {
    words
        .iter()
        .map(String::as_str)
        .find(|word| *word != "env" && (word.starts_with('/') || !word.contains('=')))
}

/// The name of the desktop entry whose program is `exe`'s file name, to name a process by.
pub fn file_description(exe: &str) -> Option<String> {
    let wanted = Path::new(exe).file_name()?.to_str()?;
    gio::AppInfo::all()
        .into_iter()
        .filter(|info| info.should_show())
        .find(|info| {
            let words: Vec<String> = info
                .commandline()
                .and_then(|line| glib::shell_parse_argv(line.to_str()?).ok())
                .map(|words| {
                    words
                        .iter()
                        .map(|w| w.to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            command_program(&words)
                .and_then(|program| Path::new(program).file_name())
                .is_some_and(|name| name == wanted)
        })
        .map(|info| info.name().to_string())
        .filter(|name| !name.is_empty())
}

pub fn process_path(pid: u32) -> Option<String> {
    let path = crate::linux::process::image_path(pid)?;
    // A program replaced while it runs reads "/usr/bin/x (deleted)".
    let path = path.to_string_lossy();
    Some(path.strip_suffix(" (deleted)").unwrap_or(&path).to_string())
}

/// What the desktop calls the file's type ("PNG image").
pub fn type_name(path: &str) -> Option<String> {
    let info = gio::File::for_path(path)
        .query_info(
            "standard::content-type",
            gio::FileQueryInfoFlags::NONE,
            gio::Cancellable::NONE,
        )
        .ok()?;
    let kind = info.content_type()?;
    let name = gio::content_type_get_description(&kind);
    (!name.is_empty()).then(|| name.to_string())
}

/// The desktop's system monitor, started; false when there is none.
pub fn open_task_manager() -> bool {
    use std::process::{Command, Stdio};
    TASK_MANAGERS.iter().any(|program| {
        match Command::new(program)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(mut child) => {
                // Reaped when it ends, so it does not linger as a zombie.
                std::thread::spawn(move || child.wait());
                true
            }
            Err(_) => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_name_is_an_icon_name() {
        for good in [
            "spotify-client",
            "org.gnome.Rhythmbox3",
            "audio-x-generic",
            "a_b+c",
        ] {
            assert_eq!(icon_name(good), Some(good));
        }
        for bad in [
            "",
            "/etc/passwd",
            "../x",
            "a b",
            "a/b",
            "icon:x",
            &"x".repeat(129),
        ] {
            assert_eq!(icon_name(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_command_line_names_its_program_past_env_and_variables() {
        let words = |line: &[&str]| line.iter().map(|w| w.to_string()).collect::<Vec<_>>();
        let program = |line: &[&str]| command_program(&words(line)).map(str::to_string);
        assert_eq!(
            program(&["env", "GTK_USE_PORTAL=1", "/usr/lib/firefox/firefox", "%u"]).as_deref(),
            Some("/usr/lib/firefox/firefox")
        );
        assert_eq!(
            program(&["spotify", "--uri=%u"]).as_deref(),
            Some("spotify")
        );
        assert_eq!(
            program(&["/opt/a=b/run", "x"]).as_deref(),
            Some("/opt/a=b/run")
        );
        assert_eq!(program(&["env", "A=1"]), None);
        assert_eq!(program(&[]), None);
    }

    #[test]
    fn a_process_is_found_by_its_pid_and_a_vanished_one_is_not() {
        let own = process_path(std::process::id()).unwrap();
        assert!(Path::new(&own).is_absolute() && !own.ends_with("(deleted)"));
        assert_eq!(process_path(u32::MAX), None);
    }

    #[test]
    fn a_file_type_is_named() {
        let dir = std::env::temp_dir().join(format!("openbar-shell-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let text = dir.join("note.txt");
        std::fs::write(&text, "hello").unwrap();
        let name = type_name(&text.to_string_lossy()).expect("a type name");
        assert!(name.to_lowercase().contains("text"), "{name}");
        assert_eq!(type_name("/nonexistent/file"), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    // Drawing needs GTK on a display: run with DISPLAY set and --ignored.
    #[test]
    #[ignore = "needs a display (X server) for GTK"]
    fn icons_and_thumbnails_are_drawn_as_png() {
        gtk::init().expect("GTK needs a display");
        let png =
            |bytes: Option<Vec<u8>>| bytes.filter(|b| b.starts_with(&[0x89, b'P', b'N', b'G']));
        // The theme's own icon by name, at the size asked.
        let by_name = png(render("icon:folder", 48, 48, true)).expect("icon:folder");
        println!("icon:folder is {} bytes", by_name.len());
        assert!(png(render("icon:no-such-icon-anywhere", 48, 48, true)).is_none());
        assert!(png(render("icon:/etc/passwd", 48, 48, true)).is_none());
        // A folder, a text file, and a program by its name.
        let dir = std::env::temp_dir().join(format!("openbar-icons-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let text = dir.join("note.txt");
        std::fs::write(&text, "hello").unwrap();
        assert!(
            png(render(&dir.to_string_lossy(), 32, 32, true)).is_some(),
            "folder"
        );
        assert!(
            png(render(&text.to_string_lossy(), 32, 32, true)).is_some(),
            "text file"
        );
        println!(
            "/usr/bin/env: {:?}",
            png(render("/usr/bin/env", 32, 32, true)).map(|b| b.len())
        );
        assert!(
            png(render("/usr/bin/env", 32, 32, true)).is_some(),
            "program"
        );
        // A picture's thumbnail is the picture, fit into the box.
        let picture = dir.join("pic.png");
        let mut data = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut data, 64, 32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&vec![200u8; 64 * 32 * 4]).unwrap();
        }
        std::fs::write(&picture, data).unwrap();
        let thumb = render(&picture.to_string_lossy(), 16, 16, false).expect("thumbnail");
        let decoded = Pixbuf::from_read(std::io::Cursor::new(thumb)).unwrap();
        assert_eq!(
            (decoded.width(), decoded.height()),
            (16, 8),
            "fit into 16x16 keeps the ratio"
        );
        // Not a picture: its icon instead.
        assert!(png(render(&text.to_string_lossy(), 16, 16, false)).is_some());
        assert!(png(render("/nonexistent/x.png", 16, 16, false)).is_none());
        std::fs::remove_dir_all(dir).unwrap();
        assert!(file_description("/usr/lib/firefox/firefox").is_none_or(|n| !n.is_empty()));
        println!(
            "firefox is named: {:?}",
            file_description("/usr/lib/firefox/firefox")
        );
    }
}
