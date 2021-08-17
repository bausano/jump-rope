use crate::frequency_tracker::FrequencyTracker;
use bevy::prelude::*;
use bevy_prototype_lyon::prelude::*;
use std::sync::Arc;

struct Tracker(Arc<FrequencyTracker>);

pub fn start(tracker: Arc<FrequencyTracker>) {
    App::build()
        .insert_resource(Msaa { samples: 8 })
        .insert_resource(Tracker(tracker))
        .add_plugins(DefaultPlugins)
        .add_plugin(ShapePlugin)
        .add_startup_system(setup.system())
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn_bundle(OrthographicCameraBundle::new_2d());

    // TODO: draw and update UI
}
