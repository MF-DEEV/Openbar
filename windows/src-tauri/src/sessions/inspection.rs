//! Permission content and backend-issued, request-bound inspection capabilities (#224).
//! Full text never travels in recurring snapshots. No approval trusts a UI boolean.
use super::models::Permission;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, sync::Arc};

pub const PREVIEW_LIMIT: usize = 8000;
// Rendering beyond this budget fails closed; it never silently clips the full viewer.
pub const VIEWER_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct Content {
    pub full: Arc<str>,
    pub complete: bool,
    pub original_length: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub revision: String,
    pub complete: bool,
    pub truncated: bool,
    pub remaining: Option<usize>,
    pub requires_inspection: bool,
    pub available: bool,
}

impl Content {
    pub fn new(text: &str, complete: bool, original_length: Option<usize>) -> Self {
        Self {
            full: Arc::from(text),
            complete,
            original_length,
        }
    }
    pub fn preview(&self) -> String {
        let count = self.full.chars().count();
        let mut text = display(&self.full.chars().take(PREVIEW_LIMIT).collect::<String>());
        let remaining = self
            .original_length
            .unwrap_or(count)
            .saturating_sub(count.min(PREVIEW_LIMIT));
        if remaining > 0 {
            text.push_str(&format!("\n… {} more characters", remaining));
        }
        if !self.complete {
            text.push_str("\n… full command unavailable");
        }
        text
    }
    pub fn metadata(&self, p: &Permission) -> Metadata {
        let count = self.full.chars().count();
        let mut hash = Sha256::new();
        // Length-prefix fields: no ambiguous concatenation, including embedded NULs.
        for part in [
            p.request_id.as_bytes(),
            p.tool.as_bytes(),
            &p.since.to_bits().to_le_bytes(),
            self.full.as_bytes(),
            &[u8::from(self.complete)],
            &self.original_length.unwrap_or(usize::MAX).to_le_bytes(),
        ] {
            hash.update(part.len().to_le_bytes());
            hash.update(part);
        }
        // An empty request (a tool with no arguments) hides nothing; an empty
        // command is marked incomplete by its reader instead.
        let complete = self.complete;
        let hidden = hidden(&self.full);
        Metadata {
            revision: format!("{:x}", hash.finalize()),
            complete,
            truncated: count > PREVIEW_LIMIT || !complete,
            remaining: self
                .original_length
                .map(|n| n.saturating_sub(count.min(PREVIEW_LIMIT))),
            requires_inspection: !complete || count > PREVIEW_LIMIT || hidden.iter().any(|&h| h),
            available: complete
                && self.full.len() <= VIEWER_BYTES
                && self
                    .full
                    .chars()
                    .zip(&hidden)
                    .map(|(c, &h)| if h { 16 } else { c.len_utf8() })
                    .sum::<usize>()
                    <= VIEWER_BYTES,
        }
    }
}

fn visible_control(c: char) -> bool {
    matches!(c, '\u{27e6}' | '\u{27e7}')
        || (c.is_control() && c != '\n' && c != '\t')
        || matches!(c, '\u{061c}' | '\u{200b}'..='\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}')
}

/// Emoji, their skin tones and the variation selector that ends a flag or symbol.
fn pictograph(c: char) -> bool {
    matches!(c, '\u{1f000}'..='\u{1faff}' | '\u{2600}'..='\u{27bf}' | '\u{2b00}'..='\u{2bff}' | '\u{fe0f}')
}

/// Which characters are shown as annotations. A zero-width joiner between two
/// emoji is part of one picture (🧑‍💻), not hidden text.
pub fn hidden(text: &str) -> Vec<bool> {
    let chars: Vec<char> = text.chars().collect();
    (0..chars.len())
        .map(|i| {
            if chars[i] == '\u{200d}'
                && i > 0
                && i + 1 < chars.len()
                && pictograph(chars[i - 1])
                && pictograph(chars[i + 1])
            {
                return false;
            }
            visible_control(chars[i])
        })
        .collect()
}

/// Annotate otherwise invisible controls without interpreting ANSI or bidi commands.
/// The original remains byte-for-byte intact in Content.full.
pub fn display(raw: &str) -> String {
    let mut out = String::new();
    for (c, h) in raw.chars().zip(hidden(raw)) {
        if h {
            out.push_str(&format!("⟦U+{:04X}⟧", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub revision: String,
    pub token: String,
    pub text: String,
}

#[derive(Default)]
pub struct Gate {
    grants: HashMap<String, (String, String)>,
}

impl Gate {
    pub fn open(&mut self, session: &str, p: &Permission, revision: &str) -> Option<Inspection> {
        let content = p.content.as_ref()?;
        let meta = content.metadata(p);
        if meta.revision != revision || !meta.available {
            return None;
        }
        let mut random = [0u8; 32];
        getrandom::fill(&mut random).ok()?;
        let token = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        self.grants
            .retain(|_, (s, r)| s != session || r != revision);
        self.grants
            .insert(token.clone(), (session.to_owned(), revision.to_owned()));
        Some(Inspection {
            revision: revision.to_owned(),
            token,
            text: display(&content.full),
        })
    }
    pub fn allows(
        &self,
        session: &str,
        p: &Permission,
        revision: Option<&str>,
        token: Option<&str>,
    ) -> bool {
        let Some(content) = &p.content else {
            return false;
        };
        let meta = content.metadata(p);
        if revision != Some(meta.revision.as_str()) || !meta.complete {
            return false;
        }
        if !meta.requires_inspection {
            return true;
        }
        meta.available
            && token
                .and_then(|t| self.grants.get(t))
                .is_some_and(|(s, r)| s == session && r == &meta.revision)
    }
    pub fn invalidate_revision(&mut self, session: &str, revision: &str) {
        self.grants
            .retain(|_, (s, r)| s != session || r != revision);
    }
    pub fn invalidate(&mut self, session: &str) {
        self.grants.retain(|_, (s, _)| s != session);
    }
    pub fn close(&mut self, token: &str) {
        self.grants.remove(token);
    }
    pub fn retain(&mut self, live: &[(String, String)]) {
        self.grants.retain(|_, grant| live.contains(grant));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::{
        hook_agents,
        hook_message::HookMessage,
        models::{ContentPolicy, HookEvent},
    };
    use serde_json::json;

    fn card(raw: &str, complete: bool) -> Permission {
        let mut p = Permission::new("request-a", "Bash", None, true, 1.0);
        p.set_content(raw, complete, Some(raw.chars().count()));
        p
    }

    #[test]
    fn security_custom_opencode_patterns_require_explicit_full_content() {
        let raw = format!("\n  {} HIDDEN_PATTERN_SUFFIX 🦀  \n", "x".repeat(9000));
        for flag in [serde_json::Value::Null, json!(false), json!(1), json!(true)] {
            let payload = json!({"openbar":"opencode","event":{
                "kind":"permission","session":"s","id":"custom","permission":"custom_shell",
                "patterns":["",raw.trim_matches('\n'),""], "metadata":{},
                "permissionContentComplete":flag,"permissionDetailLength":raw.chars().count()
            }});
            let decoded = hook_agents::decode(
                &serde_json::to_vec(&payload).unwrap(),
                1.0,
                ContentPolicy::default(),
            )
            .unwrap();
            let hook_agents::Kind::Permission(p) = &decoded.events[0].kind else {
                panic!("permission");
            };
            assert_eq!(p.content.as_ref().unwrap().full.as_ref(), raw);
            assert!(!p.detail.contains("HIDDEN_PATTERN_SUFFIX"));
            let revision = p.content.as_ref().unwrap().metadata(p).revision;
            let mut gate = Gate::default();
            assert!(!gate.allows("opencode:s", p, Some(&revision), None));
            let inspection = gate.open("opencode:s", p, &revision);
            assert_eq!(inspection.is_some(), flag == json!(true));
            if let Some(inspection) = inspection {
                assert_eq!(inspection.text, raw);
                assert!(gate.allows("opencode:s", p, Some(&revision), Some(&inspection.token)));
            }
        }
        for event in [
            json!({"patterns":["partial"],"permissionContentComplete":true,"permissionDetailLength":99}),
            json!({"patterns":["partial",42],"permissionContentComplete":true}),
            json!({"patterns":["partial"],"metadata":42,"permissionContentComplete":true}),
            json!({"metadata":{"command":42},"permissionContentComplete":true}),
        ] {
            let mut payload = json!({"openbar":"opencode","event":{"kind":"permission","session":"s","id":"bad","permission":"custom_shell"}});
            payload["event"]
                .as_object_mut()
                .unwrap()
                .extend(event.as_object().unwrap().clone());
            let decoded = hook_agents::decode(
                &serde_json::to_vec(&payload).unwrap(),
                1.0,
                ContentPolicy::default(),
            )
            .unwrap();
            let hook_agents::Kind::Permission(p) = &decoded.events[0].kind else {
                panic!("permission");
            };
            assert!(!p.content.as_ref().unwrap().complete);
        }
    }

    #[test]
    fn security_opencode_command_cannot_be_hidden_by_a_file_diff() {
        let metadata = json!({"command":"echo FIXTURE", "commandComplete":true, "commandLength":12,
            "filepath":"/fixture.txt", "diff":"--- a/fixture.txt\n+++ b/fixture.txt\n@@ -1 +1 @@\n-old\n+new\n"});
        assert!(!crate::sessions::file_diff::opencode(metadata.as_object().unwrap()).is_empty());
        let payload = json!({"openbar":"opencode", "event":{"kind":"permission", "session":"s", "id":"diff", "permission":"edit", "metadata":metadata}});
        let decoded = hook_agents::decode(
            &serde_json::to_vec(&payload).unwrap(),
            1.0,
            ContentPolicy::default(),
        )
        .unwrap();
        let hook_agents::Kind::Permission(p) = &decoded.events[0].kind else {
            panic!("permission")
        };
        assert!(p.diffs.is_empty());
        assert_eq!(p.detail, "echo FIXTURE");
        assert!(p.content.as_ref().unwrap().complete);
    }

    #[test]
    fn security_preview_boundaries_and_hidden_suffix() {
        for length in [0, 1, 7999, 8000, 8001, 40000] {
            let raw = "x".repeat(length);
            let p = card(&raw, true);
            let c = p.content.as_ref().unwrap();
            let meta = c.metadata(&p);
            assert_eq!(c.full.as_ref(), raw);
            assert_eq!(meta.requires_inspection, length > 8000);
            assert_eq!(
                Gate::default().allows("s", &p, Some(&meta.revision), None),
                length <= 8000
            );
        }
        let raw = format!(
            "{}\nprintf 'HIDDEN_SUFFIX_FIXTURE'",
            "# harmless\n".repeat(1000)
        );
        let p = card(&raw, true);
        assert!(!p.detail.contains("HIDDEN_SUFFIX_FIXTURE"));
        let revision = p.content.as_ref().unwrap().metadata(&p).revision;
        let mut gate = Gate::default();
        assert!(!gate.allows("s", &p, None, Some("forged")));
        let inspection = gate.open("s", &p, &revision).unwrap();
        assert_eq!(inspection.text, raw);
        assert!(gate.allows("s", &p, Some(&revision), Some(&inspection.token)));
        assert!(!gate.allows("other", &p, Some(&revision), Some(&inspection.token)));
        let mut replaced = p.clone();
        replaced.request_id = "request-b".into();
        assert!(!gate.allows("s", &replaced, Some(&revision), Some(&inspection.token)));
        replaced = p.clone();
        replaced.set_content(&(raw + "\necho changed"), true, None);
        assert!(!gate.allows("s", &replaced, Some(&revision), Some(&inspection.token)));
        replaced = p.clone();
        replaced.since = 2.0;
        assert!(!gate.allows("s", &replaced, Some(&revision), Some(&inspection.token)));
        gate.close(&inspection.token);
        assert!(!gate.allows("s", &p, Some(&revision), Some(&inspection.token)));
    }

    #[test]
    fn security_exact_content_controls_and_resource_failure() {
        let raw = "  printf '%s\\n' 'zażółć 🦀'\r\n\t echo \"quoted\"\u{1b}[2J\u{202e}  ";
        let p = card(raw, true);
        assert_eq!(p.content.as_ref().unwrap().full.as_ref(), raw);
        assert!(p.detail.contains("⟦U+001B⟧") && p.detail.contains("⟦U+202E⟧"));
        assert_ne!(
            display("\u{1b}"),
            display("⟦U+001B⟧"),
            "literal annotations cannot impersonate control characters"
        );
        let huge = card(&"x".repeat(VIEWER_BYTES + 1), true);
        assert!(!huge.content.as_ref().unwrap().metadata(&huge).available);
        assert!(!Gate::default().allows("s", &huge, None, None));
        let incomplete = card("partial", false);
        let revision = incomplete
            .content
            .as_ref()
            .unwrap()
            .metadata(&incomplete)
            .revision;
        assert!(Gate::default().open("s", &incomplete, &revision).is_none());
        let mut clipped = card("partial", false);
        clipped.set_content("partial", false, Some(9000));
        assert!(
            clipped.detail.contains("8993 more characters")
                && clipped.detail.contains("full command unavailable")
        );
    }

    #[test]
    fn empty_requests_and_emoji_joiners() {
        let decode = |tool: &str, input: serde_json::Value| {
            let payload = json!({"session_id":"s", "hook_event_name":"PermissionRequest", "tool_name":tool, "tool_input":input});
            let msg = HookMessage::decode(
                &serde_json::to_vec(&payload).unwrap(),
                "r",
                1.0,
                ContentPolicy::default(),
            )
            .unwrap();
            let HookEvent::PermissionRequested(p) = msg.fact.event else {
                panic!("permission");
            };
            p
        };
        let tool = decode("mcp__notes__list", json!({}));
        let meta = tool.content.as_ref().unwrap().metadata(&tool);
        assert!(
            !meta.requires_inspection,
            "a tool with no arguments hides nothing"
        );
        assert!(Gate::default().allows("s", &tool, Some(&meta.revision), None));
        let bash = decode("Bash", json!({"command":""}));
        let meta = bash.content.as_ref().unwrap().metadata(&bash);
        assert!(
            meta.requires_inspection
                && !Gate::default().allows("s", &bash, Some(&meta.revision), None)
        );
        let emoji = card(
            "echo 'dev 🧑\u{200d}💻 👩🏽\u{200d}🚀 🏳\u{fe0f}\u{200d}🌈'",
            true,
        );
        assert!(
            !emoji
                .content
                .as_ref()
                .unwrap()
                .metadata(&emoji)
                .requires_inspection
        );
        assert!(!emoji.detail.contains('⟦'));
        for raw in [
            "echo a\u{200d}b",
            "echo 🧑\u{200d}",
            "\u{200d}💻",
            "echo 🧑\u{200d}x",
        ] {
            let p = card(raw, true);
            assert!(
                p.content.as_ref().unwrap().metadata(&p).requires_inspection,
                "{raw:?}"
            );
        }
    }

    #[test]
    fn codex_scripts_unwrap_only_when_nothing_hides() {
        use crate::sessions::codex::state::approval_script as script;
        assert_eq!(
            script(r#""C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe" -Command 'echo ''hi'''"#).as_deref(),
            Some("echo 'hi'")
        );
        assert_eq!(
            script(r#"pwsh.exe -NoProfile -Command "dir""#).as_deref(),
            Some("dir")
        );
        assert_eq!(script("cmd.exe /c dir").as_deref(), Some("dir"));
        for hiding in [
            "evil; powershell.exe -Command 'ls'",
            "powershell.exe -Command 'a' ; 'b'",
            "powershell.exe -Command 'ls'; rm x",
            "echo cmd /c dir",
            "ls",
        ] {
            assert_eq!(script(hiding), None, "{hiding}");
        }
    }

    #[test]
    fn security_sessions_are_independent_and_expiry_revokes() {
        let p = card(&"x".repeat(8001), true);
        let revision = p.content.as_ref().unwrap().metadata(&p).revision;
        let mut gate = Gate::default();
        let a = gate.open("a", &p, &revision).unwrap();
        let b = gate.open("b", &p, &revision).unwrap();
        gate.invalidate("a");
        assert!(!gate.allows("a", &p, None, Some(&a.token)));
        assert!(gate.allows("b", &p, Some(&revision), Some(&b.token)));
        gate.retain(&[]);
        assert!(!gate.allows("b", &p, Some(&revision), Some(&b.token)));
    }

    #[test]
    fn security_claude_and_opencode_preserve_full_and_reject_legacy() {
        let raw = format!("  {}\n# suffix 🦀  ", "x".repeat(12000));
        let payload = json!({"session_id":"s", "hook_event_name":"PermissionRequest", "tool_name":"Bash", "tool_input":{"command":raw}});
        let msg = HookMessage::decode(
            &serde_json::to_vec(&payload).unwrap(),
            "r",
            1.0,
            ContentPolicy::default(),
        )
        .unwrap();
        let HookEvent::PermissionRequested(p) = msg.fact.event else {
            panic!("permission");
        };
        assert_eq!(p.content.unwrap().full.as_ref(), raw);
        for metadata in [
            json!({"command":raw, "commandComplete":true, "commandLength":raw.chars().count()}),
            json!({"command":raw}),
            json!({"command":42}),
            json!({"command":"x", "commandComplete":true,"commandLength":99}),
        ] {
            let complete = metadata.get("commandLength") == Some(&json!(raw.chars().count()));
            let event = json!({"openbar":"opencode", "event":{"kind":"permission","session":"s","id":"r","permission":"bash","metadata":metadata}});
            let decoded = hook_agents::decode(
                &serde_json::to_vec(&event).unwrap(),
                1.0,
                ContentPolicy::default(),
            )
            .unwrap();
            let hook_agents::Kind::Permission(p) = &decoded.events[0].kind else {
                panic!("permission");
            };
            assert_eq!(p.content.as_ref().unwrap().complete, complete);
            assert!(!Gate::default().allows("opencode:s", p, None, None));
            if complete {
                assert_eq!(p.content.as_ref().unwrap().full.as_ref(), raw);
            }
        }
    }
}
