// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 MF DEV, Adrian and the Openbar contributors

import CoreImage.CIFilterBuiltins
import SwiftUI

/// Settings › Sessions › Phone approvals (Telegram), #214: what leaves the
/// Mac and where to, a bot's token from BotFather, the one-time code that
/// links the phone, and when messages go out.
struct PhoneApprovalsSettings: View {
    @ObservedObject private var bridge = TelegramBridge.shared
    @AppStorage(DefaultsKey.notchSessionsPhoneEnabled) private var enabled = false
    @AppStorage(DefaultsKey.notchSessionsPhoneAwayMinutes) private var awayMinutes = 5
    @AppStorage(DefaultsKey.notchSessionsPhoneSendAlways) private var sendAlways = false
    @AppStorage(DefaultsKey.notchSessionsPhoneQuietHours) private var quietHours = false
    @AppStorage(DefaultsKey.notchSessionsPhoneQuietFrom) private var quietFrom = 22 * 60
    @AppStorage(DefaultsKey.notchSessionsPhoneQuietTo) private var quietTo = 7 * 60
    @AppStorage(DefaultsKey.notchSessionsPhoneHideCode) private var hideCode = false
    @AppStorage(DefaultsKey.notchSessionsPhoneTellEnds) private var tellEnds = true
    @State private var draft = ""
    @State private var confirmingUnlink = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            SettingsRow(symbol: "iphone.gen3", title: PhoneApprovalStrings.title, caption: PhoneApprovalStrings.hint) {
                if bridge.link != nil {
                    Toggle(PhoneApprovalStrings.enable, isOn: $enabled).labelsHidden().toggleStyle(.switch)
                }
            }
            Group {
                Label(PhoneApprovalStrings.privacy, systemImage: "paperplane")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                if let link = bridge.link {
                    linked(link)
                } else if let linking = bridge.linking, let bot = bridge.bot {
                    code(linking, bot: bot)
                } else if let bot = bridge.bot {
                    ready(bot)
                } else {
                    setup
                }
                if let problem = bridge.problem {
                    Label(problem, systemImage: "exclamationmark.triangle.fill")
                        .font(.caption)
                        .foregroundStyle(.orange)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .padding(.leading, settingsRowTextInset)
        }
        .alert(PhoneApprovalStrings.unlinkConfirmTitle, isPresented: $confirmingUnlink) {
            Button(PhoneApprovalStrings.cancel, role: .cancel) {}
            Button(PhoneApprovalStrings.unlink, role: .destructive) { bridge.unlink() }
        } message: {
            Text(PhoneApprovalStrings.unlinkConfirm)
        }
    }

    // MARK: States

    private var setup: some View {
        VStack(alignment: .leading, spacing: 6) {
            step(1, PhoneApprovalStrings.step1) {
                Link(PhoneApprovalStrings.openBotFather, destination: URL(string: "https://t.me/BotFather")!)
                    .font(.caption)
            }
            step(2, PhoneApprovalStrings.step2)
            step(3, PhoneApprovalStrings.step3)
            HStack(spacing: 6) {
                SecureField(PhoneApprovalStrings.tokenPlaceholder, text: $draft)
                    .textFieldStyle(.roundedBorder)
                    .controlSize(.small)
                    .onSubmit(useToken)
                if bridge.checking {
                    ProgressView().controlSize(.small)
                } else {
                    Button(PhoneApprovalStrings.useToken, action: useToken)
                        .controlSize(.small)
                        .disabled(draft.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
        }
    }

    private func ready(_ bot: String) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(String(format: PhoneApprovalStrings.botReady, bot)).font(.callout)
            HStack(spacing: 8) {
                Button(PhoneApprovalStrings.linkPhone) { bridge.startLinking() }.controlSize(.small)
                Button(PhoneApprovalStrings.forgetBot) { bridge.unlink() }.controlSize(.small)
            }
        }
    }

    private func code(_ linking: TelegramBridge.Linking, bot: String) -> some View {
        HStack(alignment: .top, spacing: 14) {
            if let url = bridge.linkURL, let image = Self.qrCode(url) {
                Image(nsImage: image)
                    .interpolation(.none)
                    .resizable()
                    .frame(width: 132, height: 132)
                    .padding(6)
                    .background(Color.white, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                    .accessibilityLabel(PhoneApprovalStrings.openInTelegram)
            }
            VStack(alignment: .leading, spacing: 8) {
                Text(String(format: PhoneApprovalStrings.codeIntro, bot))
                    .font(.callout)
                    .fixedSize(horizontal: false, vertical: true)
                Text(linking.code)
                    .font(.system(size: 22, weight: .semibold, design: .monospaced))
                    .tracking(3)
                    .textSelection(.enabled)
                Text(PhoneApprovalStrings.codeExpires).font(.caption).foregroundStyle(.secondary)
                HStack(spacing: 8) {
                    if let url = bridge.linkURL.flatMap(URL.init(string:)) {
                        Link(PhoneApprovalStrings.openInTelegram, destination: url).font(.callout)
                    }
                    Button(PhoneApprovalStrings.copyCode) {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(linking.code, forType: .string)
                    }
                    .controlSize(.small)
                    Button(PhoneApprovalStrings.cancel) { bridge.cancelLinking() }.controlSize(.small)
                }
            }
        }
    }

    private func linked(_ link: PhoneLink) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Circle().fill(enabled ? Color.green : Color.secondary).frame(width: 7, height: 7).accessibilityHidden(true)
                Text(String(format: PhoneApprovalStrings.linkedFormat, link.name, bridge.bot ?? "bot"))
                    .font(.callout)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                Button(PhoneApprovalStrings.sendTest) { bridge.sendTest() }.controlSize(.small)
                Button(PhoneApprovalStrings.unlink) { confirmingUnlink = true }.controlSize(.small)
            }
            HStack(spacing: 6) {
                Text(PhoneApprovalStrings.awayAfter)
                Picker(PhoneApprovalStrings.awayAfter, selection: $awayMinutes) {
                    ForEach(PhoneSettings.awayChoices, id: \.self) { minutes in
                        Text(String(format: PhoneApprovalStrings.awayMinutes, minutes)).tag(minutes)
                    }
                }
                .labelsHidden()
                .fixedSize()
                .disabled(sendAlways)
            }
            Text(PhoneApprovalStrings.awayHint).font(.caption).foregroundStyle(.secondary)
            Toggle(PhoneApprovalStrings.sendAlways, isOn: $sendAlways)
            HStack(spacing: 6) {
                Toggle(PhoneApprovalStrings.quietHours, isOn: $quietHours)
                DatePicker(PhoneApprovalStrings.quietHours, selection: minuteBinding($quietFrom), displayedComponents: .hourAndMinute)
                    .labelsHidden()
                    .disabled(!quietHours)
                Text(PhoneApprovalStrings.quietTo)
                DatePicker(PhoneApprovalStrings.quietTo, selection: minuteBinding($quietTo), displayedComponents: .hourAndMinute)
                    .labelsHidden()
                    .disabled(!quietHours)
            }
            Text(PhoneApprovalStrings.quietHint).font(.caption).foregroundStyle(.secondary)
            Toggle(PhoneApprovalStrings.hideCode, isOn: $hideCode)
            Text(PhoneApprovalStrings.hideCodeHint).font(.caption).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            Toggle(PhoneApprovalStrings.tellEnds, isOn: $tellEnds)
        }
        .toggleStyle(.checkbox)
        .disabled(!enabled)
    }

    // MARK: Helpers

    private func step(_ number: Int, _ text: String, @ViewBuilder trailing: () -> some View = { EmptyView() }) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Text("\(number).").font(.callout.monospacedDigit()).foregroundStyle(.secondary)
            Text(text).font(.callout).fixedSize(horizontal: false, vertical: true)
            trailing()
        }
    }

    private func useToken() {
        let token = draft
        Task {
            await bridge.useToken(token)
            if bridge.bot != nil { draft = "" }
        }
    }

    /// Minutes of the day, as the time pickers take them.
    private func minuteBinding(_ minutes: Binding<Int>) -> Binding<Date> {
        Binding(get: {
            Calendar.current.startOfDay(for: Date()).addingTimeInterval(TimeInterval(minutes.wrappedValue * 60))
        }, set: { date in
            minutes.wrappedValue = PhoneAway.minute(of: date)
        })
    }

    /// The link as a QR code a phone's camera opens in Telegram.
    static func qrCode(_ text: String) -> NSImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }
        let scaled = output.transformed(by: CGAffineTransform(scaleX: 8, y: 8))
        guard let cgImage = CIContext().createCGImage(scaled, from: scaled.extent) else { return nil }
        return NSImage(cgImage: cgImage, size: NSSize(width: scaled.extent.width, height: scaled.extent.height))
    }
}
