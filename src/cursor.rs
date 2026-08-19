use bevy::{
    prelude::*,
    window::{CursorGrabMode, CursorOptions, WindowFocused},
};

pub fn plugin(app: &mut App) {
    app.add_systems(Update, (toggle_cursor_capture, recapture_on_window_focus));
}

/// Run condition other systems (movement, camera, mining, placing) gate on,
/// so nothing reacts to input while the cursor is released — the game is
/// effectively paused behind an OS-level menu/alt-tab.
pub fn cursor_captured(cursor_options: Single<&CursorOptions>) -> bool {
    cursor_options.grab_mode != CursorGrabMode::None
}

/// Esc releases the cursor (shows it, stops locking it to the window) so you
/// can get to a menu, alt-tab, etc. Clicking back in the window re-grabs and
/// re-hides it. The cursor starts grabbed already via `primary_cursor_options`
/// in `main()`, so this only needs to handle the release/re-grab toggle.
fn toggle_cursor_capture(
    mut cursor_options: Single<&mut CursorOptions>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        release(&mut cursor_options);
    } else if mouse_button.just_pressed(MouseButton::Left)
        && cursor_options.grab_mode == CursorGrabMode::None
    {
        capture(&mut cursor_options);
    }
}

/// Fixes the "game ignores the mouse right after Esc until I click the
/// window" issue: on several platforms (notably Windows and X11), the very
/// first click on an unfocused window is consumed by the OS to refocus it
/// and never reaches the app as a `MouseButton::Left` press. That means
/// `toggle_cursor_capture` alone needs *two* clicks after Esc — one to focus
/// the window (silently swallowed), one that actually registers — so the
/// game looks unresponsive in between.
///
/// Watching `WindowFocused` instead re-grabs the cursor the instant the OS
/// hands focus back to the window, regardless of whether that focus change
/// came with a mouse click bevy could see. So the first click that lands on
/// the window works immediately, and alt-tabbing back also just works
/// without needing a click at all.
fn recapture_on_window_focus(
    mut focus_events: MessageReader<WindowFocused>,
    mut cursor_options: Single<&mut CursorOptions>,
) {
    for event in focus_events.read() {
        if event.focused && cursor_options.grab_mode == CursorGrabMode::None {
            capture(&mut cursor_options);
        }
    }
}

fn capture(cursor_options: &mut CursorOptions) {
    cursor_options.visible = false;
    cursor_options.grab_mode = CursorGrabMode::Locked;
}

fn release(cursor_options: &mut CursorOptions) {
    cursor_options.visible = true;
    cursor_options.grab_mode = CursorGrabMode::None;
}
