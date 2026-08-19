use bevy::prelude::*;

use crate::voxel::SelectedBlock;

#[derive(Component)]
struct SelectedBlockLabel;

#[derive(Component)]
struct PausedLabel;

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, setup_hud)
        .add_systems(Update, (update_selected_block_label, update_paused_label));
}

/// A fixed dot in the center of the screen (crosshair), a label showing
/// which block left-click will place, and a "cursor released" hint — small
/// additions, but without them there's no on-screen indication of what
/// mining/placing will target or why input has stopped responding after
/// Esc.
fn setup_hud(mut commands: Commands) {
    // Crosshair
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                Node {
                    width: Val::Px(4.0),
                    height: Val::Px(4.0),
                    ..default()
                },
                BackgroundColor(Color::WHITE),
            ));
        });

    // Selected block / controls hint, bottom-left
    commands.spawn((
        Text::new(format!(
            "Block: {}  (1-7 select, LMB place, RMB mine)",
            SelectedBlock::default().0.name()
        )),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(12.0),
            bottom: Val::Px(12.0),
            ..default()
        },
        SelectedBlockLabel,
    ));

    // "Paused" hint, top-center — only visible while the cursor is released
    commands.spawn((
        Text::new("Cursor released — click to resume"),
        TextFont {
            font_size: FontSize::Px(18.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(0.0),
            right: Val::Px(0.0),
            justify_self: JustifySelf::Center,
            ..default()
        },
        Visibility::Hidden,
        PausedLabel,
    ));
}

fn update_selected_block_label(
    selected: Res<SelectedBlock>,
    mut label_query: Query<&mut Text, With<SelectedBlockLabel>>,
) {
    if !selected.is_changed() {
        return;
    }
    let Ok(mut text) = label_query.single_mut() else {
        return;
    };
    **text = format!(
        "Block: {}  (1-7 select, LMB place, RMB mine)",
        selected.0.name()
    );
}

fn update_paused_label(
    cursor_options: Single<&bevy::window::CursorOptions>,
    mut label_query: Query<&mut Visibility, With<PausedLabel>>,
) {
    let Ok(mut visibility) = label_query.single_mut() else {
        return;
    };
    let captured = cursor_options.grab_mode != bevy::window::CursorGrabMode::None;
    *visibility = if captured {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };
}
