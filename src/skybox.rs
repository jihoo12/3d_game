use bevy::{
    prelude::*,
    render::render_resource::{TextureViewDescriptor, TextureViewDimension},
};

#[derive(Resource)]
pub struct SceneSkybox {
    pub handle: Handle<Image>,
    pub is_loaded: bool,
}

pub fn plugin(app: &mut App) {
    app.add_systems(Update, check_skybox_loaded);
}

fn check_skybox_loaded(
    mut skybox_res: ResMut<SceneSkybox>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
) {
    if !skybox_res.is_loaded && asset_server.load_state(&skybox_res.handle).is_loaded() {
        skybox_res.is_loaded = true;
        if let Some(mut image) = images.get_mut(&skybox_res.handle) {
            if image.texture_descriptor.array_layer_count() == 1 {
                let layers = image.height() / image.width();
                image
                    .reinterpret_stacked_2d_as_array(layers)
                    .expect("Failed to reinterpret skybox image as an array texture");

                image.texture_view_descriptor = Some(TextureViewDescriptor {
                    dimension: Some(TextureViewDimension::Cube),
                    ..default()
                });
            }
        }
    }
}
