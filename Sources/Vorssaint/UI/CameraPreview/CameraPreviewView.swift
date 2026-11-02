// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AVFoundation
import SwiftUI

/// The floating mirror: the live camera image with a camera picker that
/// appears on hover when more than one camera is around. Esc, a click
/// anywhere else or switching to the meeting app closes it.
struct CameraPreviewView: View {
    var size = CGSize(width: 320, height: 240)
    /// The picker over the image on hover. The island turns it off: its top
    /// row holds the picker and the stop button, clear of the image.
    var showsCameraMenu = true
    @ObservedObject private var l10n = L10n.shared
    @ObservedObject private var service = CameraPreviewService.shared
    @State private var hovering = false

    private var strings: CameraPreviewFeatureStrings {
        FeatureStrings.cameraPreview(l10n.language)
    }

    var body: some View {
        ZStack {
            Color.black
            content
        }
        .frame(width: size.width, height: size.height)
        .overlay(alignment: .bottom) { controls }
        .animation(.easeInOut(duration: 0.15), value: hovering)
        .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .strokeBorder(Color.white.opacity(0.14), lineWidth: 1)
        )
        // The surface is always black, so the controls (spinner, menu,
        // buttons) must draw for a dark background in either system look.
        .environment(\.colorScheme, .dark)
        .onHover { hovering = $0 }
    }

    @ViewBuilder
    private var content: some View {
        switch service.state {
        case .running:
            if let session = service.session {
                CameraLayerView(session: session, mirrored: service.isMirrored,
                                onMirroringSupport: service.reportMirroringSupport)
            }
        case .idle, .waitingPermission, .starting:
            ProgressView()
                .controlSize(.small)
                .tint(.white)
        case .denied:
            statusMessage(icon: "video.slash", text: strings.deniedMessage) {
                Button(l10n.s.permissionOpenSettings) {
                    Permissions.shared.openCameraSettings()
                }
                .controlSize(.small)
            }
        case .unavailable:
            statusMessage(icon: "video.slash", text: FeatureStrings.notchActivities(l10n.language).cameraUnavailable) {
                Button(FeatureStrings.notchActivities(l10n.language).startCamera, action: service.retryCapture)
                    .controlSize(.small)
            }
        case .noCamera:
            statusMessage(icon: "web.camera", text: strings.noCameraMessage) { EmptyView() }
        }
    }

    private var showsControls: Bool {
        showsCameraMenu && hovering && service.state == .running && service.session != nil
    }

    private var showsMenu: Bool { service.devices.count > 1 }

    @ViewBuilder
    private var controls: some View {
        if showsControls {
            // A camera name too long for the image leaves the picker its
            // icon, so it is never cut off at the image's edges.
            ViewThatFits(in: .horizontal) {
                controlRow(showsName: true)
                controlRow(showsName: false)
            }
            .transition(.opacity)
            .padding([.horizontal, .bottom], 10)
        }
    }

    private func controlRow(showsName: Bool) -> some View {
        HStack(spacing: 8) {
            if showsMenu { cameraMenu(showsName: showsName) }
            CameraMirrorButton()
        }
    }

    private func cameraMenu(showsName: Bool) -> some View {
        Menu {
            ForEach(service.devices, id: \.uniqueID) { device in
                Button {
                    service.selectCamera(device)
                } label: {
                    if device.uniqueID == service.selectedDeviceID {
                        Label(device.localizedName, systemImage: "checkmark")
                    } else {
                        Text(device.localizedName)
                    }
                }
            }
        } label: {
            HStack(spacing: 5) {
                Image(systemName: "web.camera")
                    .font(.system(size: 10, weight: .semibold))
                if showsName {
                    Text(currentCameraName)
                        .font(.caption)
                        .lineLimit(1)
                }
                Image(systemName: "chevron.up.chevron.down")
                    .font(.system(size: 8, weight: .semibold))
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .foregroundStyle(.white)
        .background(.black.opacity(0.55), in: Capsule())
        .accessibilityLabel(strings.cameraMenuLabel)
    }

    private var currentCameraName: String {
        service.devices.first { $0.uniqueID == service.selectedDeviceID }?.localizedName
            ?? strings.cameraMenuLabel
    }

    private func statusMessage<Extra: View>(icon: String,
                                            text: String,
                                            @ViewBuilder extra: () -> Extra) -> some View {
        VStack(spacing: 10) {
            Image(systemName: icon)
                .font(.system(size: 26, weight: .light))
                .foregroundStyle(.white.opacity(0.55))
            Text(text)
                .font(.caption)
                .multilineTextAlignment(.center)
                .foregroundStyle(.white.opacity(0.8))
                .fixedSize(horizontal: false, vertical: true)
            extra()
        }
        .padding(.horizontal, 28)
    }
}

/// Flips the preview between mirrored and as the camera sees it. A camera
/// whose preview can't be flipped keeps the button, dimmed, saying so.
private struct CameraMirrorButton: View {
    @ObservedObject private var l10n = L10n.shared
    @ObservedObject private var service = CameraPreviewService.shared

    var body: some View {
        let strings = FeatureStrings.cameraPreview(l10n.language)
        let on = service.isMirrored && service.canMirror
        Button(action: service.toggleMirrored) {
            Image(systemName: CameraMirrorSymbol.name(mirrored: on))
                .font(.system(size: 11, weight: .semibold))
                .frame(width: 26, height: 26)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(.white.opacity(service.canMirror ? 1 : 0.4))
        .background(on ? .white.opacity(0.28) : .black.opacity(0.55), in: Circle())
        .disabled(!service.canMirror)
        .help(service.canMirror ? strings.mirrorLabel : strings.mirrorUnavailable)
        .accessibilityLabel(strings.mirrorLabel)
        .accessibilityValue(service.canMirror ? "" : strings.mirrorUnavailable)
        .accessibilityAddTraits(on ? .isSelected : [])
    }
}

/// The flip glyph, filled while the preview is mirrored, so the state reads
/// without relying on the button's background alone.
enum CameraMirrorSymbol {
    static func name(mirrored: Bool) -> String {
        mirrored ? "arrow.left.and.right.righttriangle.left.righttriangle.right.fill"
            : "arrow.left.and.right.righttriangle.left.righttriangle.right"
    }
}

/// Hosts the AVCaptureVideoPreviewLayer. The mirror flip lives on the layer's
/// connection, which only exists once the session has its input and is
/// replaced when the camera changes, so it is (re)applied on every update.
private struct CameraLayerView: NSViewRepresentable {
    let session: AVCaptureSession
    let mirrored: Bool
    let onMirroringSupport: (Bool) -> Void

    final class LayerHostView: NSView {
        let previewLayer: AVCaptureVideoPreviewLayer

        init(session: AVCaptureSession) {
            previewLayer = AVCaptureVideoPreviewLayer(session: session)
            super.init(frame: .zero)
            wantsLayer = true
            previewLayer.videoGravity = .resizeAspectFill
            layer = previewLayer
        }

        @available(*, unavailable)
        required init?(coder: NSCoder) { nil }

        /// Mirrored is what a mirror shows, the same thing Photo Booth does;
        /// unmirrored is what the people on a call see. Only the preview
        /// layer flips, never the frames. Returns whether this camera's
        /// preview can be flipped, or nil while there is no connection yet.
        func applyMirroring(_ mirrored: Bool) -> Bool? {
            guard let connection = previewLayer.connection else { return nil }
            guard connection.isVideoMirroringSupported else { return false }
            if connection.automaticallyAdjustsVideoMirroring {
                connection.automaticallyAdjustsVideoMirroring = false
            }
            if connection.isVideoMirrored != mirrored {
                connection.isVideoMirrored = mirrored
            }
            return true
        }
    }

    func makeNSView(context: Context) -> LayerHostView {
        LayerHostView(session: session)
    }

    func updateNSView(_ view: LayerHostView, context: Context) {
        if view.previewLayer.session !== session {
            view.previewLayer.session = session
        }
        guard let supported = view.applyMirroring(mirrored) else { return }
        // Publishing in the middle of a view update is not allowed.
        DispatchQueue.main.async { onMirroringSupport(supported) }
    }
}
