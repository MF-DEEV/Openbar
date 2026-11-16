// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import SwiftUI

/// Export diagnostics in Settings › Sessions (#230), beside the hooks it reports on.
struct DiagnosticsSettingsRow: View {
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        let text = FeatureStrings.diagnostics(l10n.language)
        Divider()
        SettingsRow(symbol: "doc.zipper", title: text.settingsTitle, caption: text.settingsCaption) {
            Button(text.exportButton) {
                DiagnosticsExportWindow.shared.show()
            }
            .controlSize(.small)
            .fixedSize()
        }
    }
}
