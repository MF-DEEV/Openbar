// SPDX-License-Identifier: GPL-3.0-or-later
import SwiftUI

struct SessionSoundsSettings: View {
    @ObservedObject private var l10n = L10n.shared
    @AppStorage(SessionSoundPreferences.volumeKey) private var volume = 100.0
    @AppStorage(SessionSoundPreferences.muteKey) private var until = 0.0
    private var text: SessionSoundStrings { .forLanguage(l10n.language) }
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(text.title).font(.headline)
            Text(text.caption).font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            ForEach(SessionSoundKind.allCases, id: \.self) { SessionSoundRow($0) }
            SettingsRow(symbol: "speaker.wave.2", title: text.volume) {
                HStack(spacing: 8) {
                    Text("\(Int(SessionSoundPreferences.volume()))%")
                        .monospacedDigit().frame(minWidth: 36, alignment: .trailing)
                    Slider(value: $volume, in: 0...100, step: 1).frame(maxWidth: 160).accessibilityLabel(text.volume)
                }
            }
            .onChange(of: volume) { _, value in if value <= 0 { SessionSoundPlayer.stop() } }
            TimelineView(.explicit([Date(timeIntervalSince1970: until)])) { context in
                if until > context.date.timeIntervalSince1970 {
                    SettingsRow(symbol: "bell.slash", title: text.muted,
                                caption: Date(timeIntervalSince1970: until).formatted()) {
                        Button(text.unmute) { until = 0 }
                    }
                }
            }
        }
    }
}

private struct SessionSoundRow: View {
    let kind: SessionSoundKind
    @ObservedObject private var l10n = L10n.shared
    @AppStorage private var choice: String
    @AppStorage(SessionSoundPreferences.volumeKey) private var volume = 100.0
    @State private var failed = false
    init(_ kind: SessionSoundKind) {
        self.kind = kind
        _choice = AppStorage(wrappedValue: SessionSoundPreferences.choice(kind), kind.preferenceKey)
    }
    private var text: SessionSoundStrings { .forLanguage(l10n.language) }
    var body: some View {
        SettingsRow(symbol: "bell", title: text.event(kind), caption: failed ? text.failed : nil) {
            HStack(spacing: 8) {
                Picker(text.event(kind), selection: $choice) {
                    Text(text.none).tag("none")
                    Text(text.chime).tag("")
                    Text(text.soft).tag("openbar:soft")
                    Text(text.alert).tag("openbar:alert")
                    Divider()
                    ForEach(TextSnippetSupport.alertSoundNames, id: \.self) { Text($0).tag($0) }
                    if !["none", "", "openbar:soft", "openbar:alert"].contains(choice), !TextSnippetSupport.alertSoundNames.contains(choice) {
                        Text(choice + " — " + text.unavailable).tag(choice)
                    }
                }.labelsHidden().frame(maxWidth: 180)
                Button {
                    failed = !SessionSoundPlayer.play(choice, volume: SessionSoundPreferences.volume())
                } label: { Image(systemName: "play.fill") }
                .accessibilityLabel(text.preview(kind)).help(text.preview(kind))
                .disabled(choice == "none" || volume <= 0)
            }
        }
    }
}

struct SessionSoundMuteMenu: View {
    @ObservedObject private var l10n = L10n.shared
    @AppStorage(SessionSoundPreferences.muteKey) private var until = 0.0
    private var text: SessionSoundStrings { .forLanguage(l10n.language) }
    var body: some View {
        if until > Date().timeIntervalSince1970 {
            Text(text.muted + " " + Date(timeIntervalSince1970: until).formatted())
            Button(text.unmute) { until = 0 }
        }
        Button(text.hour) {
            until = Date().addingTimeInterval(3600).timeIntervalSince1970
            SessionSoundPlayer.stop()
        }
        Button(text.tomorrow) {
            until = SessionSoundPreferences.muteUntilTomorrow().timeIntervalSince1970
            SessionSoundPlayer.stop()
        }
    }
}
