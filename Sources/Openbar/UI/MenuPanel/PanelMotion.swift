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
}

/// Content that settles in when it appears: it fades up from a few points
/// off and out of a light blur. Only the content's own opacity, offset and
/// blur move. Animating the layout instead (a transition inside
/// `withAnimation`) springs the panel's height, and the hosting view in the
/// panel's scroll area then draws everything shifted down for a few frames.
private struct PanelReveal: ViewModifier {
    let offset: CGSize
    let blur: CGFloat
    @State private var shown = false

    func body(content: Content) -> some View {
        content
            .opacity(shown ? 1 : 0)
            .offset(shown ? .zero : offset)
            .blur(radius: shown ? 0 : blur)
            .onAppear {
                guard !shown else { return }
                withAnimation(PanelMotion.tab) { shown = true }
            }
    }
}

extension View {
    /// A section arriving from a tab: `direction` is +1 when the tabs moved
    /// right and -1 when they moved left.
    func panelSectionReveal(direction: CGFloat) -> some View {
        let still = PanelMotion.reduceMotion
        return modifier(PanelReveal(offset: still ? .zero : CGSize(width: 14 * direction, height: 0),
                                    blur: still ? 0 : 3))
    }

    /// Rows opening under a disclosure drop a few points into place.
    func panelReveal() -> some View {
        modifier(PanelReveal(offset: PanelMotion.reduceMotion ? .zero : CGSize(width: 0, height: -6),
                             blur: 0))
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
            .animation(PanelMotion.press) {
                $0.opacity(isEnabled ? (configuration.isPressed ? 0.78 : 1) : 0.45)
                    .scaleEffect(PanelMotion.pressScale(configuration.isPressed))
            }
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
