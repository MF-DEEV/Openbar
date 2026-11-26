// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

//! AT-SPI, the desktop's accessibility bus, as far as Openbar reads it: an
//! app's object tree, each object's name and states. Shared by Discord's
//! reply (its message box) and the microphone indicator (its Mute and Deafen
//! buttons).

use zbus::blocking::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

// AT-SPI state bits (AtspiStateType).
pub const STATE_CHECKED: u32 = 4;
pub const STATE_EDITABLE: u32 = 7;
pub const STATE_FOCUSED: u32 = 12;
pub const STATE_PRESSED: u32 = 20;
pub const STATE_SHOWING: u32 = 25;
pub const STATE_CHECKABLE: u32 = 41;

/// One object on the accessibility bus: its owner and object path.
pub type Node = (String, OwnedObjectPath);

pub struct Accessibility {
    bus: Connection,
}

impl Accessibility {
    pub fn open() -> Option<Self> {
        let session = Connection::session().ok()?;
        let address: String = session
            .call_method(
                Some("org.a11y.Bus"),
                "/org/a11y/bus",
                Some("org.a11y.Bus"),
                "GetAddress",
                &(),
            )
            .ok()?
            .body()
            .deserialize()
            .ok()?;
        let bus = zbus::blocking::connection::Builder::address(address.as_str())
            .ok()?
            .build()
            .ok()?;
        Some(Self { bus })
    }

    pub fn call<B>(&self, node: &Node, interface: &str, method: &str, body: &B) -> Option<zbus::Message>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        self.bus
            .call_method(
                Some(node.0.as_str()),
                node.1.as_str(),
                Some(interface),
                method,
                body,
            )
            .ok()
    }

    pub fn children(&self, node: &Node) -> Vec<Node> {
        self.call(node, "org.a11y.atspi.Accessible", "GetChildren", &())
            .and_then(|reply| reply.body().deserialize().ok())
            .unwrap_or_default()
    }

    pub fn name(&self, node: &Node) -> String {
        self.call(
            node,
            "org.freedesktop.DBus.Properties",
            "Get",
            &("org.a11y.atspi.Accessible", "Name"),
        )
        .and_then(|reply| reply.body().deserialize::<OwnedValue>().ok())
        .and_then(|value| String::try_from(value).ok())
        .unwrap_or_default()
    }

    pub fn states(&self, node: &Node) -> u64 {
        let words: Vec<u32> = self
            .call(node, "org.a11y.atspi.Accessible", "GetState", &())
            .and_then(|reply| reply.body().deserialize().ok())
            .unwrap_or_default();
        words
            .iter()
            .take(2)
            .enumerate()
            .fold(0, |bits, (i, word)| bits | (u64::from(*word) << (32 * i)))
    }

    pub fn text(&self, node: &Node) -> Option<String> {
        self.call(node, "org.a11y.atspi.Text", "GetText", &(0i32, -1i32))?
            .body()
            .deserialize()
            .ok()
    }

    pub fn grab_focus(&self, node: &Node) -> bool {
        self.call(node, "org.a11y.atspi.Component", "GrabFocus", &())
            .and_then(|reply| reply.body().deserialize::<bool>().ok())
            .unwrap_or(false)
    }

    /// The application object of the process `pid`.
    pub fn application(&self, pid: u32) -> Option<Node> {
        let root: Node = (
            "org.a11y.atspi.Registry".into(),
            OwnedObjectPath::try_from("/org/a11y/atspi/accessible/root").ok()?,
        );
        self.children(&root).into_iter().find(|app| {
            self.bus
                .call_method(
                    Some("org.freedesktop.DBus"),
                    "/org/freedesktop/DBus",
                    Some("org.freedesktop.DBus"),
                    "GetConnectionUnixProcessID",
                    &(app.0.as_str(),),
                )
                .ok()
                .and_then(|reply| reply.body().deserialize::<u32>().ok())
                == Some(pid)
        })
    }
}
