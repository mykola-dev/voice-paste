//! Shared tray commands and presentation policy; no native handles or GPUI.
use voicepaste_core::State;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TrayAction {
    Toggle,
    Settings,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MenuItem {
    Command {
        action: TrayAction,
        label: &'static str,
        enabled: bool,
    },
    Separator,
}

pub(super) fn items(state: State) -> [MenuItem; 5] {
    [
        MenuItem::Command {
            action: TrayAction::Toggle,
            label: if matches!(state, State::Recording(_)) {
                "Stop recording"
            } else {
                "Start recording"
            },
            enabled: !matches!(state, State::Transcribing(_)),
        },
        MenuItem::Separator,
        MenuItem::Command {
            action: TrayAction::Settings,
            label: "Settings...",
            enabled: true,
        },
        MenuItem::Separator,
        MenuItem::Command {
            action: TrayAction::Quit,
            label: "Quit",
            enabled: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use voicepaste_core::SessionId;

    #[test]
    fn recording_command_preserves_labels_and_busy_policy() {
        for (state, label, enabled) in [
            (State::Idle, "Start recording", true),
            (State::Recording(SessionId(1)), "Stop recording", true),
            (State::Transcribing(SessionId(1)), "Start recording", false),
        ] {
            assert_eq!(
                items(state),
                [
                    MenuItem::Command {
                        action: TrayAction::Toggle,
                        label,
                        enabled
                    },
                    MenuItem::Separator,
                    MenuItem::Command {
                        action: TrayAction::Settings,
                        label: "Settings...",
                        enabled: true
                    },
                    MenuItem::Separator,
                    MenuItem::Command {
                        action: TrayAction::Quit,
                        label: "Quit",
                        enabled: true
                    },
                ]
            );
        }
    }
}
