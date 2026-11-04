import AppKit
import SwiftUI

/// The panel's motion vocabulary. The Windows app takes the same feel from
/// Animate UI's primitives (Highlight, Tabs, Collapsible, Button, Sliding
/// Number); these are their SwiftUI counterparts, tuned to the same springs so
/// both apps move alike. Every curve falls back to a short fade when the user
/// turns on Reduce Motion.
enum PanelMotion {
    static var reduceMotion: Bool {
        NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
    }

    /// The tab highlight sliding between sections, and the section swap.
    static var tab: Animation {
        reduceMotion ? .easeOut(duration: 0.12) : .spring(response: 0.34, dampingFraction: 0.84)
    }

    /// Opening or closing a group: height and content together.
    static var disclosure: Animation {
        reduceMotion ? .easeOut(duration: 0.12) : .spring(response: 0.3, dampingFraction: 0.88)
    }

    /// A selection moving inside a small control, like a chip lighting up.
    static var selection: Animation {
        reduceMotion ? .easeOut(duration: 0.1) : .spring(response: 0.26, dampingFraction: 0.82)
    }

    /// The press-in of a button, quick enough to follow the pointer.
    static var press: Animation {
        reduceMotion ? .easeOut(duration: 0.08) : .spring(response: 0.2, dampingFraction: 0.68)
    }

    /// Live readings rolling to their next value.
    static var value: Animation {
        reduceMotion ? .linear(duration: 0.01) : .smooth(duration: 0.35)
    }

    /// How far a pressed button sinks. Reduce Motion keeps it still and lets
    /// the dimming carry the feedback.
    static func pressScale(_ pressed: Bool) -> CGFloat {
        pressed && !reduceMotion ? 0.96 : 1
    }

    /// The incoming section slides a few points in from the side the user
    /// moved toward and sharpens out of a light blur; the outgoing one only
    /// fades, so the two never read as a shuffle.
    static func sectionTransition(direction: CGFloat) -> AnyTransition {
        if reduceMotion { return .opacity }
        return .asymmetric(
            insertion: .modifier(
                active: PanelSlideBlur(offset: 14 * direction, blur: 3, opacity: 0),
                identity: PanelSlideBlur(offset: 0, blur: 0, opacity: 1)
            ),
            removal: .opacity.animation(.easeOut(duration: 0.12))
        )
    }

    /// Content that appears under a disclosure header: it drops a few points
    /// into place while the group grows around it.
    static var revealTransition: AnyTransition {
        if reduceMotion { return .opacity }
        return .asymmetric(
            insertion: .modifier(
                active: PanelDropIn(offset: -6, opacity: 0),
                identity: PanelDropIn(offset: 0, opacity: 1)
            ),
            removal: .opacity.animation(.easeOut(duration: 0.1))
        )
    }
}

private struct PanelSlideBlur: ViewModifier {
    var offset: CGFloat
    var blur: CGFloat
    var opacity: Double

    func body(content: Content) -> some View {
        content
            .offset(x: offset)
            .blur(radius: blur)
            .opacity(opacity)
    }
}

private struct PanelDropIn: ViewModifier {
    var offset: CGFloat
    var opacity: Double

    func body(content: Content) -> some View {
        content
            .offset(y: offset)
            .opacity(opacity)
    }
}

/// The plain button style with a press: the label sinks slightly and springs
/// back, the way Animate UI's Button primitive taps. Idle it looks exactly
/// like `.plain`, including the dimmed disabled state.
struct PanelPressButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        PanelPressBody(configuration: configuration)
    }
}

private struct PanelPressBody: View {
    let configuration: ButtonStyleConfiguration
    @Environment(\.isEnabled) private var isEnabled

    var body: some View {
        configuration.label
            .opacity(isEnabled ? (configuration.isPressed ? 0.78 : 1) : 0.45)
            .scaleEffect(PanelMotion.pressScale(configuration.isPressed))
            .animation(PanelMotion.press, value: configuration.isPressed)
    }
}

extension ButtonStyle where Self == PanelPressButtonStyle {
    static var panelPress: PanelPressButtonStyle { PanelPressButtonStyle() }
}

extension View {
    /// A live number that rolls digit by digit when it changes, like Animate
    /// UI's Sliding Number. `value` is whatever the text is built from.
    func panelRollingValue<V: Equatable>(_ value: V) -> some View {
        contentTransition(.numericText())
            .animation(PanelMotion.value, value: value)
    }
}
