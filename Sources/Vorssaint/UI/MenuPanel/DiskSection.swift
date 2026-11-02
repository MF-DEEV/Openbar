// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Vorssaint

import AppKit
import SwiftUI

struct DiskSection: View {
    @ObservedObject private var l10n = L10n.shared
    @ObservedObject private var monitor = SystemMonitor.shared
    @ObservedObject private var protection = DiskProtectionService.shared
    @Environment(\.colorScheme) private var colorScheme
    var collapsible = true
    @AppStorage(DefaultsKey.monitorGraphDisk) private var showGraph = true
    @AppStorage(DefaultsKey.monitorDiskUsage) private var diskUsage = true
    @AppStorage(DefaultsKey.monitorDiskActivity) private var diskActivity = true
    @AppStorage(DefaultsKey.monitorDiskSMART) private var diskSMART = true
    @AppStorage(DefaultsKey.monitorDiskProtection) private var diskProtection = true
    @AppStorage(DefaultsKey.monitorDiskTools) private var diskTools = true
    @AppStorage(DefaultsKey.panelDiskOrder) private var diskOrderRaw = ""
    @AppStorage(DefaultsKey.temperatureUnit) private var temperatureUnit = TemperatureUnit.celsius.rawValue
    @State private var draggingBlock: Block?
    @State private var selectedDiskID: String?

    private enum Block: String, PanelOrderItem { case usage, activity, smart, protection, tools }

    var body: some View {
        PanelSection(.disk, title: l10n.s.diskSection, collapsible: collapsible,
                     supportsEditing: true,
                     resetAction: resetPanelDefaults) { editing in
            VStack(alignment: .leading, spacing: 10) {
                if disks.isEmpty {
                    Text(l10n.s.diskNoDisks)
                        .font(.system(size: 10.5))
                        .foregroundStyle(.tertiary)
                } else {
                    if disks.count > 1 {
                        diskSelector
                    }
                    if let selected = selectedDisk {
                        ForEach(Array(blocks(editing: editing, disk: selected).enumerated()), id: \.element) { index, block in
                            if index > 0 || disks.count > 1 {
                                Divider()
                            }
                            PanelReorderableItem(item: block,
                                                 isEnabled: editing,
                                                 order: blockOrderBinding,
                                                 dragging: $draggingBlock) {
                                HStack(alignment: .top, spacing: 8) {
                                    if editing {
                                        PanelDragHandle()
                                    }
                                    blockContent(block, disk: selected, editing: editing)
                                        .frame(maxWidth: .infinity, alignment: .leading)
                                }
                            }
                        }
                    }
                }
            }
            .panelCard()
            .onAppear(perform: ensureSelectedDisk)
            .onChange(of: disks.map(\.id)) { _, _ in ensureSelectedDisk() }
        }
    }

    private var disks: [DiskDeviceReading] {
        monitor.snapshot.disk?.devices ?? []
    }

    private var selectedDisk: DiskDeviceReading? {
        if let selectedDiskID,
           let disk = disks.first(where: { $0.id == selectedDiskID }) {
            return disk
        }
        return disks.first
    }

    private var ejectableDisks: [DiskDeviceReading] {
        var seen = Set<String>()
        return disks.filter { disk in
            guard disk.canEject, let id = disk.ejectBSDName else { return false }
            guard !protection.isExcluded(disk: disk) else { return false }
            return seen.insert(id).inserted
        }
    }

    private func ensureSelectedDisk() {
        guard !disks.isEmpty else {
            selectedDiskID = nil
            return
        }
        if selectedDiskID == nil || !disks.contains(where: { $0.id == selectedDiskID }) {
            selectedDiskID = disks.first?.id
        }
    }

    private var orderedBlocks: [Block] {
        _ = diskOrderRaw
        return PanelLayout.itemOrder(Block.self, key: DefaultsKey.panelDiskOrder)
    }

    private var blockOrderBinding: Binding<[Block]> {
        Binding {
            orderedBlocks
        } set: { newValue in
            PanelLayout.setItemOrder(newValue, key: DefaultsKey.panelDiskOrder)
        }
    }

    private func blocks(editing: Bool, disk: DiskDeviceReading) -> [Block] {
        guard !editing else { return orderedBlocks }
        return orderedBlocks.filter { block in
            guard isVisible(block) else { return false }
            // Eject buttons that can never do anything only take room.
            if block == .protection { return disk.canEject || !ejectableDisks.isEmpty }
            return true
        }
    }

    private func isVisible(_ block: Block) -> Bool {
        switch block {
        case .usage: return diskUsage
        case .activity: return diskActivity
        case .smart: return diskSMART
        case .protection: return diskProtection
        case .tools: return diskTools
        }
    }

    private func resetPanelDefaults() {
        PanelLayout.resetItemOrder(key: DefaultsKey.panelDiskOrder)
        diskOrderRaw = ""
        diskUsage = true
        diskActivity = true
        diskSMART = true
        diskProtection = true
        diskTools = true
    }

    @ViewBuilder
    private func blockContent(_ block: Block, disk: DiskDeviceReading, editing: Bool) -> some View {
        switch block {
        case .usage: usageBlock(disk: disk, editing: editing)
        case .activity: activityBlock(disk: disk, editing: editing)
        case .smart: smartBlock(disk: disk, editing: editing)
        case .protection: protectionBlock(disk: disk, editing: editing)
        case .tools: toolsBlock(disk: disk, editing: editing)
        }
    }

    @ViewBuilder
    private var diskSelector: some View {
        LazyVGrid(columns: diskSelectorColumns, alignment: .leading, spacing: 6) {
            ForEach(disks) { disk in
                diskSelectorButton(for: disk)
            }
        }
        .accessibilityLabel(l10n.s.diskSelect)
    }

    private var diskSelectorColumns: [GridItem] {
        [GridItem(.adaptive(minimum: 96, maximum: 138), spacing: 6, alignment: .leading)]
    }

    private func diskSelectorButton(for disk: DiskDeviceReading) -> some View {
        HStack(spacing: 0) {
            Button {
                selectedDiskID = disk.id
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: disk.isInternal ? "internaldrive" : "externaldrive")
                        .font(.system(size: 10, weight: .semibold))
                    VStack(alignment: .leading, spacing: 1) {
                        Text(disk.name)
                            .font(.system(size: 10.5, weight: .semibold))
                            .lineLimit(1)
                            .truncationMode(.middle)
                        Text("\(MetricFormat.percent(disk.usedFraction)) \(l10n.s.diskUsed)")
                            .font(.system(size: 9))
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                    Spacer(minLength: 0)
                }
                .padding(.leading, 8)
                .padding(.trailing, disk.canEject ? 2 : 8)
                .padding(.vertical, 5)
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if disk.canEject {
                DiskSelectorEjectButton(state: protection.state(for: disk),
                                        idleHelp: l10n.s.diskEject,
                                        busyHelp: l10n.s.diskEjecting,
                                        readyHelp: l10n.s.diskReadyToRemove,
                                        failedHelp: l10n.s.diskEjectFailed) {
                    protection.eject(disk)
                }
                .padding(.trailing, 5)
            }
        }
        .background(selectorBackground(for: disk))
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
    }

    private func selectorBackground(for disk: DiskDeviceReading) -> some ShapeStyle {
        disk.id == selectedDisk?.id ? Color.accentColor.opacity(0.18) : Color.primary.opacity(0.06)
    }

    @ViewBuilder
    private func usageBlock(disk: DiskDeviceReading, editing: Bool) -> some View {
        if !diskUsage {
            PanelHiddenItemRow(title: l10n.s.monitorItemDiskUsage,
                               systemImage: "internaldrive",
                               isVisible: $diskUsage)
        } else {
            VStack(alignment: .leading, spacing: 6) {
                if editing {
                    blockHeader(l10n.s.monitorItemDiskUsage, editing: editing, visible: $diskUsage)
                }
                diskTitleRow(disk, trailing: MetricFormat.percent(disk.usedFraction))
                DiskUsageBar(fraction: disk.usedFraction)
                usageFigures(disk)
            }
        }
    }

    private func usageFigures(_ disk: DiskDeviceReading) -> some View {
        let used = Text("\(MetricFormat.diskBytes(disk.usedBytes)) / \(MetricFormat.diskBytes(disk.totalBytes))")
        let available = Text("\(MetricFormat.diskBytes(disk.freeBytes)) \(l10n.s.diskAvailable)")
        let purgeable = disk.purgeableBytes.flatMap { $0 >= 500_000_000 ? $0 : nil }.map {
            Text("\(MetricFormat.diskBytes($0)) \(l10n.s.diskPurgeable)").foregroundStyle(.tertiary)
        }
        // One line when it fits, the purgeable figure drops to a second line when not.
        return ViewThatFits(in: .horizontal) {
            HStack(spacing: 6) {
                used
                Spacer(minLength: 8)
                available
                if let purgeable {
                    Text("·").foregroundStyle(.tertiary)
                    purgeable
                }
            }
            .fixedSize(horizontal: true, vertical: false)
            .frame(maxWidth: .infinity)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    used
                    Spacer(minLength: 8)
                    available
                }
                if let purgeable {
                    HStack {
                        Spacer(minLength: 0)
                        purgeable
                    }
                }
            }
        }
        .font(.system(size: 10.5, weight: .medium))
        .monospacedDigit()
        .foregroundStyle(.secondary)
        .lineLimit(1)
    }

    @ViewBuilder
    private func activityBlock(disk: DiskDeviceReading, editing: Bool) -> some View {
        if !diskActivity {
            PanelHiddenItemRow(title: l10n.s.monitorItemDiskActivity,
                               systemImage: "arrow.up.arrow.down",
                               isVisible: $diskActivity)
        } else {
            VStack(alignment: .leading, spacing: 7) {
                if editing {
                    blockHeader(l10n.s.monitorItemDiskActivity, editing: editing, visible: $diskActivity)
                }
                HStack(spacing: 10) {
                    rateColumn(icon: "arrow.down",
                               label: l10n.s.diskRead,
                               value: disk.readBytesPerSec,
                               color: .accentColor)
                    Divider().frame(height: 28)
                    rateColumn(icon: "arrow.up",
                               label: l10n.s.diskWrite,
                               value: disk.writeBytesPerSec,
                               color: PanelMetricColor.pink(for: colorScheme))
                }
                if showGraph, monitor.snapshot.diskReadHistory.count >= 2 {
                    graph
                }
                HStack(spacing: 6) {
                    Text(l10n.s.networkThisSession)
                        .font(.system(size: 10))
                        .foregroundStyle(.tertiary)
                    Spacer()
                    Text("↓ \(MetricFormat.diskBytes(disk.totalReadBytes ?? 0))  ↑ \(MetricFormat.diskBytes(disk.totalWrittenBytes ?? 0))")
                        .font(.system(size: 10.5, weight: .medium))
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    private var graph: some View {
        let read = monitor.snapshot.diskReadHistory
        let write = monitor.snapshot.diskWriteHistory
        let peak = MetricFormat.graphCeiling(max(read.max() ?? 0, write.max() ?? 0, 1), unitStep: 1024)
        return ZStack {
            Sparkline(values: read, color: .accentColor, maxValue: peak, showsZeroBaseline: true)
            Sparkline(values: write,
                      color: PanelMetricColor.pink(for: colorScheme),
                      maxValue: peak,
                      fillOpacity: 0.08)
        }
        .frame(height: 30)
        .graphCeilingLabel(MetricFormat.bytesPerSec(peak))
    }

    private func rateColumn(icon: String, label: String, value: Double?, color: Color) -> some View {
        HStack(spacing: 7) {
            Image(systemName: icon)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(color)
            VStack(alignment: .leading, spacing: 1) {
                Text(value.map { MetricFormat.bytesPerSec($0) } ?? l10n.s.networkMeasuring)
                    .font(.system(size: 13.5, weight: .semibold, design: .rounded))
                    .monospacedDigit()
                    .contentTransition(.numericText())
                Text(label)
                    .font(.system(size: 10))
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private func smartBlock(disk: DiskDeviceReading, editing: Bool) -> some View {
        if !diskSMART {
            PanelHiddenItemRow(title: l10n.s.monitorItemDiskSMART,
                               systemImage: "checkmark.shield",
                               isVisible: $diskSMART)
        } else {
            VStack(alignment: .leading, spacing: 8) {
                smartHeader(disk.smart?.status, editing: editing)
                if !diskUsage {
                    diskTitleRow(disk)
                }
                if let smart = disk.smart {
                    smartTiles(smart)
                } else {
                    Text(l10n.s.diskSMARTUnavailable)
                        .font(.system(size: 10.5))
                        .foregroundStyle(.tertiary)
                }
            }
        }
    }

    private func smartHeader(_ status: String?, editing: Bool) -> some View {
        HStack(spacing: 6) {
            Text(l10n.s.monitorItemDiskSMART)
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(.tertiary)
            Spacer(minLength: 0)
            if let status {
                Text(status)
                    .font(.system(size: 10.5, weight: .semibold))
                    .foregroundStyle(smartStatusColor(status))
                    .lineLimit(1)
            }
            if editing {
                PanelInlineHideButton(isVisible: $diskSMART)
            }
        }
    }

    private func smartStatusColor(_ status: String) -> Color {
        // IOKit reports the raw "Verified" / "Failing" string.
        switch status.lowercased() {
        case "verified": return PanelMetricColor.green(for: colorScheme)
        case "failing": return PanelMetricColor.red(for: colorScheme)
        default: return .primary
        }
    }

    private func smartTiles(_ smart: DiskSMARTReading) -> some View {
        var tiles: [(label: String, value: String)] = []
        if let written = smart.totalWrittenBytes {
            tiles.append((l10n.s.diskTotalWritten, MetricFormat.diskBytesPrecise(written)))
        }
        if let read = smart.totalReadBytes {
            tiles.append((l10n.s.diskTotalRead, MetricFormat.diskBytesPrecise(read)))
        }
        if let temp = smart.temperatureCelsius {
            tiles.append((l10n.s.diskTemperature, MetricFormat.temperature(temp, unit: displayTemperatureUnit)))
        }
        if let health = smart.healthPercent {
            tiles.append((l10n.s.diskHealth, "\(health)%"))
        }
        if let cycles = smart.powerCycles {
            tiles.append((l10n.s.diskPowerCycles, "\(cycles)"))
        }
        if let hours = smart.powerOnHours {
            tiles.append((l10n.s.diskPowerOnHours, "\(hours)"))
        }
        let columns = Array(repeating: GridItem(.flexible(), spacing: 8, alignment: .topLeading), count: 3)
        return LazyVGrid(columns: columns, alignment: .leading, spacing: 8) {
            ForEach(tiles, id: \.label) { tile in
                VStack(alignment: .leading, spacing: 1) {
                    Text(tile.value)
                        .font(.system(size: 12, weight: .semibold, design: .rounded))
                        .monospacedDigit()
                        .lineLimit(1)
                        .minimumScaleFactor(0.8)
                    Text(tile.label)
                        .font(.system(size: 9.5))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .minimumScaleFactor(0.75)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityElement(children: .combine)
            }
        }
    }

    @ViewBuilder
    private func protectionBlock(disk: DiskDeviceReading, editing: Bool) -> some View {
        if !diskProtection {
            PanelHiddenItemRow(title: l10n.s.monitorItemDiskProtection,
                               systemImage: "eject",
                               isVisible: $diskProtection)
        } else {
            VStack(alignment: .leading, spacing: 6) {
                if editing {
                    blockHeader(l10n.s.monitorItemDiskProtection, editing: editing, visible: $diskProtection)
                }
                HStack(spacing: 7) {
                    Button {
                        protection.eject(disk)
                    } label: {
                        Label(l10n.s.diskEject, systemImage: "eject.fill")
                            .font(.system(size: 10.5, weight: .medium))
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .disabled(!disk.canEject || protection.state(for: disk) == .ejecting)

                    Button {
                        protection.ejectAll(disks)
                    } label: {
                        Label(l10n.s.diskEjectAll, systemImage: "eject.fill")
                            .font(.system(size: 10.5, weight: .medium))
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.small)
                    .disabled(ejectableDisks.isEmpty)
                }
                Text(disk.canEject ? ejectCaption(for: disk) : l10n.s.diskNoExternal)
                    .fixedSize(horizontal: false, vertical: true)
                    .font(.system(size: 9.5))
                    .foregroundStyle(disk.canEject ? ejectCaptionColor(for: disk) : .secondary)
                    .lineLimit(2)
            }
        }
    }

    private func ejectCaption(for disk: DiskDeviceReading) -> String {
        switch protection.state(for: disk) {
        case .ejecting: return l10n.s.diskEjecting
        case .ready: return l10n.s.diskReadyToRemove
        case .failed: return l10n.s.diskEjectFailed
        case .none: return l10n.s.diskProtectionCaption
        }
    }

    private func ejectCaptionColor(for disk: DiskDeviceReading) -> Color {
        switch protection.state(for: disk) {
        case .ready: return PanelMetricColor.green(for: colorScheme)
        case .failed: return PanelMetricColor.red(for: colorScheme)
        default: return .secondary
        }
    }

    @ViewBuilder
    private func toolsBlock(disk: DiskDeviceReading, editing: Bool) -> some View {
        if !diskTools {
            PanelHiddenItemRow(title: l10n.s.monitorItemDiskTools,
                               systemImage: "folder.badge.gearshape",
                               isVisible: $diskTools)
        } else {
            VStack(alignment: .leading, spacing: 6) {
                if editing {
                    blockHeader(l10n.s.monitorItemDiskTools, editing: editing, visible: $diskTools)
                }
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 7) { toolButtons(disk) }
                    VStack(alignment: .leading, spacing: 6) { toolButtons(disk) }
                }
            }
        }
    }

    @ViewBuilder
    private func toolButtons(_ disk: DiskDeviceReading) -> some View {
        Button {
            NSWorkspace.shared.open(URL(fileURLWithPath: disk.mountPath))
        } label: {
            Label(l10n.s.diskOpenInFinder, systemImage: "folder")
                .font(.system(size: 10.5, weight: .medium))
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .help(disk.name)
        .fixedSize()
        Button {
            if let url = URL(string: "x-apple.systempreferences:com.apple.Storage-Settings.extension") {
                NSWorkspace.shared.open(url)
            }
        } label: {
            Label(l10n.s.diskStorageSettings, systemImage: "gearshape")
                .font(.system(size: 10.5, weight: .medium))
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .fixedSize()
    }

    private func diskTitleRow(_ disk: DiskDeviceReading, trailing: String? = nil) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Image(systemName: disk.isInternal ? "internaldrive" : "externaldrive")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(.secondary)
            Text(disk.name)
                .font(.system(size: 11.5, weight: .semibold))
                .lineLimit(1)
                .truncationMode(.middle)
                .layoutPriority(1)
            Text(diskTags(disk))
                .font(.system(size: 9.5, weight: .medium))
                .foregroundStyle(.tertiary)
                .lineLimit(1)
            Spacer(minLength: 0)
            if let trailing {
                Text(trailing)
                    .font(.system(size: 11.5, weight: .semibold))
                    .monospacedDigit()
                    .contentTransition(.numericText())
            }
        }
    }

    private func diskTags(_ disk: DiskDeviceReading) -> String {
        [disk.fileSystem, disk.isInternal ? l10n.s.diskInternal : l10n.s.diskExternal]
            .compactMap { $0 }
            .joined(separator: " · ")
    }

    private func blockHeader(_ title: String, editing: Bool, visible: Binding<Bool>) -> some View {
        HStack(spacing: 6) {
            Text(title)
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(.tertiary)
            Spacer(minLength: 0)
            if editing {
                PanelInlineHideButton(isVisible: visible)
            }
        }
    }

    private var displayTemperatureUnit: TemperatureUnit {
        TemperatureUnit(rawValue: temperatureUnit) ?? .celsius
    }
}

private struct DiskSelectorEjectButton: View {
    @Environment(\.colorScheme) private var colorScheme
    let state: DiskEjectState?
    let idleHelp: String
    let busyHelp: String
    let readyHelp: String
    let failedHelp: String
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            icon
                .frame(width: 18, height: 18)
                .background(Circle().fill(Color.primary.opacity(hovering && !busy ? 0.1 : 0)))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(busy)
        .onHover { hovering = $0 }
        .help(help)
        .accessibilityLabel(help)
        .animation(.easeOut(duration: 0.12), value: hovering)
    }

    @ViewBuilder
    private var icon: some View {
        switch state {
        case .ejecting:
            ProgressView()
                .controlSize(.small)
                .scaleEffect(0.55)
        case .ready:
            Image(systemName: "checkmark")
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(PanelMetricColor.green(for: colorScheme))
        case .failed:
            Image(systemName: "eject.fill")
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(PanelMetricColor.red(for: colorScheme))
        case .none:
            Image(systemName: "eject.fill")
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(hovering ? .primary : .secondary)
        }
    }

    private var busy: Bool {
        state == .ejecting
    }

    private var help: String {
        switch state {
        case .ejecting: return busyHelp
        case .ready: return readyHelp
        case .failed: return failedHelp
        case .none: return idleHelp
        }
    }
}

private struct DiskUsageBar: View {
    @Environment(\.colorScheme) private var colorScheme
    let fraction: Double

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule().fill(Color.primary.opacity(0.08))
                Capsule()
                    .fill(color)
                    .frame(width: max(3, proxy.size.width * min(1, max(0, fraction))))
                    .animation(.easeOut(duration: 0.4), value: fraction)
            }
        }
        .frame(height: 6)
    }

    private var color: Color {
        switch fraction {
        case ..<0.75: return .accentColor
        case ..<0.9: return PanelMetricColor.yellow(for: colorScheme)
        default: return PanelMetricColor.red(for: colorScheme)
        }
    }
}
